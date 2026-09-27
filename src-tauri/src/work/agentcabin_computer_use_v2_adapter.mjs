import { Type } from "typebox";
import { randomUUID } from "node:crypto";
import { ComputerUseV2Session } from "./computer_use_v2_runtime.mjs";
import { createComputerUseToolPipelineInvoker } from "./computer_use_tool_pipeline.mjs";
import { createBrowserInvoker } from "./pi_browser_operator_adapter.mjs";

const StateId = Type.String({ description: "Immutable UI state owning every @e ref used by this operation." });
const Condition = {
  ref: Type.Optional(Type.String()),
  text: Type.Optional(Type.String()),
  role: Type.Optional(Type.String()),
  value: Type.Optional(Type.String()),
  until: Type.Optional(Type.Union([Type.Literal("present"), Type.Literal("absent")])),
  timeoutMs: Type.Optional(Type.Number({ minimum: 100, maximum: 60_000 })),
};
const Action = Type.Union([
  Type.Object({ action: Type.Literal("press"), ref: Type.String() }),
  Type.Object({ action: Type.Literal("click"), ref: Type.Optional(Type.String()), x: Type.Optional(Type.Number()), y: Type.Optional(Type.Number()), button: Type.Optional(Type.String()), clickCount: Type.Optional(Type.Integer({ minimum: 1, maximum: 3 })) }),
  Type.Object({ action: Type.Literal("setText"), ref: Type.String(), text: Type.String() }),
  Type.Object({ action: Type.Literal("typeText"), ref: Type.Optional(Type.String()), text: Type.String() }),
  Type.Object({ action: Type.Literal("keypress"), ref: Type.Optional(Type.String()), keys: Type.Array(Type.String(), { minItems: 1 }) }),
  Type.Object({ action: Type.Literal("scroll"), ref: Type.Optional(Type.String()), scrollX: Type.Optional(Type.Number()), scrollY: Type.Optional(Type.Number()) }),
]);

export function registerComputerUseV2Tools(pi, options = {}) {
  const registerTool = options.registerTool || options.registerWorkTool || ((tool) => pi.registerTool(tool));
  const browserInvoker = options.invokeBrowser || options.browserInvoker || createBrowserInvoker(options);
  const session = options.session || new ComputerUseV2Session(createComputerUseToolPipelineInvoker(options), {
    invokeBrowser: browserInvoker,
  });
  // Interactive agents stay alive after replying. Release the shared desktop
  // at the end of each agent turn, rather than waiting for process shutdown.
  let desktopUsed = false;
  const releaseDesktop = async () => {
    if (!desktopUsed) return;
    await session.backend(`desktop-release:${randomUUID()}`,
      "desktop_release", "release", {}, undefined);
    desktopUsed = false;
  };
  if (typeof pi.on === "function") {
    pi.on("agent_end", releaseDesktop);
    pi.on("session_shutdown", releaseDesktop);
  }
  const register = (name, description, parameters, execute) => registerTool({
    name,
    label: name,
    description,
    parameters,
    execute: (toolCallId, params, signal) => {
      desktopUsed = true;
      return execute.call(session, toolCallId, params || {}, signal);
    },
  });

  register("launch_app", "Launch a native application and immediately return its first immutable UI state.", Type.Object({ name: Type.Optional(Type.String()), bundleId: Type.Optional(Type.String()), createsNewApplicationInstance: Type.Optional(Type.Boolean()) }), session.launchApp);
  register("find_roots", "Find a bounded set of controllable desktop roots. Use natural app or window text to narrow the result.", Type.Object({ text: Type.Optional(Type.String()), app: Type.Optional(Type.String()) }), session.findRoots);
  register("observe_ui", "Observe one desktop root and return a compact accessibility outline plus immutable stateId.", Type.Object({ root: Type.Optional(Type.String()), mode: Type.Optional(Type.Union([Type.Literal("semantic"), Type.Literal("visual"), Type.Literal("fused")])) }), session.observeUi);
  register("search_ui", "Search the complete cached UI state without capturing the screen again.", Type.Object({ stateId: StateId, text: Type.Optional(Type.String()), role: Type.Optional(Type.String()) }), session.searchUi);
  register("expand_ui", "Show bounded local context around one element in a cached UI state.", Type.Object({ stateId: StateId, ref: Type.String(), depth: Type.Optional(Type.Integer({ minimum: 1, maximum: 8 })) }), session.expandUi);
  register("inspect_ui", "Inspect one exact element owned by an immutable UI state.", Type.Object({ stateId: StateId, ref: Type.String() }), session.inspectUi);
  register("act_ui", "Execute one or more checked dependent UI actions and return the successor state. Add expect when completion has observable UI evidence.", Type.Object({ stateId: StateId, actions: Type.Array(Action, { minItems: 1, maxItems: 20 }), expect: Type.Optional(Type.Object(Condition)) }), session.actUi);
  register("read_text", "Read text from an exact element in an immutable UI state.", Type.Object({ stateId: StateId, ref: Type.String(), offset: Type.Optional(Type.Integer({ minimum: 0 })) }), session.readText);
  register("wait_for", "Wait for a scoped UI condition and return the successor state.", Type.Object({ stateId: StateId, ...Condition }), session.waitFor);
  return session;
}

export default function agentCabinComputerUseV2Extension(pi, dependencies = {}) {
  return registerComputerUseV2Tools(pi, dependencies);
}
