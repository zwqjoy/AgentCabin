<script lang="ts">
  import { formatBytes } from "$lib/utils/format";
  import { isPdf } from "$lib/utils/file-types";
  import { t } from "$lib/i18n/index.svelte";

  let {
    name,
    size,
    mimeType = "",
    contentBase64 = "",
    isPathRef = false,
    onremove,
  }: {
    name: string;
    size: number;
    mimeType?: string;
    contentBase64?: string;
    isPathRef?: boolean;
    onremove?: () => void;
  } = $props();

  let isDoc = $derived(isPdf(mimeType));
  let isImage = $derived(mimeType.startsWith("image/"));
  let isDir = $derived(mimeType === "inode/directory");
  let lightboxOpen = $state(false);
  let zoom = $state(1);

  function openLightbox() {
    if (!isImage || !contentBase64) return;
    zoom = 1;
    lightboxOpen = true;
  }

  function closeLightbox() {
    lightboxOpen = false;
  }

  function handleLightboxKeydown(event: KeyboardEvent) {
    if (!lightboxOpen) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeLightbox();
    }
  }

  function changeZoom(delta: number) {
    zoom = Math.min(3, Math.max(0.5, Number((zoom + delta).toFixed(2))));
  }

  function resetZoom() {
    zoom = 1;
  }

  function handleImageWheel(event: WheelEvent) {
    event.preventDefault();
    changeZoom(event.deltaY > 0 ? -0.1 : 0.1);
  }

  function downloadImage(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    const link = document.createElement("a");
    link.href = `data:${mimeType};base64,${contentBase64}`;
    link.download = name || "image";
    document.body.appendChild(link);
    link.click();
    link.remove();
  }

  function handleCloseClick(event: MouseEvent) {
    event.stopPropagation();
    closeLightbox();
  }

  // Color scheme per file type
  let colorClasses = $derived.by(() => {
    if (isDir) {
      return {
        border: isPathRef
          ? "border-dashed border-amber-400/50 dark:border-amber-500/40"
          : "border-amber-200 dark:border-amber-800",
        bg: "bg-amber-50 dark:bg-amber-950/40",
        icon: "text-amber-600 dark:text-amber-400",
        size: "text-amber-400 dark:text-amber-500",
      };
    }
    if (isImage) {
      return {
        border: isPathRef
          ? "border-dashed border-sky-400/50 dark:border-sky-500/40"
          : "border-sky-200 dark:border-sky-800",
        bg: "bg-sky-50 dark:bg-sky-950/40",
        icon: "text-sky-600 dark:text-sky-400",
        size: "text-sky-400 dark:text-sky-500",
      };
    }
    if (isDoc) {
      return {
        border: isPathRef
          ? "border-dashed border-red-400/50 dark:border-red-500/40"
          : "border-red-200 dark:border-red-800",
        bg: "bg-red-50 dark:bg-red-950/40",
        icon: "text-red-600 dark:text-red-400",
        size: "text-red-400 dark:text-red-500",
      };
    }
    // Default (other files)
    return {
      border: isPathRef ? "border-dashed border-muted-foreground/40" : "border-border",
      bg: "bg-muted/50",
      icon: "text-muted-foreground",
      size: "text-muted-foreground",
    };
  });
</script>

<svelte:window onkeydown={handleLightboxKeydown} />

<div
  class="group flex items-center gap-2 rounded-md border {colorClasses.border} {colorClasses.bg} px-1.5 py-1 text-xs"
>
  {#if isImage && contentBase64}
    <button
      type="button"
      class="shrink-0 cursor-zoom-in rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
      onclick={openLightbox}
      aria-label={`放大查看 ${name}`}
      title="点击放大查看"
    >
      <img
        src="data:{mimeType};base64,{contentBase64}"
        alt={name}
        class="h-12 w-16 rounded object-cover ring-1 ring-black/5 transition-transform hover:scale-[1.03] dark:ring-white/10"
      />
    </button>
  {/if}
  {#if isDir}
    <!-- Folder icon -->
    <svg
      class="h-3.5 w-3.5 {colorClasses.icon} shrink-0"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path
        d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"
      />
    </svg>
  {:else if isImage && !contentBase64}
    <!-- Image icon -->
    <svg
      class="h-3.5 w-3.5 {colorClasses.icon} shrink-0"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
      <circle cx="9" cy="9" r="2" />
      <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
    </svg>
  {:else if isDoc}
    <!-- Document icon for PDF -->
    <svg
      class="h-3.5 w-3.5 {colorClasses.icon} shrink-0"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
      <path d="M14 2v4a2 2 0 0 0 2 2h4" />
      <path d="M10 9H8" /><path d="M16 13H8" /><path d="M16 17H8" />
    </svg>
  {:else if !isImage || !contentBase64}
    <!-- Paperclip icon for other -->
    <svg
      class="h-3.5 w-3.5 {colorClasses.icon} shrink-0"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path
        d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"
      />
    </svg>
  {/if}
  {#if !isImage || !contentBase64}
    <span class="max-w-[120px] truncate">{name}</span>
  {/if}
  {#if size > 0}
    <span class={colorClasses.size}>{formatBytes(size)}</span>
  {/if}
  {#if onremove}
    <button
      class="ml-auto shrink-0 text-muted-foreground hover:text-foreground"
      onclick={onremove}
      aria-label={t("common_removeAttachment")}
    >
      <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M18 6 6 18M6 6l12 12" />
      </svg>
    </button>
  {/if}
</div>

{#if lightboxOpen}
  <div
    class="fixed inset-0 z-[100] flex items-center justify-center bg-black/75 p-6 backdrop-blur-sm animate-in fade-in duration-150"
    role="dialog"
    aria-modal="true"
    aria-label={`预览图片 ${name}`}
    tabindex="-1"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeLightbox();
    }}
    onkeydown={(event) => {
      if (event.key === "Escape") closeLightbox();
    }}
  >
    <div class="pointer-events-auto absolute right-5 top-5 z-30 flex items-center gap-3">
      <a
        class="flex h-14 w-14 items-center justify-center rounded-full bg-white/95 text-slate-800 shadow-lg transition hover:bg-white"
        href="data:{mimeType};base64,{contentBase64}"
        download={name}
        onclick={downloadImage}
        aria-label="下载图片"
        title="下载"
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 3v12" /><path d="m7 10 5 5 5-5" /><path d="M5 21h14" />
        </svg>
      </a>
      <button
        type="button"
        class="flex h-14 w-14 items-center justify-center rounded-full bg-white/95 text-xl text-slate-800 shadow-lg transition hover:bg-white"
        onclick={handleCloseClick}
        aria-label="关闭图片预览"
        title="关闭"
      >
        ×
      </button>
    </div>
    <div
      class="relative z-0 max-h-[calc(100vh-3rem)] max-w-[calc(100vw-3rem)] overflow-auto rounded-xl bg-white p-2 shadow-2xl"
    >
      <img
        src="data:{mimeType};base64,{contentBase64}"
        alt={name}
        class="block max-h-[calc(100vh-4rem)] max-w-[calc(100vw-4rem)] origin-center object-contain transition-transform duration-200"
        style={`transform: scale(${zoom});`}
        onwheel={handleImageWheel}
        draggable="false"
      />
    </div>
    <div
      class="pointer-events-auto absolute bottom-6 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full bg-white/95 p-1.5 shadow-xl"
      role="group"
      aria-label="图片缩放控制"
    >
      <button
        type="button"
        class="flex h-11 w-11 items-center justify-center rounded-full text-slate-800 transition hover:bg-slate-100 disabled:opacity-40"
        onclick={() => changeZoom(-0.25)}
        disabled={zoom <= 0.5}
        aria-label="缩小图片"
        title="缩小"
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M5 12h14" />
        </svg>
      </button>
      <button
        type="button"
        class="min-w-16 rounded-full px-2 py-2 text-center text-sm font-medium text-slate-700 transition hover:bg-slate-100"
        onclick={resetZoom}
        aria-label="恢复图片到 100%"
        title="恢复 100%"
        aria-live="polite"
      >
        {Math.round(zoom * 100)}%
      </button>
      <button
        type="button"
        class="flex h-11 w-11 items-center justify-center rounded-full text-slate-800 transition hover:bg-slate-100 disabled:opacity-40"
        onclick={() => changeZoom(0.25)}
        disabled={zoom >= 3}
        aria-label="放大图片"
        title="放大"
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 5v14M5 12h14" />
        </svg>
      </button>
    </div>
  </div>
{/if}
