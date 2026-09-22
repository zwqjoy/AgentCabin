<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { PetAggregateState, PetMode, PetNotification } from "./types";
  import { MODE_FRAMES, MODE_ROW, fitSpriteToCanvas, type SpriteSheet } from "./pet-sprites";

  interface Props {
    sprite: SpriteSheet | null;
    state: PetAggregateState;
    dragging?: boolean;
    notification?: PetNotification | null;
    active?: boolean;
  }

  let {
    sprite,
    state: petState,
    dragging = false,
    notification = null,
    active = true,
  }: Props = $props();

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let stopLoop: (() => void) | null = null;

  const IDLE_FPS = 8;
  const DEFAULT_FPS = 12;

  function syncCanvasSize(canvas: HTMLCanvasElement) {
    const dpr = window.devicePixelRatio || 1;
    const cssWidth = Math.max(1, canvas.clientWidth);
    const cssHeight = Math.max(1, canvas.clientHeight);
    const pixelWidth = Math.max(1, Math.round(cssWidth * dpr));
    const pixelHeight = Math.max(1, Math.round(cssHeight * dpr));

    if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
      canvas.width = pixelWidth;
      canvas.height = pixelHeight;
    }
    return { dpr, cssWidth, cssHeight };
  }

  function drawFallback(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    mode: PetMode,
    frame: number,
  ) {
    const colors: Record<PetMode, { body: string; accent: string }> = {
      idle: { body: "#8c96a3", accent: "#dbe4ed" },
      waiting: { body: "#d18b42", accent: "#fff1c7" },
      running: { body: "#4b9d78", accent: "#d8f4dc" },
      review: { body: "#5284c8", accent: "#dcecff" },
      failed: { body: "#c96569", accent: "#ffe0df" },
    };
    const palette = colors[mode];
    const cx = width / 2;
    const cy = height * 0.54;
    const radius = Math.min(width, height) * 0.28;
    const bob = Math.sin(frame * 0.8) * radius * (mode === "idle" ? 0.04 : 0.09);

    ctx.save();
    ctx.translate(cx, cy + bob);
    ctx.imageSmoothingEnabled = false;

    ctx.fillStyle = palette.body;
    ctx.beginPath();
    ctx.arc(-radius * 0.62, -radius * 0.85, radius * 0.42, 0, Math.PI * 2);
    ctx.arc(radius * 0.62, -radius * 0.85, radius * 0.42, 0, Math.PI * 2);
    ctx.fill();

    ctx.beginPath();
    ctx.roundRect(-radius, -radius, radius * 2, radius * 2.15, radius * 0.72);
    ctx.fill();

    ctx.fillStyle = palette.accent;
    ctx.beginPath();
    ctx.ellipse(0, radius * 0.44, radius * 0.62, radius * 0.78, 0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = "#263746";
    ctx.beginPath();
    ctx.arc(-radius * 0.38, -radius * 0.25, Math.max(2, radius * 0.1), 0, Math.PI * 2);
    ctx.arc(radius * 0.38, -radius * 0.25, Math.max(2, radius * 0.1), 0, Math.PI * 2);
    ctx.fill();

    ctx.strokeStyle = "#263746";
    ctx.lineWidth = Math.max(1, radius * 0.07);
    ctx.lineCap = "round";
    ctx.beginPath();
    if (mode === "failed") {
      ctx.moveTo(-radius * 0.35, radius * 0.2);
      ctx.lineTo(radius * 0.35, radius * 0.48);
      ctx.moveTo(radius * 0.35, radius * 0.2);
      ctx.lineTo(-radius * 0.35, radius * 0.48);
    } else {
      ctx.arc(0, radius * 0.2, radius * 0.28, 0.15, Math.PI - 0.15);
    }
    ctx.stroke();

    if (mode === "waiting") {
      ctx.fillStyle = "#263746";
      for (let index = 0; index < 3; index += 1) {
        ctx.globalAlpha = 0.45 + ((frame + index) % 3) * 0.25;
        ctx.beginPath();
        ctx.arc(
          -radius * 0.52 + index * radius * 0.52,
          radius * 1.13,
          radius * 0.08,
          0,
          Math.PI * 2,
        );
        ctx.fill();
      }
    } else if (mode === "running") {
      ctx.strokeStyle = palette.accent;
      ctx.lineWidth = Math.max(2, radius * 0.1);
      ctx.beginPath();
      ctx.moveTo(-radius * 1.25, radius * 0.15);
      ctx.lineTo(-radius * 1.55, radius * 0.35);
      ctx.moveTo(radius * 1.25, radius * 0.15);
      ctx.lineTo(radius * 1.55, radius * 0.35);
      ctx.stroke();
    } else if (mode === "review") {
      ctx.strokeStyle = "#263746";
      ctx.lineWidth = Math.max(2, radius * 0.1);
      ctx.beginPath();
      ctx.arc(radius * 0.92, radius * 0.55, radius * 0.32, 0, Math.PI * 2);
      ctx.moveTo(radius * 1.15, radius * 0.8);
      ctx.lineTo(radius * 1.45, radius * 1.1);
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawNotification(
    ctx: CanvasRenderingContext2D,
    canvas: HTMLCanvasElement,
    currentNotification: PetNotification | null,
  ) {
    if (!currentNotification) return;

    const elapsed = Date.now() - currentNotification.timestamp;
    const fadeIn = 250;
    const fadeOut = 500;
    const total = 4000;
    if (elapsed < 0 || elapsed >= total) return;

    let alpha = 1;
    if (elapsed < fadeIn) alpha = elapsed / fadeIn;
    else if (elapsed > total - fadeOut) alpha = (total - elapsed) / fadeOut;
    if (alpha <= 0) return;

    const { dpr, cssWidth, cssHeight } = syncCanvasSize(canvas);
    const fontSize = Math.round(13 * dpr);
    const paddingX = 10 * dpr;
    const paddingY = 5 * dpr;
    const maxWidth = cssWidth * dpr - 20 * dpr;
    const text = currentNotification.text.trim();

    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.font = `600 ${fontSize}px system-ui, -apple-system, sans-serif`;
    const boxWidth = Math.min(maxWidth, ctx.measureText(text).width + paddingX * 2);
    const boxHeight = fontSize * 1.5 + paddingY * 2;
    const boxX = (cssWidth * dpr - boxWidth) / 2;
    const boxY = cssHeight * dpr - boxHeight - 6 * dpr;

    ctx.fillStyle = "rgba(255, 255, 255, 0.95)";
    ctx.beginPath();
    ctx.roundRect(boxX, boxY, boxWidth, boxHeight, 7 * dpr);
    ctx.fill();
    ctx.strokeStyle = "rgba(38, 55, 70, 0.75)";
    ctx.lineWidth = Math.max(1, dpr);
    ctx.stroke();

    ctx.fillStyle = currentNotification.type === "error" ? "#b83d46" : "#287650";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(text, boxX + boxWidth / 2, boxY + boxHeight / 2);
    ctx.restore();
  }

  $effect(() => {
    const canvas = canvasEl;
    const shouldAnimate = active;
    if (!canvas) return;

    const ctx = canvas.getContext("2d", { alpha: true });
    if (!ctx) return;

    let stopped = false;
    let rafId = 0;
    let currentMode: PetMode = "idle";
    let frame = 0;
    let accumulator = 0;
    let lastTime = performance.now();

    const draw = () => {
      if (stopped) return;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.imageSmoothingEnabled = false;
      if (!shouldAnimate) return;

      const mode = petState.mode;
      if (mode !== currentMode) {
        currentMode = mode;
        frame = 0;
        accumulator = 0;
      }

      const currentSprite = sprite;
      const row = MODE_ROW[mode];
      if (currentSprite) {
        const animation = currentSprite.animations?.[mode] ?? currentSprite.animations?.idle;
        if (animation) {
          const animationImage = animation as HTMLImageElement;
          const sourceWidth = animationImage.naturalWidth || animationImage.width || 1;
          const sourceHeight = animationImage.naturalHeight || animationImage.height || 1;
          const destination = fitSpriteToCanvas(
            canvas.width,
            canvas.height,
            sourceWidth,
            sourceHeight,
          );
          ctx.imageSmoothingEnabled = true;
          ctx.drawImage(
            animation,
            destination.x,
            destination.y,
            destination.width,
            destination.height,
          );
        } else if (currentSprite.singleFrame) {
          ctx.save();
          const indicatorColors: Record<PetMode, string> = {
            idle: "#7c8490",
            waiting: "#e7a63b",
            running: "#18a978",
            review: "#3c91ed",
            failed: "#e35b7b",
          };
          ctx.fillStyle = indicatorColors[mode];
          ctx.beginPath();
          ctx.arc(
            canvas.width * 0.82,
            canvas.height * 0.18,
            Math.max(3, canvas.width * 0.025),
            0,
            Math.PI * 2,
          );
          ctx.fill();
          ctx.restore();
        } else {
          const sourceWidth = currentSprite.cellW;
          const sourceHeight = currentSprite.cellH;
          const destination = fitSpriteToCanvas(
            canvas.width,
            canvas.height,
            sourceWidth,
            sourceHeight,
          );
          ctx.drawImage(
            currentSprite.image,
            frame * sourceWidth,
            row * sourceHeight,
            sourceWidth,
            sourceHeight,
            destination.x,
            destination.y,
            destination.width,
            destination.height,
          );
        }
      } else {
        drawFallback(ctx, canvas.width, canvas.height, mode, frame);
      }

      // Keep the notification clock independent of animation time while dragging.
      drawNotification(ctx, canvas, notification);
    };

    const loop = (now: number) => {
      if (stopped) return;
      draw();
      rafId = requestAnimationFrame(loop);

      if (!shouldAnimate || dragging) {
        lastTime = now;
        return;
      }

      const mode = petState.mode;
      const frameCount = MODE_FRAMES[mode];
      const fps = mode === "idle" ? IDLE_FPS : DEFAULT_FPS;
      const frameDuration = 1000 / fps;
      accumulator += Math.min(250, now - lastTime);
      lastTime = now;

      while (accumulator >= frameDuration) {
        accumulator -= frameDuration;
        frame = (frame + 1) % frameCount;
      }
    };

    const handleResize = () => {
      const dpr = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(canvas.clientWidth * dpr));
      const height = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
      }
      draw();
    };

    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const dpr = window.devicePixelRatio || 1;
        const width = Math.max(1, Math.round(entry.contentRect.width * dpr));
        const height = Math.max(1, Math.round(entry.contentRect.height * dpr));
        if (canvas.width !== width || canvas.height !== height) {
          canvas.width = width;
          canvas.height = height;
        }
      }
      draw();
    });

    handleResize();
    resizeObserver.observe(canvas);
    window.addEventListener("resize", handleResize);
    untrack(draw);

    if (shouldAnimate) {
      rafId = requestAnimationFrame(loop);
    }

    stopLoop = () => {
      stopped = true;
      if (rafId) cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      window.removeEventListener("resize", handleResize);
    };

    return () => {
      stopped = true;
      if (rafId) cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      window.removeEventListener("resize", handleResize);
      if (stopLoop) stopLoop = null;
    };
  });

  onDestroy(() => {
    stopLoop?.();
    stopLoop = null;
  });
</script>

<div class="relative h-full w-full">
  {#if sprite?.singleFrame}
    <img
      src={sprite.url}
      alt=""
      aria-hidden="true"
      class="pet-single-image absolute inset-0 h-full w-full object-contain"
    />
  {/if}
  <canvas
    bind:this={canvasEl}
    aria-label="AgentCabin desktop pet"
    class="relative z-10 block h-full w-full"
    style="image-rendering: pixelated;"
  ></canvas>
</div>

<style>
  .pet-single-image {
    animation: pet-single-image-bob 2.4s ease-in-out infinite;
  }

  @keyframes pet-single-image-bob {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(-2.5%);
    }
  }
</style>
