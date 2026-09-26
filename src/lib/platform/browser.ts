/** Renderer-facing API for the Electron-owned embedded browser surface. */
export interface EmbeddedBrowserEndpoint {
  host: string;
  port: number;
  token: string;
  targetId: string;
}

export interface EmbeddedBrowserTab {
  id: string;
  targetId: string;
  index: number;
  url: string;
  title: string;
  active: boolean;
}

export interface EmbeddedBrowserTabsChanged {
  viewId: string;
  runId: string;
  activeTargetId: string;
  tabs: EmbeddedBrowserTab[];
}

function bridge() {
  return typeof window === "undefined" ? undefined : window.agentcabinDesktop?.browser;
}

export function isEmbeddedBrowserAvailable(): boolean {
  return Boolean(bridge());
}

export function attachEmbeddedBrowser(payload: {
  viewId: string;
  runId: string;
  bindingId: string;
  url?: string;
  rect?: { x: number; y: number; width: number; height: number };
}) {
  return bridge()!.attach(payload);
}

export function setEmbeddedBrowserBounds(
  viewId: string,
  bindingId: string,
  rect: { x: number; y: number; width: number; height: number },
) {
  return bridge()!.setBounds({ viewId, bindingId, rect });
}

export function setEmbeddedBrowserVisible(viewId: string, bindingId: string, visible: boolean) {
  return bridge()!.setVisible({ viewId, bindingId, visible });
}

export function sendEmbeddedBrowserCommand(
  viewId: string,
  action: "back" | "forward" | "reload" | "stop",
) {
  return bridge()!.command({ viewId, action });
}

export function getEmbeddedBrowserEndpoint(viewId: string) {
  return bridge()!.getEndpoint({ viewId });
}

export function unbindEmbeddedBrowserSurface(viewId: string, bindingId: string) {
  return bridge()!.unbind({ viewId, bindingId });
}

export function destroyEmbeddedBrowser(runId: string) {
  return bridge()!.destroy({ runId });
}
