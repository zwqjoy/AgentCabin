import test from "node:test";
import assert from "node:assert/strict";
import { DesktopComputerUseBackend } from "./desktop_computer_use_backend.mjs";

test("DesktopComputerUseBackend accepts native listRoots roots payloads", async () => {
  const backend = new DesktopComputerUseBackend(async () => ({
    success: true,
    structuredContent: {
      roots: [{
        pid: 123,
        windowId: 45,
        rootRef: "w-native",
        appName: "Calculator",
        title: "Calculator",
        isOnscreen: true,
      }],
    },
  }), { enableExternalCdp: false });

  const roots = await backend.listRoots("roots-test");
  assert.equal(roots.length, 1);
  assert.equal(roots[0].pid, 123);
  assert.equal(roots[0].windowId, 45);
  assert.equal(roots[0].nativeRootRef, "w-native");
  assert.equal(roots[0].appName, "Calculator");
});
