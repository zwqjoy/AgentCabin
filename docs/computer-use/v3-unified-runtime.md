# AgentCabin Computer Use V3 架构与开发文档

## 0. 架构愿景与基线

基于 Computer Use V2，演进为统一 UI 运行时：

```text
                    Unified UI Runtime
                           │
                    StateStore (stateId / epoch)
                           │
                 stable @r / stable @e
                           │
                         act_ui
                           │
                      verification
                           │
                    successor state
                           │
                      semantic diff
                           │
        ┌──────────────────┼──────────────────┐
        │                                     │
     Desktop                                 CDP
     macOS AX / UIA                        Browser / Electron
        │                                     │
  DesktopBackend                         CdpBackend
```

核心原则：
1. **统一工具契约**：模型仅操作统一的 9 个标准工具（`launch_app`, `find_roots`, `observe_ui`, `search_ui`, `expand_ui`, `inspect_ui`, `act_ui`, `read_text`, `wait_for`），不向模型暴露任何 `browser_*`、`desktop_*` 或 `cdp_*` 原生私有工具。
2. **状态与隔离模型**：`@e` 归属于不可变的 `stateId`。写操作受严格的 `resourceKey + epoch` 约束，旧 epoch 写入严格 fail-closed 拦截。
3. **真实语义后验**：`act_ui` 的 `expect` 执行真实验证与轮询，诚实报告 `verified` / `preexisting` / `failed` (`outcome: "didnt"`）。

---

## 1. 里程碑完成情况

### Milestone A (Phase 0): 正确性加固（Correctness Hardening）- [已完成]
- **稳定 Agent 引用**：实现 `ElementIdentity` 与 `RefStabilizer`，双向二分匹配与歧义检测，禁止原生线级引用（如 `ax:old-button`）泄露给模型。
- **真实语义后验**：`expect` 具备真实轮询与诚实裁决，多步批处理失败时精确记录 `stoppedAt`。
- **全局 Target 消除**：Host 不再依赖单例 `current_target`，严格基于 `pid / window_id / root_ref` 显式路由。
- **语义差分**：连续观察同一 root 产出精确变更集（`added`, `removed`, `updated`）。

### Milestone B (Phase 1): 统一 UI 森林与 CDP 后端 - [已完成]
- **领域模型与状态仓储 (`computer_use_v3_models.mjs`)**：
  - `UiRoot`：统一描述 Desktop 窗口、Browser 页面、Electron 页面。
  - `UiState`：管理状态快照、元素树与多模态截图。
  - `UiElement`：规范模型可见的 `@eN`、语义标签、值、边界矩形及底层 Token。
  - `StateStore`：独立抽象的状态仓储层，管理不可变状态集合与有界驱逐（LRU Eviction）。
- **桌面与 CDP 双后端适配器**：
  - `DesktopComputerUseBackend` (`desktop_computer_use_backend.mjs`)：统一调度原生桥接。
  - `CdpComputerUseBackend` (`cdp_computer_use_backend.mjs`)：映射 Playwright / Browser Worker 调用，支持页面切换、DOM 快照转元素树、动作精确翻译与截断。
- **生产装配胶水层接通**：
  - `pi_browser_operator_adapter.mjs`：导出 `createBrowserInvoker(options)`，无缝桥接 Work ToolPipeline（含 Inbox Approval 循环）或直接 HTTP Bridge。
  - `agentcabin_computer_use_v2_adapter.mjs`：自动接入 `invokeBrowser`，将 CDP 后端注入 `ComputerUseV2Session`。
  - `desktop_mcp_adapter.mjs`：通过 `AGENTCABIN_BROWSER_BRIDGE_PORT / TOKEN` 直连 Browser Runtime。
- **工具面收拢**：
  - 在 `pi_core_extension.mjs` 中，当 `AGENTCABIN_WORK_DESKTOP_USE_ENABLED === "1"` 开启时，不再向模型暴露裸露的 `browser_*` 工具，所有浏览器与桌面操作统一收拢至 9 个标准工具。
- **真实 Electron 端口探测与 AX 降级**：
  - `probeElectronCdp(pid, portHint, timeoutMs, fetcher)`：检查进程命令行 `--remote-debugging-port` 与端口提示，向 `http://127.0.0.1:<port>/json/version` 发起轻量 HTTP 探测。
  - 若端口畅通且返回合法 DevTools WebSocket 端点，则归为 `kind: "electron_page"`，优先走 CDP 通道；
  - 若未开放或连接失败，则安全降级为 `kind: "desktop_window"`，走 macOS AX 驱动。

### Milestone D (Phase 2): 视觉兜底定位（Visual Grounding Fallback）- [已完成]
- **严格优先级调度**：遵循 `Structured/CDP → Native AX → Vision Grounding → Coordinate Action` 调度级联。
- **触发准则**：在语义树为空、仅含 Canvas/Picture 控件（无交互子控件）或显式指定 `mode: "visual" | "fused"` 时激活。
- **视觉控制面与稳定引用**：OCR 与视觉探测结果映射为标准 `UiElement`（`evidence: { visual: true }`），经 `RefStabilizer` 统一赋予 `@eN` 引用，模型 9 大工具面保持完全不变。
- **坐标安全与状态作用域绑定**：
  - 点击视觉元素自动计算中心坐标 `(x + w/2, y + h/2)`，底层 Native Bridge 调用去除 `element_token` 走真实坐标点击。
  - 严格校验 `stateId`：跨状态复用坐标直接拒绝（Fail-Closed）。
- **多模态后验验证**：`waitForCondition` 与 `act_ui` 的 `expect` 机制支持 `requireVisualDiff` 及 OCR 文本比对。

### Milestone C (Phase 3): 评测基准套件（Computer Use Eval Harness）- [已完成]
- **13 类 Canonical 失败归因分类法 (`eval/eval_types.mjs`)**：
  `root_not_found`, `stale_state`, `wrong_grounding`, `element_not_found`, `action_rejected`, `timeout_waiting`, `postcondition_failed`, `unexpected_modal`, `coordinate_out_of_bounds`, `daemon_crash`, `permission_denied`, `unsupported_action`, `flaky_success`。
- **25 个 macOS 真实场景评测用例 (`eval/eval_cases_macos.mjs`)**：
  覆盖 Calculator (5), TextEdit (5), Finder (5), Chrome (5), Cross-App (5)。
- **自动化运行器与机器可读报告 (`eval/eval_runner.mjs`)**：
  追踪步骤耗时、工具调用次数，计算 `successRate`, `wrongClickRate`, `avgToolCalls`, `avgLatencyMs`，并输出失败类型直方图。

### Milestone E (Phase 4): Windows UIA 后端 - [暂不实现 / 延后]
- 根据产品规划与用户决策，Windows Native Bridge & UIA Backend 暂不实施，当前版本聚焦 macOS 与跨平台 Chromium/Electron (CDP)。

---

## 2. 生产模块组织与分发

| 模块文件 | 职责 | 生产分发路径 |
|---|---|---|
| `computer_use_v3_models.mjs` | `UiRoot`, `UiState`, `UiElement`, `StateStore` | `work_extensions_dir()`, `managed_runtime_dir` |
| `desktop_computer_use_backend.mjs` | 桌面后端驱动、原生响应解析、Electron CDP 探测 | `work_extensions_dir()`, `managed_runtime_dir` |
| `cdp_computer_use_backend.mjs` | CDP 动作翻译、DOM/AX 快照映射、标签页切换 | `work_extensions_dir()`, `managed_runtime_dir` |
| `visual_grounding_backend.mjs` | 视觉兜底检测、OCR 融合、中心坐标解析与多模态验证 | `work_extensions_dir()`, `managed_runtime_dir` |
| `computer_use_v2_runtime.mjs` | 会话调度、引用稳定化、资源锁、多路复用 | `work_extensions_dir()`, `managed_runtime_dir` |
| `agentcabin_computer_use_v2_adapter.mjs` | AgentCabin Work 运行时适配器，接入 Inbox 审批 | `work_extensions_dir()`, `managed_runtime_dir` |
| `desktop_mcp_adapter.mjs` | Claude / Codex / Grok 的 MCP 投影 | `managed_runtime_dir/agentcabin_desktop_mcp.mjs` |
| `eval/eval_types.mjs` | 13 类失败分类法与用例 Schema 校验 | `eval/` |
| `eval/eval_cases_macos.mjs` | 25 个 macOS 真实场景评测用例集合 | `eval/` |
| `eval/eval_runner.mjs` | 自动化评测运行器与指标聚合器 | `eval/` |

---

## 3. 验证套件

```bash
# 1. Visual Grounding 视觉兜底测试 (6 tests)
node --test src-tauri/src/work/visual_grounding_backend.test.mjs

# 2. Computer Use 评测套件测试 (5 tests)
node --test src-tauri/src/work/eval/eval_harness.test.mjs

# 3. Computer Use 核心运行时与加固测试 (22 tests)
node --test src-tauri/src/work/computer_use_v2_runtime.test.mjs

# 4. Desktop MCP Adapter 测试 (1 test)
node --test src-tauri/src/work/desktop_mcp_adapter.test.mjs

# 5. Pi Core Extension 全量测试 (34 tests)
node --test src-tauri/src/work/pi_core_extension.test.mjs

# 6. Pi Subagents Adapter 隔离测试 (54 tests)
node --test src-tauri/src/work/pi_subagents_adapter.test.mjs

# 7. Rust Desktop Operator 驱动与协议不变性测试 (3 tests)
cargo test --manifest-path src-tauri/Cargo.toml --package AgentCabin --lib work::desktop_operator
```
