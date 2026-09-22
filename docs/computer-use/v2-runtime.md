# AgentCabin Computer Use V2

AgentCabin 的 Computer Use 架构以状态化 UI runtime 为中心，产品控制面继续由
AgentCabin 负责，macOS 平台实现使用内置的 AgentCabin Computer Use native backend。

## 工具面

Code 与 Work 使用同一组模型工具：

- `launch_app`
- `find_roots`
- `observe_ui`
- `search_ui`
- `expand_ui`
- `inspect_ui`
- `act_ui`
- `read_text`
- `wait_for`

这些名称属于模型内部协议。用户只需描述目标，不应被要求记住或输入工具名。

## 状态与执行

`observe_ui` 返回 immutable `stateId`。状态记录包含 `resourceKey` 和 `epoch`；
写操作在调用 native backend 前先递增资源 epoch，因此同一状态不能重复写入。
`search_ui`、`expand_ui`、`inspect_ui` 和 `read_text` 查询完整的有界缓存，不重新截图。

`act_ui` 接受 1–20 个关联动作，并在动作后返回 successor state 和语义 diff。
当调用包含 `expect` 时，结果使用 `verified`、`preexisting`、`failed` 或 `timeout` 表达后置条件，
并使用 `worked`、`didnt` 或 `unknown` 表达真实结果，而不是把事件投递当作目标成功。

## 当前迁移边界

V2 runtime 已是 Code Pi、Work Pi 以及 Claude/Codex/Grok MCP 的模型工具面。
所有实时操作仍通过 AgentCabin Host bridge 和 Work Tool Pipeline，保持 Policy、Inbox
Approval、Audit 与会话生命周期边界。私有 `desktop_*` 方法只是 Host backend
传输协议，不注册为模型工具。

旧兼容实现和资源已经移除。当前实现已经完成：

- 将 AgentCabin Computer Use macOS Swift bridge 作为签名资源纳入构建；
- 在 Host 侧实现持久进程、请求关联、超时、退出恢复和输出上限；
- 把原生 look、batch action、stable refs 和 Desktop roots 投影到 V2 契约；
- 保持 Code loopback bearer token 与 Work Policy/Inbox/Audit 边界；
- 在设置页显示 bridge、Accessibility 和 Screen Recording 状态。

剩余工作包括 `drag`、`moveMouse`、CDP roots，以及 Calculator、TextEdit、
浏览器/Electron、焦点抢占、并发 stale state 和 DMG 权限验收。开发宿主必须先获得
Accessibility 与 Screen Recording 权限，才能执行真实桌面动作。

## P1.1 Correctness Hardening

- Agent ref 与原生 wire ref 分开保存。连续观察按控件身份匹配；歧义匹配不复用 ref，动作始终使用当前状态对应的原生 ref。
- `expect.timeoutMs` 与 `wait_for` 共用轮询观察路径（默认 10 秒，范围 100–60000 毫秒）。超时表示未确认目标，返回 `unknown`；最终截图来自最终状态。单次 Host 观察仍受其独立请求超时约束。
- Host 不保存全局 current target。每次观察和动作携带 `pid/window_id/root_ref`，目标不存在或不唯一时拒绝执行。观察已有 root 不再重新启动应用。
- 每个 root 独立维护 epoch；整个物理桌面仍由会话租约控制，避免多个会话同时输入。
- 每次连接已有 helper 和启动新 helper 都校验协议版本、架构版本与七项 required invariants，不兼容时拒绝调用。

上述行为有运行时和 Host 单元测试；真实多窗口、焦点抢占及打包权限仍需桌面验收。

## 架构来源

V2 的状态作用域、资源 epoch、progressive disclosure、事务动作、语义后置条件和
successor diff 参考了 MIT 许可的
[`injaneity/pi-computer-use`](https://github.com/injaneity/pi-computer-use)。AgentCabin
没有直接加载其 Pi Extension，因为实时桌面操作必须经过 AgentCabin Host 的策略、
审批和审计边界。
