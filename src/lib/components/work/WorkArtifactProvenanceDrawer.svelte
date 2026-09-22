<script lang="ts">
  import type { WorkArtifactSummary } from "$lib/types/work";

  interface Props {
    artifact: WorkArtifactSummary | null;
    open: boolean;
    onClose: () => void;
  }

  let { artifact, open, onClose }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      onClose();
    }
  }

  function formatBytes(bytes?: number): string {
    if (!bytes && bytes !== 0) return "-";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && artifact}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm transition-opacity"
    onclick={onClose}
    role="presentation"
  ></div>

  <!-- Drawer -->
  <div
    class="fixed inset-y-0 right-0 z-50 flex w-full max-w-md flex-col bg-background p-6 shadow-2xl border-l border-border transition-transform"
    role="dialog"
    aria-modal="true"
    aria-labelledby="artifact-details-title"
  >
    <!-- Header -->
    <div class="flex items-start justify-between gap-4 pb-4 border-b border-border">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2 mb-1">
          <span
            class="rounded bg-primary/10 px-2 py-0.5 text-[10px] font-mono uppercase text-primary font-medium"
          >
            v{artifact.version || 1}
          </span>
          <span
            class="rounded bg-muted px-2 py-0.5 text-[10px] font-mono capitalize text-muted-foreground"
          >
            {artifact.status}
          </span>
        </div>
        <h2 id="artifact-details-title" class="text-base font-bold text-foreground truncate">
          {artifact.title || artifact.path}
        </h2>
        <p class="text-xs font-mono text-muted-foreground truncate mt-0.5">
          {artifact.path}
        </p>
      </div>

      <button
        type="button"
        class="rounded-lg p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={onClose}
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto py-4 space-y-5 text-xs">
      <!-- File Metadata -->
      <div>
        <h3 class="font-semibold text-foreground mb-2">文件属性 (Properties)</h3>
        <div
          class="rounded-lg border border-border bg-muted/20 p-3 space-y-1.5 font-mono text-[11px]"
        >
          <div class="flex justify-between">
            <span class="text-muted-foreground">Category:</span>
            <span class="text-foreground capitalize">{artifact.category}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-muted-foreground">MIME:</span>
            <span class="text-foreground">{artifact.mimeType || "-"}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-muted-foreground">Size:</span>
            <span class="text-foreground">{formatBytes(artifact.size)}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-muted-foreground">Updated At:</span>
            <span class="text-foreground">{artifact.updatedAt || "-"}</span>
          </div>
        </div>
      </div>

      <!-- Provenance: Produced By -->
      <div>
        <h3 class="font-semibold text-foreground mb-2">生成来源 (Produced By)</h3>
        <div
          class="rounded-lg border border-border bg-muted/20 p-3 space-y-1.5 font-mono text-[11px]"
        >
          <div class="flex justify-between">
            <span class="text-muted-foreground">Run ID:</span>
            <span class="text-foreground">{artifact.runId || "-"}</span>
          </div>
          {#if artifact.producer}
            {#if artifact.producer.producerToolCallId}
              <div class="flex justify-between">
                <span class="text-muted-foreground">Tool Call:</span>
                <span class="text-foreground truncate max-w-[200px]"
                  >{artifact.producer.producerToolCallId}</span
                >
              </div>
            {/if}
            {#if artifact.producer.executionId}
              <div class="flex justify-between">
                <span class="text-muted-foreground">Execution ID:</span>
                <span class="text-foreground truncate max-w-[200px]"
                  >{artifact.producer.executionId}</span
                >
              </div>
            {/if}
          {/if}
        </div>
      </div>

      <!-- Evidence & Verification -->
      <div>
        <h3 class="font-semibold text-foreground mb-2">校验与哈希 (Verification)</h3>
        <div class="rounded-lg border border-border bg-muted/20 p-3 space-y-2 text-[11px]">
          <div>
            <span class="text-muted-foreground block text-[10px] mb-0.5">SHA256 校验和:</span>
            <div
              class="rounded bg-background p-1.5 font-mono text-[10px] break-all border border-border/60 text-foreground"
            >
              {artifact.evidence?.sha256 || "-"}
            </div>
          </div>

          {#if artifact.validationSummary}
            <div class="pt-1.5 border-t border-border/40">
              <span class="text-muted-foreground block mb-1">校验结果摘要:</span>
              <p class="text-foreground bg-background/80 p-2 rounded border border-border/50">
                {artifact.validationSummary}
              </p>
            </div>
          {/if}

          {#if artifact.evidence}
            <div class="pt-1.5 border-t border-border/40 space-y-1 font-mono text-[10px]">
              <div class="flex justify-between">
                <span class="text-muted-foreground">Validator:</span>
                <span>{artifact.evidence.validatorVersion || "-"}</span>
              </div>
              <div class="flex justify-between">
                <span class="text-muted-foreground">Verified At:</span>
                <span>{artifact.evidence.verifiedAt || "-"}</span>
              </div>
              {#if artifact.evidence.checks && artifact.evidence.checks.length > 0}
                <div class="pt-1">
                  <span class="text-muted-foreground block mb-1">通过检查项:</span>
                  <div class="space-y-0.5">
                    {#each artifact.evidence.checks as check}
                      <div class="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400">
                        <span>✓</span>
                        <span>{check}</span>
                      </div>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </div>

      <!-- Sources -->
      {#if artifact.sources && artifact.sources.length > 0}
        <div>
          <h3 class="font-semibold text-foreground mb-2">输入溯源 (Sources)</h3>
          <div class="space-y-1.5">
            {#each artifact.sources as source}
              <div
                class="rounded-lg border border-border/60 bg-muted/20 p-2.5 text-[11px] font-mono"
              >
                <div class="flex items-center justify-between">
                  <span class="font-semibold text-foreground capitalize">{source.sourceType}</span>
                  <span class="text-[10px] text-muted-foreground">{source.capturedAt}</span>
                </div>
                {#if source.url}
                  <div class="text-primary truncate mt-1">
                    <a href={source.url} target="_blank" rel="noreferrer" class="hover:underline">
                      {source.url}
                    </a>
                  </div>
                {/if}
                <div class="text-[10px] text-muted-foreground truncate mt-0.5">
                  Digest: {source.resultDigest}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="pt-4 border-t border-border flex justify-end">
      <button
        type="button"
        class="rounded-lg bg-muted px-4 py-1.5 text-xs font-medium text-foreground hover:bg-muted/80 transition-colors"
        onclick={onClose}
      >
        关闭
      </button>
    </div>
  </div>
{/if}
