/**
 * CoreProcessManager — owns the headless Rust core child process.
 *
 * Spawns `AgentCabin --core-server`, parses the AGENTCABIN_CORE_READY line
 * from stdout to discover the loopback port and auth token, then forwards
 * renderer `core:invoke` calls to POST /invoke and streams core events from
 * GET /events (SSE) into the window broadcast.
 *
 * Lifecycle: auto-restarts with backoff if the child dies unexpectedly;
 * graceful SIGINT shutdown on app quit.
 */
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import readline from "node:readline";
import path from "node:path";
import { app } from "electron";

const READY_PREFIX = "AGENTCABIN_CORE_READY ";

interface CoreStartupInfo {
  port: number;
  token: string;
  pid: number;
}

export type CoreEventHandler = (event: string, payload: unknown) => void;

export class CoreProcessManager {
  private child: ChildProcess | null = null;
  private info: CoreStartupInfo | null = null;
  private stopping = false;
  private restartAttempts = 0;
  private restartTimer: NodeJS.Timeout | null = null;
  private abortEvents: AbortController | null = null;
  private startPromise: Promise<void> | null = null;

  constructor(
    private readonly onEvent: CoreEventHandler,
    private readonly log: (msg: string) => void = (m) => console.log(m),
  ) {}

  /** Path to the Rust core binary. AGENTCABIN_CORE_BIN overrides. */
  private coreBinary(): string {
    if (process.env.AGENTCABIN_CORE_BIN) return process.env.AGENTCABIN_CORE_BIN;

    const exeName = process.platform === "win32" ? "AgentCabin.exe" : "AgentCabin";

    if (app.isPackaged) {
      const inBinDir = path.join(process.resourcesPath, "bin", exeName);
      if (existsSync(inBinDir)) return inBinDir;
      const inResourcesRoot = path.join(process.resourcesPath, exeName);
      if (existsSync(inResourcesRoot)) return inResourcesRoot;
    }

    // Development always uses debug so `npm run electron:dev` cannot reuse a
    // stale release binary after Rust/runtime changes.
    const targetDir = path.join(__dirname, "..", "..", "src-tauri", "target");
    const releaseBin = path.join(targetDir, "release", exeName);
    const debugBin = path.join(targetDir, "debug", exeName);
    if (!app.isPackaged && existsSync(debugBin)) return debugBin;
    const releaseExists = existsSync(releaseBin);
    const debugExists = existsSync(debugBin);

    if (releaseExists && debugExists) {
      try {
        const releaseMtime = statSync(releaseBin).mtimeMs;
        const debugMtime = statSync(debugBin).mtimeMs;
        return releaseMtime >= debugMtime ? releaseBin : debugBin;
      } catch {
        return releaseBin;
      }
    }
    if (releaseExists) return releaseBin;
    return debugBin;
  }

  getToken(): string | null {
    return this.info?.token ?? null;
  }

  isReady(): boolean {
    return this.info !== null && this.child !== null && !this.child.killed;
  }

  /** Spawn the core process and wait for the READY line. Resolves when usable. */
  async start(): Promise<void> {
    if (this.isReady()) return;
    if (this.startPromise) return this.startPromise;

    const pending = this.startInternal();
    this.startPromise = pending;
    try {
      await pending;
    } finally {
      if (this.startPromise === pending) this.startPromise = null;
    }
  }

  private async startInternal(): Promise<void> {
    if (this.isReady()) return;
    this.stopping = false;

    const bin = this.coreBinary();
    if (!existsSync(bin)) {
      throw new Error(
        `AgentCabin core binary not found at ${bin}. Build it with: cargo build --bin AgentCabin (in src-tauri), or set AGENTCABIN_CORE_BIN.`,
      );
    }

    this.log(`[core-process] spawning ${bin}`);
    const child = spawn(bin, ["--core-server"], {
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        AGENTCABIN_PACKAGED: app.isPackaged ? "1" : process.env.AGENTCABIN_PACKAGED,
        AGENTCABIN_RUNTIME_ROOT: app.isPackaged ? path.join(process.resourcesPath, "runtimes") : process.env.AGENTCABIN_RUNTIME_ROOT,
        RUST_LOG: process.env.RUST_LOG ?? "warn",
      },
    });
    this.child = child;

    child.on("exit", (code, signal) => {
      this.log(`[core-process] exited code=${code} signal=${signal}`);
      if (this.child === child) {
        this.child = null;
        this.info = null;
        this.abortEvents?.abort();
        this.abortEvents = null;
      }
      if (!this.stopping) {
        this.scheduleRestart();
      }
    });
    child.on("error", (err) => {
      this.log(`[core-process] spawn error: ${err.message}`);
    });

    // stderr → console for diagnostics.
    child.stderr?.on("data", (chunk: Buffer) => {
      const text = chunk.toString().trimEnd();
      if (text) this.log(`[core] ${text}`);
    });

    // stdout: wait for the READY line; log the rest.
    const ready = new Promise<CoreStartupInfo>((resolve, reject) => {
      const rl = readline.createInterface({ input: child.stdout! });
      const timer = setTimeout(() => {
        reject(new Error("core process did not become ready in 30s"));
      }, 30_000);
      rl.on("line", (line) => {
        if (line.startsWith(READY_PREFIX)) {
          clearTimeout(timer);
          try {
            resolve(JSON.parse(line.slice(READY_PREFIX.length)) as CoreStartupInfo);
          } catch (e) {
            reject(new Error(`failed to parse core READY line: ${e}`));
          }
        } else if (line.trim()) {
          this.log(`[core] ${line}`);
        }
      });
      rl.on("close", () => clearTimeout(timer));
      child.once("exit", () => {
        clearTimeout(timer);
        reject(new Error("core process exited before becoming ready"));
      });
    });

    this.info = await ready;
    this.restartAttempts = 0;
    this.log(`[core-process] ready on 127.0.0.1:${this.info.port} (pid ${this.info.pid})`);
    void this.streamEvents();
  }

  /** Ensure the core is ready before an operation that cannot be dropped. */
  async invokeWhenReady(method: string, params: unknown): Promise<unknown> {
    if (!this.isReady()) await this.start();
    return this.invoke(method, params);
  }

  private scheduleRestart(): void {
    if (this.stopping || this.restartTimer) return;
    const delay = Math.min(1000 * 2 ** this.restartAttempts, 10_000);
    this.restartAttempts += 1;
    this.log(`[core-process] restarting in ${delay}ms (attempt ${this.restartAttempts})`);
    this.restartTimer = setTimeout(() => {
      this.restartTimer = null;
      this.start().catch((e) => this.log(`[core-process] restart failed: ${e.message}`));
    }, delay);
  }

  /** Forward POST /invoke to the core. Throws while the core is unavailable. */
  async invoke(method: string, params: unknown): Promise<unknown> {
    if (!this.info) {
      throw new Error("AgentCabin core is starting — retry in a moment");
    }
    const { port, token } = this.info;
    const res = await fetch(`http://127.0.0.1:${port}/invoke`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ method, params: params ?? {} }),
      signal: AbortSignal.timeout(120_000),
    });
    const body = (await res.json().catch(() => null)) as {
      result?: unknown;
      error?: string;
    } | null;
    if (!res.ok) {
      throw new Error(body?.error ?? `core invoke failed (HTTP ${res.status})`);
    }
    return body?.result;
  }

  /** GET /events SSE → onEvent(event, payload) for every envelope. */
  private async streamEvents(): Promise<void> {
    if (!this.info) return;
    const { port, token } = this.info;
    const abort = new AbortController();
    this.abortEvents = abort;

    try {
      const res = await fetch(
        `http://127.0.0.1:${port}/events?token=${encodeURIComponent(token)}`,
        {
          signal: abort.signal,
          headers: { accept: "text/event-stream" },
        },
      );
      if (!res.ok || !res.body) {
        throw new Error(`events stream HTTP ${res.status}`);
      }
      let buffer = "";
      for await (const chunk of res.body as unknown as AsyncIterable<Uint8Array>) {
        buffer += new TextDecoder().decode(chunk);
        let idx: number;
        while ((idx = buffer.indexOf("\n\n")) !== -1) {
          const frame = buffer.slice(0, idx);
          buffer = buffer.slice(idx + 2);
          for (const raw of frame.split("\n")) {
            const line = raw.trim();
            if (!line.startsWith("data:")) continue;
            try {
              const envelope = JSON.parse(line.slice(5).trim()) as {
                event?: string;
                payload?: unknown;
                seq?: number | null;
              };
              if (envelope.event) {
                // Mirror websocket.ts: inject _seq into bus-event payloads so
                // session-store checkpointing is transport-agnostic (Tauri
                // live events carry the same field).
                if (
                  envelope.seq != null &&
                  envelope.payload &&
                  typeof envelope.payload === "object" &&
                  !Array.isArray(envelope.payload) &&
                  (envelope.payload as Record<string, unknown>)._seq === undefined
                ) {
                  (envelope.payload as Record<string, unknown>)._seq = envelope.seq;
                }
                this.onEvent(envelope.event, envelope.payload);
              }
            } catch {
              // Malformed frame — skip.
            }
          }
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (!abort.signal.aborted) {
        this.log(`[core-process] events stream ended: ${msg}`);
      }
    }
  }

  /** Graceful shutdown: SIGINT triggers the core's graceful path. */
  async stop(): Promise<void> {
    this.stopping = true;
    if (this.restartTimer) {
      clearTimeout(this.restartTimer);
      this.restartTimer = null;
    }
    this.abortEvents?.abort();
    this.abortEvents = null;
    const child = this.child;
    if (!child || child.killed) return;
    await new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        child.kill("SIGKILL");
        resolve();
      }, 5000);
      child.once("exit", () => {
        clearTimeout(timer);
        resolve();
      });
      child.kill("SIGINT");
    });
    this.child = null;
    this.info = null;
    this.log("[core-process] stopped");
  }
}
