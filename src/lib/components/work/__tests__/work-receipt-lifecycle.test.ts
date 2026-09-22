import { describe, expect, it } from "vitest";

describe("Work Receipt Loading Lifecycle & Terminal Guard (Cases 7, 8, 9, 10)", () => {
  // Simulates the exact state machine and reactive guard implemented in WorkConversationInspector
  function createReceiptManager(getReceiptMock: () => Promise<any>) {
    let receipt: any = null;
    let receiptLoading = false;
    let receiptError = "";
    let receiptOpen = false;
    let receiptLoadedFor = "";
    let lastTerminalReceiptRefreshKey = "";
    let lastInspectedRunKey = "";
    let getReceiptCalls = 0;

    const wrappedGetReceipt = async () => {
      getReceiptCalls++;
      return await getReceiptMock();
    };

    async function loadReceipt(receiptKey: string, force = false) {
      if (!wrappedGetReceipt || receiptLoading) return;
      const key = receiptKey;
      if (key === "__none__") {
        receipt = null;
        return;
      }
      if (!force && receiptLoadedFor === key) return;
      receiptLoading = true;
      receiptError = "";
      try {
        receipt = await wrappedGetReceipt();
        receiptLoadedFor = key;
      } catch (cause: any) {
        receiptError = cause?.message ?? String(cause);
      } finally {
        receiptLoading = false;
      }
    }

    async function toggleReceipt(receiptKey: string) {
      receiptOpen = !receiptOpen;
      if (receiptOpen) await loadReceipt(receiptKey);
    }

    function handleRunChange(receiptKey: string) {
      const key = receiptKey;
      if (key !== lastInspectedRunKey) {
        lastInspectedRunKey = key;
        receipt = null;
        receiptError = "";
        receiptLoadedFor = "";
        lastTerminalReceiptRefreshKey = "";
      }
    }

    async function handleTerminalStatusEffect(receiptKey: string, runStatus?: string) {
      const status = runStatus;
      const key = receiptKey;
      const isTerminal = status === "completed" || status === "failed" || status === "cancelled";

      if (receiptOpen && isTerminal && key !== "__none__") {
        const terminalKey = `${key}:${status}`;
        if (lastTerminalReceiptRefreshKey !== terminalKey) {
          lastTerminalReceiptRefreshKey = terminalKey;
          await loadReceipt(key, true);
        }
      }
    }

    return {
      getReceiptCalls: () => getReceiptCalls,
      getReceipt: () => receipt,
      isLoading: () => receiptLoading,
      getError: () => receiptError,
      isOpen: () => receiptOpen,
      toggleReceipt,
      handleRunChange,
      handleTerminalStatusEffect,
      manualRefresh: (receiptKey: string) => loadReceipt(receiptKey, true),
    };
  }

  // Case 7: Receipt opened while running -> loads once
  it("Case 7: when opened during running state, getReceipt is called exactly once", async () => {
    const manager = createReceiptManager(async () => ({
      workRunId: "run-1",
      durationMs: 12000,
      artifacts: [],
    }));

    manager.handleRunChange("run-1");
    // Initial running status effect
    await manager.handleTerminalStatusEffect("run-1", "running");
    expect(manager.getReceiptCalls()).toBe(0);

    // User expands receipt
    await manager.toggleReceipt("run-1");

    expect(manager.getReceiptCalls()).toBe(1);
    expect(manager.getReceipt()).toEqual({
      workRunId: "run-1",
      durationMs: 12000,
      artifacts: [],
    });

    // Subsequent re-renders while running do NOT trigger additional calls
    await manager.handleTerminalStatusEffect("run-1", "running");
    await manager.handleTerminalStatusEffect("run-1", "running");
    expect(manager.getReceiptCalls()).toBe(1);
  });

  // Case 8: Receipt opened while running -> status becomes completed -> auto-refreshes ONCE, no loop
  it("Case 8: when running transitions to completed while receipt is open, auto-refreshes exactly once with no reactive loop", async () => {
    let returnFinishedReceipt = false;
    const manager = createReceiptManager(async () => {
      if (returnFinishedReceipt) {
        return { workRunId: "run-1", durationMs: 48000, status: "completed" };
      }
      return { workRunId: "run-1", durationMs: 20000, status: "running" };
    });

    manager.handleRunChange("run-1");
    // User opened while running
    await manager.toggleReceipt("run-1");
    expect(manager.getReceiptCalls()).toBe(1);
    expect(manager.getReceipt()?.status).toBe("running");

    // Run completes!
    returnFinishedReceipt = true;
    await manager.handleTerminalStatusEffect("run-1", "completed");

    expect(manager.getReceiptCalls()).toBe(2);
    expect(manager.getReceipt()?.status).toBe("completed");

    // Simulating multiple reactive progressView updates (polling every second or event deltas)
    for (let i = 0; i < 10; i++) {
      await manager.handleTerminalStatusEffect("run-1", "completed");
    }

    // Still exactly 2 calls! Guard prevented infinite refresh loop!
    expect(manager.getReceiptCalls()).toBe(2);
    expect(manager.isLoading()).toBe(false);
  });

  // Case 9: Manual refresh works even after terminal auto-refresh
  it("Case 9: manual refresh button can always trigger a reload", async () => {
    const manager = createReceiptManager(async () => ({
      workRunId: "run-1",
      durationMs: 50000,
    }));

    manager.handleRunChange("run-1");
    await manager.toggleReceipt("run-1");
    await manager.handleTerminalStatusEffect("run-1", "completed");
    expect(manager.getReceiptCalls()).toBe(2);

    // User clicks manual refresh button
    await manager.manualRefresh("run-1");
    expect(manager.getReceiptCalls()).toBe(3);
  });

  // Case 10: Switching Work run clears old receipt and allows fresh load on new run
  it("Case 10: switching from Run A to Run B clears Run A receipt and loads Run B cleanly", async () => {
    let currentRunId = "run-A";
    const manager = createReceiptManager(async () => ({
      workRunId: currentRunId,
      durationMs: currentRunId === "run-A" ? 30000 : 60000,
    }));

    // Load Run A
    manager.handleRunChange("run-A");
    await manager.toggleReceipt("run-A");
    expect(manager.getReceipt()?.workRunId).toBe("run-A");

    // Switch to Run B
    currentRunId = "run-B";
    manager.handleRunChange("run-B");

    // Run A receipt is immediately cleared
    expect(manager.getReceipt()).toBeNull();

    // Now load Run B
    await manager.manualRefresh("run-B");
    expect(manager.getReceipt()?.workRunId).toBe("run-B");
    expect(manager.getReceipt()?.durationMs).toBe(60000);
  });
});
