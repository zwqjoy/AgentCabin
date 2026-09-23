<p align="center">
  <img src="static/logo-text.png" width="360" alt="AgentCabin">
</p>

<p align="center">
  <strong>AgentCabin Desktop — Local-first AI workbench powered by Pi Agent</strong>
</p>

<p align="center">
  An open-source Electron + Rust desktop application powered by <a href="https://github.com/earendil-works/pi">Pi Coding Agent</a> — providing Code and Work modes for interactive development, autonomous workflows, and local-first AI paired programming.
</p>

<p align="center">
  Looking for a <strong>Pi desktop</strong> or an <strong>AI coding agent workspace</strong>? AgentCabin is built for that use case.
</p>

<p align="center">
  <a href="#what-is-agentcabin-desktop">What is AgentCabin?</a> &middot;
  <a href="#key-capabilities">Capabilities</a> &middot;
  <a href="#quick-start">Quick Start</a> &middot;
  <a href="#supported-providers">Providers</a> &middot;
  <a href="#architecture">Architecture</a> &middot;
  <a href="#license">License</a>
</p>

<p align="center">
  <b>English</b> | <a href="README.zh-CN.md">简体中文</a>
</p>

---

<p align="center">
  <img src="static/screenshot.png" width="800" alt="AgentCabin Screenshot">
</p>

## What is AgentCabin Desktop?

AgentCabin Desktop brings AI coding agents out of scattered terminal sessions and into one persistent desktop workspace powered by Pi Agent. It combines chat, files, tool output, browser previews, session history, runtime management, and long-running Work tasks in a native app while keeping your project data **local by default**.

It is designed for people who want a visual desktop frontend for Pi Agent and compatible AI model providers without moving their repositories into a hosted service. Remote model APIs (including DeepSeek, Anthropic, OpenAI, and compatible endpoints) still require network access; AgentCabin itself has no cloud backend.

Common search terms: `AgentCabin`, `Pi`, `Pi Agent`, `Pi Coding Agent`, `Pi desktop`, `AI agent desktop`, `AI coding agent desktop`, `Electron coding agent`, and `local-first coding workspace`.

## Search keywords

AgentCabin is an open-source desktop app for Pi Coding Agent. It is a Pi desktop, Electron AI coding agent, and local-first developer workspace.

### At a glance

| | Details |
|---|---|
| Product | AgentCabin Desktop |
| Category | Open-source AI coding agent desktop / local-first developer workspace |
| Agent Engine | Pi Coding Agent |
| Modes | Code for interactive development; Work for durable tasks, approvals, recovery, artifacts, and scheduling |
| Runtime Closure | Node.js 22.19.0, Pi 0.85.1, pnpm 10.15.0 |
| Stack | Electron, Svelte 5, SvelteKit, Rust, TypeScript |
| License | Apache License 2.0 |

> **Packaged builds include the application-owned runtime closure.**
>
> The current closure pins Node.js 22.19.0, Pi 0.85.1, pnpm 10.15.0, and the Pi extensions required for the Code/Work experience. Development builds can also use the prepared local runtime tree.
>
> The official packaged target is **macOS on Apple Silicon** (`arm64`), with a minimum macOS version of 13.0. Windows and Linux remain source-level / community targets.

**Core principle**: Wrap the CLI, surface the work, keep it local.

## Key Capabilities

### What the CLI doesn't give you

| Capability | What AgentCabin adds |
|------------|---------------------|
| **Visual Tool Cards** | Every tool call (Read, Edit, Bash, Grep, Write, WebFetch, …) rendered as an inline card with syntax-highlighted diffs, structured output, and one-click copy |
| **Run History & Replay** | Browse all past sessions, full event replay, resume / fork from any point, archive or permanently delete chats |
| **Remote Browser Access** | Embedded web server for browser-based access over LAN or HTTP tunnels (ngrok / cloudflared) |
| **File Explorer** | Browse and edit project files with syntax highlighting, markdown preview, image preview, and git diff view |
| **Memory Editor** | Create and edit CLAUDE.md, project-scoped and user-scoped memory files with live preview |
| **Agent Management** | Visual editor to create, edit, and manage custom agent definitions (.md files) with form and source modes |
| **Permission Rules** | Manage CLI permission allow/deny rules at user and project level with a visual rule editor |
| **Usage Analytics** | Per-model token breakdown, cost tracking, daily heatmap, stacked model chart, session-level stats |
| **Activity Monitor** | Real-time hook event stream, tool activity timeline, file tracking panel, subagent tracking with nested tool cards |
| **MCP Management** | Discover MCP servers, view per-server status, reconnect / toggle from a panel |
| **Inline Permissions** | Rich permission review UI with batch Allow/Deny panel, CLI-suggested "Always Allow" rules, and AskUserQuestion rendering |
| **Rewind** | Checkpoint and selectively revert file changes with dry-run preview |
| **Remote Hosts** | Configure SSH hosts for remote CLI execution with key generation wizard and connectivity testing |
| **Preview & Element Picker** | Open a localhost preview in a companion window, interactively pick page elements, and insert structured context (DOM path, styles, HTML snippet) into the chat |
| **Ralph Loop** | Auto-iterate the same prompt until a completion condition is met — hands-free coding with configurable max iterations |
| **Doctor Diagnostics** | System health checks for CLI, platform, SSH, and proxy configuration |
| **Code and Work Modes** | Code for interactive development; Work for durable tasks, inbox approvals, policies, artifacts, recovery, and scheduled execution |
| **Embedded Browser** | Electron-owned Chromium surface with navigation policy, localhost preview, CDP relay, and element/context capture |
| **Computer Use** | Desktop-use capability with visible approvals and structured context injection |

### Features

- **Rich Chat UI** — Markdown, syntax highlighting, thinking blocks, image attachments, file diffs, collapsible tool burst groups
- **Session Control** — Create, resume, fork, rename sessions; plan mode toggle; model hot-switch; context history tracking
- **Drag & Drop** — Native file drag-drop for images, PDFs, directories, and path references
- **Project Folders** — Sidebar project selector with per-project scoping for memory, permissions, and sessions
- **Inline Slash Commands** — `/model`, `/diff`, `/todos`, `/tasks`, `/doctor`, `/copy`, `/stats`, `/preview`, `/ralph`, and more — rendered natively in-app
- **Keyboard Shortcuts** — Fully customizable keybindings with chord support and conflict detection
- **Hook Manager** — Configure upstream CLI hooks for event-driven automation
- **i18n** — English and Chinese (Simplified) with lightweight reactive runtime
- **System Tray** — Hide to tray; background sessions keep running with native notifications
- **Dark / Light Theme** — CSS variable-based theming with UI zoom control
- **Auto Update** — In-app update checker with download links
- **Setup Wizard** — Guided CLI detection, authentication, and provider configuration on first launch

## Quick Start

### Option A: Download Pre-built Binary (macOS Apple Silicon)

Download the latest `.dmg` from the project's release page (macOS Apple Silicon / `arm64`).

> **Note**: The app is not code-signed. On first launch, right-click and select "Open" to bypass macOS Gatekeeper.

### Option B: Automated Setup (macOS)

After cloning the project using your preferred Git provider:

```bash
cd AgentCabin
./scripts/setup.sh          # add --yes to skip confirmation prompts
npm run electron:dev
```

The setup script detects missing development dependencies (Xcode CLI Tools, Homebrew, Node.js, Rust, and the Pi runtime) and offers to install them automatically.

### Option C: Manual Setup

**Prerequisites:**

- [Node.js](https://nodejs.org/) >= 22.19.0
- [Rust](https://rustup.rs/) >= 1.75

**macOS:**
```bash
xcode-select --install
brew install node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Linux (Debian/Ubuntu):**
```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows:**
```powershell
# Install Rust from https://rustup.rs
# Install Node.js from https://nodejs.org
```

**Build & Run:**

```bash
cd AgentCabin
npm install
npm run electron:dev
```

### Setup Wizard

On first launch, AgentCabin guides you through:

1. **Runtime Detection** — Detects the managed Pi and DSH runtimes and offers setup when needed
2. **Authentication** — OpenAI / ChatGPT account configuration or a custom Provider API key
3. **Ready** — Start coding

You can re-run the wizard anytime from **Settings > General > Setup Wizard**.

## Supported Providers

The current setup is intentionally small:

| Provider | Configuration |
|----------|---------------|
| OpenAI / ChatGPT | Official ChatGPT subscription or OpenAI account configuration |
| Custom | User-defined provider name, base URL, API key, API format, and model list |

Configure providers from **Settings > Models & Providers**. Custom providers can be addressed in chat using their configured `provider-id/model-name` model identifier.

## Architecture

AgentCabin is organized around a secure Electron desktop shell and a separate Rust core process:

```text
Electron host
├── Svelte 5 / SvelteKit renderer
│   ├── Code workspace: Pi Agent and DSH
│   └── Work workspace: tasks, Inbox, policies, artifacts, recovery
├── preload bridge + embedded Chromium / CDP browser
└── Rust core process
    ├── session actors and event bus
    ├── provider and runtime bridges
    ├── Work orchestration and desktop-only execution
    └── local JSON/file storage under ~/.agentcabin/

Managed runtime closure
├── Node.js 22.19.0 + pnpm 10.15.0
├── Pi 0.85.1 (RPC)
└── DSH 0.1.5-rc.2 (ACP)
```

**Tech Stack:**

| Layer | Technology |
|-------|-----------|
| Desktop shell | [Electron](https://www.electronjs.org/) (secure BrowserWindow, preload bridge, embedded Chromium) |
| Core | Rust headless process for session actors, storage, Work orchestration, and provider bridges |
| Frontend | [Svelte 5](https://svelte.dev/) + [SvelteKit](https://svelte.dev/docs/kit/) (adapter-static) |
| Styling | [Tailwind CSS](https://tailwindcss.com/) v3 + CSS variables |
| Terminal | [xterm.js](https://xtermjs.org/) |
| Markdown | [marked](https://marked.js.org/) + [highlight.js](https://highlightjs.org/) + [DOMPurify](https://github.com/cure53/DOMPurify) |
| i18n | Custom lightweight runtime (en + zh-CN) |
| Testing | [Vitest](https://vitest.dev/) |

**Agent Communication:**

Each session is a long-lived, multi-turn process managed by a per-run session actor. **Pi Agent** uses a long-lived JSONL RPC process. **DSH** uses managed Code/Work profiles and provider bridges. Work runs are desktop-only and add task state, inbox/approval flows, policies, artifacts, recovery, and optional scheduling on top of the same local core.

**Data Storage:**

All data is stored locally at `~/.agentcabin/` — no cloud, no database.

```
~/.agentcabin/
├── settings.json          # User settings
├── runs/                  # Session history
│   └── {run-id}/
│       ├── meta.json      # Run metadata
│       ├── events.jsonl   # Event log
│       └── artifacts.json # Summary
└── keybindings.json       # Custom shortcuts
```

## Development

```bash
npm install                 # Install dependencies
npm run electron:dev        # Electron + Vite + Rust core development
npm test                    # Run tests
npm run check               # Svelte/type checks
npm run lint                # ESLint
npm run format:check        # Prettier check
npm run package             # Build macOS DMG/ZIP and smoke-test packaged runtimes
```

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding conventions, and PR guidelines.

## License

Licensed under the [Apache License 2.0](LICENSE).

Copyright 2025-2026 AgentCabin Contributors.
