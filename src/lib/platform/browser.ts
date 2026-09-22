/** Renderer-facing API for the Electron-owned embedded browser surface. */
export interface EmbeddedBrowserEndpoint {
  host: string;
  port: number;
  token: string;
  targetId: string;
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
  url?: string;
  rect?: { x: number; y: number; width: number; height: number };
}) {
  return bridge()!.attach(payload);
}

export function setEmbeddedBrowserBounds(
  viewId: string,
  rect: { x: number; y: number; width: number; height: number },
) {
  return bridge()!.setBounds({ viewId, rect });
}

export function setEmbeddedBrowserVisible(viewId: string, visible: boolean) {
  return bridge()!.setVisible({ viewId, visible });
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

export function detachEmbeddedBrowser(viewId: string) {
  return bridge()!.detach({ viewId });
}
