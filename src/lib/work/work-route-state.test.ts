import { describe, expect, it } from "vitest";
import {
  isStandaloneRoute,
  parseWorkRouteState,
  showsConversation,
  showsWorkspaceHome,
} from "./work-route-state";

function url(search: string): URL {
  return new URL(`https://app.local/chat/work${search}`);
}

describe("parseWorkRouteState", () => {
  it("parses the standalone home route", () => {
    const state = parseWorkRouteState(url(""));
    expect(state.workspaceId).toBeNull();
    expect(state.runId).toBeNull();
    expect(state.newConversation).toBe(false);
    expect(state.panel).toBeNull();
    expect(state.legacyView).toBeNull();
    expect(isStandaloneRoute(state)).toBe(true);
    expect(showsConversation(state, true)).toBe(true);
  });

  it("parses a workspace conversation route", () => {
    const state = parseWorkRouteState(url("?workspace=ws-1&run=run-1"));
    expect(state.workspaceId).toBe("ws-1");
    expect(state.runId).toBe("run-1");
    expect(isStandaloneRoute(state)).toBe(false);
    expect(showsConversation(state, true)).toBe(true);
    expect(showsWorkspaceHome(state, true)).toBe(false);
  });

  it("marks a workspace without a run as the workspace home", () => {
    const state = parseWorkRouteState(url("?workspace=ws-1"));
    expect(showsWorkspaceHome(state, true)).toBe(true);
    expect(showsConversation(state, true)).toBe(false);
  });

  it("maps legacy view params onto auxiliary panels", () => {
    expect(parseWorkRouteState(url("?view=inbox")).panel).toBe("pending");
    expect(parseWorkRouteState(url("?view=library")).panel).toBe("library");
    expect(parseWorkRouteState(url("?view=archived")).panel).toBe("archived");
    expect(parseWorkRouteState(url("?view=tasks")).panel).toBe("automation");
    expect(parseWorkRouteState(url("?view=automation")).panel).toBe("automation");
    // Unknown legacy views degrade to no panel instead of a broken page.
    expect(parseWorkRouteState(url("?view=unknown")).panel).toBeNull();
  });

  it("prefers an explicit panel param", () => {
    expect(parseWorkRouteState(url("?panel=files")).panel).toBe("files");
    expect(parseWorkRouteState(url("?panel=bogus")).panel).toBeNull();
  });

  it("parses new conversation / create dialog / fresh / prompt flags", () => {
    const state = parseWorkRouteState(
      url("?workspace=ws-1&newSession=1&fresh=1&prompt=hello&preset=office"),
    );
    expect(state.newConversation).toBe(true);
    expect(state.fresh).toBe(true);
    expect(state.prompt).toBe("hello");
    expect(state.preset).toBe("office");
    expect(parseWorkRouteState(url("?new=1")).createWorkspace).toBe(true);
  });

  it("treats blank params as absent", () => {
    const state = parseWorkRouteState(url("?workspace=&run="));
    expect(state.workspaceId).toBeNull();
    expect(state.runId).toBeNull();
    expect(isStandaloneRoute(state)).toBe(true);
  });
});
