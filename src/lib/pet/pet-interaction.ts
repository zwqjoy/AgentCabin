import { getTransport } from "$lib/transport";

export interface DragOptions {
  onDragStart?: () => void;
  onDragEnd?: () => void;
  /** Called after a pointer press that did not move the pet window. */
  onTap?: () => void;
}

export function attachWindowDrag(element: HTMLElement, options?: DragOptions): () => void {
  const transport = getTransport();
  let isDragging = false;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let dragEndTimer: ReturnType<typeof setTimeout> | null = null;
  let moveUnlisten: (() => void) | null = null;
  let detached = false;
  let pointerDownAt: { x: number; y: number } | null = null;
  let capturedPointerId: number | null = null;
  let pointerMoved = false;

  const POINTER_TAP_SLOP = 6;

  const schedulePositionSave = () => {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        const outerPos = await transport.outerPosition();
        await transport.invoke("save_pet_position", { x: outerPos.x, y: outerPos.y });
      } catch (error) {
        console.warn("[pet-interaction] Failed to save position:", error);
      }
    }, 300);
  };

  const finishDrag = () => {
    if (!isDragging) return;
    const wasTap = !pointerMoved;
    isDragging = false;
    pointerDownAt = null;
    pointerMoved = false;

    if (capturedPointerId !== null) {
      try {
        if (element.hasPointerCapture(capturedPointerId)) {
          element.releasePointerCapture(capturedPointerId);
        }
      } catch {
        // ignore
      }
      capturedPointerId = null;
    }

    void transport.invoke("set_pet_dragging", { dragging: false }).catch(() => undefined);
    if (dragEndTimer) {
      clearTimeout(dragEndTimer);
      dragEndTimer = null;
    }
    options?.onDragEnd?.();
    void transport.invoke("snap_pet_to_edge").catch(() => undefined);
    schedulePositionSave();
    if (wasTap) options?.onTap?.();
  };

  const handlePointerMove = (event: PointerEvent) => {
    if (!isDragging || !pointerDownAt) return;
    const dx = event.clientX - pointerDownAt.x;
    const dy = event.clientY - pointerDownAt.y;
    if (!pointerMoved && Math.hypot(dx, dy) > POINTER_TAP_SLOP) {
      pointerMoved = true;
    }
  };

  const handlePointerDown = async (event: PointerEvent) => {
    if (event.button !== 0 || isDragging) return;
    event.preventDefault();
    try {
      element.setPointerCapture(event.pointerId);
      capturedPointerId = event.pointerId;
    } catch {
      // ignore
    }
    isDragging = true;
    pointerDownAt = { x: event.clientX, y: event.clientY };
    pointerMoved = false;
    void transport.invoke("set_pet_dragging", { dragging: true }).catch(() => undefined);
    options?.onDragStart?.();

    try {
      await transport.startDragging();
    } catch (error) {
      console.warn("[pet-interaction] startDragging failed:", error);
      finishDrag();
    }
  };

  const handlePointerUp = () => finishDrag();

  element.addEventListener("pointerdown", handlePointerDown);
  window.addEventListener("pointermove", handlePointerMove);
  window.addEventListener("pointerup", handlePointerUp);
  window.addEventListener("pointercancel", handlePointerUp);

  // In Tauri, the OS window manager takes over during startDragging and pointerup
  // may not be received, so the move event with an inactivity timer serves as a fallback.
  const handleWindowMoved = () => {
    if (!isDragging) return;
    schedulePositionSave();
    if (capturedPointerId === null) {
      pointerMoved = true;
      if (dragEndTimer) clearTimeout(dragEndTimer);
      dragEndTimer = setTimeout(finishDrag, 180);
    }
  };

  void transport.listenWindowMoved(handleWindowMoved).then((unlisten) => {
    if (detached) unlisten();
    else moveUnlisten = unlisten;
  });

  return () => {
    detached = true;
    if (capturedPointerId !== null) {
      try {
        if (element.hasPointerCapture(capturedPointerId)) {
          element.releasePointerCapture(capturedPointerId);
        }
      } catch {
        // ignore
      }
      capturedPointerId = null;
    }
    element.removeEventListener("pointerdown", handlePointerDown);
    window.removeEventListener("pointermove", handlePointerMove);
    window.removeEventListener("pointerup", handlePointerUp);
    window.removeEventListener("pointercancel", handlePointerUp);
    moveUnlisten?.();
    moveUnlisten = null;
    if (dragEndTimer) clearTimeout(dragEndTimer);
    if (saveTimer) clearTimeout(saveTimer);
    void transport.invoke("set_pet_dragging", { dragging: false }).catch(() => undefined);
  };
}
