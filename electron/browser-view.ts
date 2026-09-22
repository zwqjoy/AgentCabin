/**
 * One embedded browser page, owned by the Electron main process.
 *
 * The renderer reserves a rectangle (the "hole") and reports it. The native
 * WebContentsView is then positioned above the renderer at that rectangle.
 */
import { WebContentsView, type BaseWindow, type BrowserWindow } from "electron";
import { classifyNavigation } from "./browser-navigation-policy";
import type { Rect, SurfaceViewport } from "./browser-view-geometry";

const MIN_AUTO_PAGE_ZOOM = 0.25;
const MAX_AUTO_PAGE_ZOOM = 1;
const PAGE_WIDTH_EPSILON = 8;

export interface BrowserViewState {
  viewId: string;
  targetId: string;
  url: string;
  title: string;
  isLoading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface BrowserViewOptions {
  viewId: string;
  partition: string;
  allowedHosts: string[];
  onStateChange: (state: BrowserViewState) => void;
  onNavigationAsk: (host: string, url: string) => Promise<boolean>;
  log?: (message: string, detail?: unknown) => void;
}

export class EmbeddedBrowserView {
  private readonly view: WebContentsView;
  private state: BrowserViewState;
  private allowedHosts: string[];
  private bounds: Rect = { x: 0, y: 0, width: 0, height: 0 };
  private readonly approvedNavigations = new Set<string>();
  private measuredPageWidth: number | null = null;
  private autoPageZoom = 1;
  private pageZoomUpdateInFlight = false;
  private pageZoomUpdateQueued = false;

  constructor(
    private readonly window: BrowserWindow | BaseWindow,
    private readonly options: BrowserViewOptions,
  ) {
    this.allowedHosts = [...options.allowedHosts];
    this.view = new WebContentsView({
      webPreferences: {
        partition: options.partition,
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        webviewTag: false,
        spellcheck: false,
      },
    });
    this.view.webContents.setZoomMode("isolated");
    this.view.webContents.setZoomFactor(1);
    this.state = {
      viewId: options.viewId,
      targetId: "",
      url: "",
      title: "",
      isLoading: false,
      canGoBack: false,
      canGoForward: false,
    };
    this.window.contentView.addChildView(this.view);
    this.view.setBounds(this.bounds);
    this.wireEvents();
  }

  get webContents() {
    return this.view.webContents;
  }

  getState(): BrowserViewState {
    return { ...this.state };
  }

  setBounds(bounds: Rect): void {
    this.bounds = { ...bounds };
    this.view.setBounds(this.bounds);
    void this.fitPageToBounds();
  }

  setVisible(visible: boolean): void {
    this.view.setVisible(visible);
    if (visible) this.view.setBounds(this.bounds);
  }

  async loadURL(url: string): Promise<void> {
    this.approvedNavigations.add(url);
    this.resetPageZoom();
    await this.view.webContents.loadURL(url);
  }

  goBack(): void {
    if (this.view.webContents.navigationHistory.canGoBack()) {
      this.view.webContents.navigationHistory.goBack();
    }
  }

  goForward(): void {
    if (this.view.webContents.navigationHistory.canGoForward()) {
      this.view.webContents.navigationHistory.goForward();
    }
  }

  reload(): void {
    this.view.webContents.reload();
  }

  stop(): void {
    this.view.webContents.stop();
  }

  destroy(): void {
    if (this.view.webContents.debugger.isAttached()) {
      try {
        this.view.webContents.debugger.detach();
      } catch {
        // The debugger may already have detached during a renderer crash.
      }
    }
    this.window.contentView.removeChildView(this.view);
    this.view.webContents.close();
  }

  async resolveTargetId(): Promise<string> {
    if (!this.view.webContents.debugger.isAttached()) {
      this.view.webContents.debugger.attach("1.3");
    }
    const info = (await this.view.webContents.debugger.sendCommand("Target.getTargetInfo")) as {
      targetInfo?: { targetId?: string };
    };
    this.state.targetId = String(info?.targetInfo?.targetId ?? "");
    return this.state.targetId;
  }

  /**
   * Keep fixed-width desktop pages usable inside the narrow inspector pane.
   * Responsive pages remain at 100%; only horizontal overflow is scaled down.
   */
  async fitPageToBounds(): Promise<void> {
    if (!this.view.webContents.debugger.isAttached() || this.bounds.width <= 0) return;
    if (this.pageZoomUpdateInFlight) {
      this.pageZoomUpdateQueued = true;
      return;
    }

    this.pageZoomUpdateInFlight = true;
    try {
      if (this.measuredPageWidth === null) {
        this.view.webContents.setZoomFactor(1);
        const result = (await this.view.webContents.debugger.sendCommand("Runtime.evaluate", {
          expression: `(() => {
            const root = document.documentElement;
            const body = document.body;
            return Math.max(
              root?.scrollWidth || 0,
              body?.scrollWidth || 0,
              root?.clientWidth || 0,
              window.innerWidth || 0,
            );
          })()`,
          returnByValue: true,
        })) as { result?: { value?: unknown } };
        const pageWidth = Number(result?.result?.value);
        if (Number.isFinite(pageWidth) && pageWidth > 0) {
          this.measuredPageWidth = pageWidth;
        }
      }

      const pageWidth = this.measuredPageWidth;
      const nextZoom =
        pageWidth !== null && pageWidth > this.bounds.width + PAGE_WIDTH_EPSILON
          ? Math.max(MIN_AUTO_PAGE_ZOOM, Math.min(MAX_AUTO_PAGE_ZOOM, this.bounds.width / pageWidth))
          : MAX_AUTO_PAGE_ZOOM;
      if (Math.abs(nextZoom - this.autoPageZoom) > 0.01) {
        this.view.webContents.setZoomFactor(nextZoom);
        this.autoPageZoom = nextZoom;
      }
    } catch (error) {
      this.options.log?.("browser view: automatic page zoom failed", error);
    } finally {
      this.pageZoomUpdateInFlight = false;
      if (this.pageZoomUpdateQueued) {
        this.pageZoomUpdateQueued = false;
        void this.fitPageToBounds();
      }
    }
  }

  setAllowedHosts(hosts: string[]): void {
    this.allowedHosts = [...hosts];
  }

  private wireEvents(): void {
    const contents = this.view.webContents;

    contents.setWindowOpenHandler(({ url }) => {
      this.options.log?.("browser view: denied window.open", url);
      return { action: "deny" };
    });

    contents.on("will-navigate", (event, url) => {
      if (this.approvedNavigations.delete(url)) return;

      const decision = classifyNavigation({
        fromUrl: this.state.url,
        toUrl: url,
        allowedHosts: this.allowedHosts,
      });
      if (decision.action === "allow") return;
      event.preventDefault();
      if (decision.action === "block") {
        this.options.log?.("browser view: blocked navigation", { url, reason: decision.reason });
        this.emitState();
        return;
      }
      void this.options.onNavigationAsk(decision.host, url).then((approved) => {
        if (approved) {
          this.approvedNavigations.add(url);
          void contents.loadURL(url);
        } else {
          this.emitState();
        }
      });
    });

    const push = () => this.emitState();
    contents.on("did-start-loading", push);
    contents.on("did-stop-loading", push);
    contents.on("did-navigate", () => {
      this.resetPageZoom();
      push();
    });
    contents.on("did-navigate-in-page", () => {
      this.resetPageZoom();
      push();
    });
    contents.on("did-stop-loading", () => {
      void this.fitPageToBounds();
    });
    contents.on("page-title-updated", push);
    contents.on("did-fail-load", (_event, errorCode, errorDescription, validatedURL) => {
      this.options.log?.("browser view: load failed", {
        errorCode,
        errorDescription,
        validatedURL,
      });
      push();
    });
  }

  private emitState(): void {
    const contents = this.view.webContents;
    this.state = {
      ...this.state,
      url: contents.getURL(),
      title: contents.getTitle(),
      isLoading: contents.isLoading(),
      canGoBack: contents.navigationHistory.canGoBack(),
      canGoForward: contents.navigationHistory.canGoForward(),
    };
    this.options.onStateChange(this.getState());
  }

  private resetPageZoom(): void {
    this.measuredPageWidth = null;
    this.autoPageZoom = 1;
    this.view.webContents.setZoomFactor(1);
  }
}

export function windowViewport(window: BrowserWindow | BaseWindow): SurfaceViewport {
  const [width, height] = window.getContentSize();
  return { width, height };
}
