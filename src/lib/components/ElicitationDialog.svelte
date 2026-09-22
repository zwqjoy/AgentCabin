<script lang="ts">
  import { platform } from "$lib/platform";
  import type { ElicitationState } from "$lib/stores/session-store.svelte";
  import type { ElicitationFieldSchema } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import {
    encodePiBatchAskResponse,
    parsePiBatchAskEnvelope,
    type PiBatchAnswer,
    type PiBatchQuestion,
  } from "$lib/utils/pi-batch-ask";

  let {
    elicitations,
    onRespond,
    surface = "default",
  }: {
    elicitations: Map<string, ElicitationState>;
    surface?: "default" | "work";
    onRespond: (
      requestId: string,
      action: "accept" | "decline" | "cancel",
      content?: Record<string, unknown>,
    ) => void | Promise<void>;
  } = $props();

  let submitting = $state(false);

  // Queue strategy: show the first pending elicitation
  let current = $derived.by(() => {
    const iter = elicitations.values();
    const first = iter.next();
    return first.done ? null : first.value;
  });
  const autoResolvedLegacyPlanRequests = new Set<string>();
  let isLegacyWorkPlanApproval = $derived(
    surface === "work" &&
      current?.mode === "pi_extension_select" &&
      current.message?.startsWith("Work 计划确认") === true,
  );
  $effect(() => {
    if (
      !current ||
      !isLegacyWorkPlanApproval ||
      autoResolvedLegacyPlanRequests.has(current.requestId)
    ) {
      return;
    }
    const requestId = current.requestId;
    autoResolvedLegacyPlanRequests.add(requestId);
    void Promise.resolve(onRespond(requestId, "accept", { value: "确认执行" })).catch(() => {
      autoResolvedLegacyPlanRequests.delete(requestId);
    });
  });
  const batchAsk = $derived(parsePiBatchAskEnvelope(current?.message));
  const isBatchAsk = $derived(batchAsk !== null);
  const isPiExtensionPrompt = $derived(
    current?.mcpServerName === "Pi extension" ||
      current?.mcpServerName === "Pi extension UI" ||
      current?.mode?.startsWith("pi_extension_") === true,
  );
  const isWorkSurface = $derived(surface === "work");
  const batchQuestions = $derived(batchAsk?.questions ?? []);
  const PI_PERMISSION_OPTIONS = new Set([
    "Yes",
    "Yes, for this session",
    "No",
    "No, provide reason",
  ]);
  let isPiConfirm = $derived(current?.mode === "pi_extension_confirm");
  let isWorkConfirm = $derived(isWorkSurface && isPiConfirm);
  let isWorkspaceKnowledgeProposal = $derived(
    isPiConfirm && current?.message?.includes("拟写入内容") === true,
  );
  let isPiEditor = $derived(current?.mode === "pi_extension_editor");
  let isPiPermissionSelect = $derived(
    current?.mode === "pi_extension_select" &&
      (current.requestedSchema?.properties?.value?.enum ?? []).some((option) =>
        PI_PERMISSION_OPTIONS.has(option),
      ),
  );
  let piPermissionOptions = $derived(current?.requestedSchema?.properties?.value?.enum ?? []);

  // Pi extension hosts may attach an absolute expiry to a request. When that
  // deadline is reached, cancel the request through the same response path as
  // the visible Cancel button so the worker is never left waiting forever.
  $effect(() => {
    const expiresAt = current?.expiresAt;
    if (!expiresAt || !current) return;
    const deadline = Date.parse(expiresAt);
    if (!Number.isFinite(deadline)) return;

    const requestId = current.requestId;
    const remaining = deadline - Date.now();
    const cancel = () => {
      if (current?.requestId !== requestId) return;
      void Promise.resolve(onRespond(requestId, "cancel")).catch((error) => {
        dbgWarn("ElicitationDialog", "expired request cancellation failed", error);
      });
    };

    if (remaining <= 0) {
      cancel();
      return;
    }

    const timeout = window.setTimeout(cancel, remaining);
    return () => window.clearTimeout(timeout);
  });

  // Form state for schema fields
  let formValues = $state<Record<string, unknown>>({});
  let activeBatchTab = $state(0);
  type BatchValue = string | string[] | boolean;
  let batchValues = $state<Record<string, BatchValue>>({});
  let batchCustom = $state<Record<string, boolean>>({});
  let batchCustomValues = $state<Record<string, string>>({});
  const isBatchReviewTab = $derived(
    batchAsk?.review === true && activeBatchTab >= batchQuestions.length,
  );
  const activeBatchQuestion = $derived(batchQuestions[activeBatchTab] ?? null);

  // Reset form when current elicitation changes
  $effect(() => {
    if (current) {
      const defaults: Record<string, unknown> = {};
      const props = current.requestedSchema?.properties;
      if (props) {
        for (const [key, field] of Object.entries(props)) {
          if (field.default !== undefined) {
            defaults[key] = field.default;
          } else if (field.type === "boolean") {
            defaults[key] = false;
          } else if (field.type === "number") {
            defaults[key] = 0;
          } else {
            defaults[key] = "";
          }
        }
      }
      formValues = defaults;
      activeBatchTab = 0;
      batchValues = {};
      batchCustom = {};
      batchCustomValues = {};
      if (batchAsk) {
        for (const question of batchAsk.questions) {
          if (question.type === "editor" && question.prefill !== undefined) {
            batchValues[question.id] = question.prefill;
          }
        }
      }
    }
  });

  function updateField(key: string, value: unknown) {
    formValues = { ...formValues, [key]: value };
  }

  // Check required fields against current schema
  let missingRequired = $derived.by(() => {
    if (!current?.requestedSchema) return [];
    const required = current.requestedSchema.required ?? [];
    const props = current.requestedSchema.properties ?? {};
    return required.filter((key) => {
      if (!(key in props)) return false;
      const val = formValues[key];
      if (val === undefined || val === null || val === "") return true;
      return false;
    });
  });

  async function handleAccept() {
    if (!current || submitting) return;
    if (missingRequired.length > 0) {
      dbgWarn("ElicitationDialog", "required fields missing", { keys: missingRequired });
      return;
    }
    submitting = true;
    dbg("ElicitationDialog", "accept", { requestId: current.requestId });
    try {
      await onRespond(current.requestId, "accept", formValues);
    } catch (e) {
      dbgWarn("ElicitationDialog", "accept error", e);
    } finally {
      submitting = false;
    }
  }

  async function handleDecline() {
    if (!current || submitting) return;
    submitting = true;
    dbg("ElicitationDialog", "decline", { requestId: current.requestId });
    try {
      await onRespond(current.requestId, "decline");
    } catch (e) {
      dbgWarn("ElicitationDialog", "decline error", e);
    } finally {
      submitting = false;
    }
  }

  async function handlePiConfirm(confirmed: boolean) {
    if (!current || submitting) return;
    submitting = true;
    dbg("ElicitationDialog", "pi confirm", { requestId: current.requestId, confirmed });
    try {
      await onRespond(current.requestId, "accept", { confirmed });
    } catch (e) {
      dbgWarn("ElicitationDialog", "pi confirm error", e);
    } finally {
      submitting = false;
    }
  }

  function piPermissionLabel(option: string): string {
    switch (option) {
      case "Yes":
        return t("elicitation_piPermissionOnce");
      case "Yes, for this session":
        return t("elicitation_piPermissionSession");
      case "No":
        return t("elicitation_piPermissionDeny");
      case "No, provide reason":
        return t("elicitation_piPermissionDenyReason");
      default:
        return option;
    }
  }

  async function handlePiPermissionChoice(value: string) {
    if (!current || submitting) return;
    submitting = true;
    dbg("ElicitationDialog", "pi permission choice", {
      requestId: current.requestId,
      value,
    });
    try {
      // Keep the extension's exact option value. The Pi permission extension
      // uses "Yes, for this session" to record an in-memory session rule.
      await onRespond(current.requestId, "accept", { value });
    } catch (e) {
      dbgWarn("ElicitationDialog", "pi permission choice error", e);
    } finally {
      submitting = false;
    }
  }

  async function openElicitationUrl(href: string) {
    // Protocol whitelist — block file://, javascript://, etc.
    try {
      const url = new URL(href);
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        dbgWarn("ElicitationDialog", "blocked non-http(s) URL", { href });
        return;
      }
    } catch {
      return;
    }
    // Open external auth URL in the system browser
    try {
      await platform.shell.openExternal(href);
    } catch {
      window.open(href, "_blank");
    }
  }

  function renderFieldType(field: ElicitationFieldSchema): string {
    if (field.enum && field.enum.length > 0) return "enum";
    return field.type ?? "string";
  }

  function batchQuestionComplete(question: PiBatchQuestion): boolean {
    const value = batchValues[question.id];
    if (question.type === "confirm") return typeof value === "boolean";
    if (question.multiSelect) {
      return (
        (Array.isArray(value) && value.some((item) => item.trim().length > 0)) ||
        (batchCustom[question.id] === true &&
          (batchCustomValues[question.id] ?? "").trim().length > 0)
      );
    }
    return typeof value === "string" && value.trim().length > 0;
  }

  const batchComplete = $derived(
    batchQuestions.length > 0 &&
      batchQuestions.every((question) => batchQuestionComplete(question)),
  );

  function setBatchValue(question: PiBatchQuestion, value: BatchValue) {
    batchValues = { ...batchValues, [question.id]: value };
  }

  function setBatchCustomValue(question: PiBatchQuestion, value: string) {
    batchCustomValues = { ...batchCustomValues, [question.id]: value };
  }

  function setBatchSelectValue(question: PiBatchQuestion, value: string) {
    if (value === "__other__") {
      batchCustom = { ...batchCustom, [question.id]: true };
      if (question.multiSelect && !Array.isArray(batchValues[question.id])) {
        setBatchValue(question, []);
      } else if (!question.multiSelect) {
        setBatchValue(question, "");
      }
    } else if (question.multiSelect) {
      const current: string[] = Array.isArray(batchValues[question.id])
        ? [...(batchValues[question.id] as string[])]
        : [];
      const selected = current.includes(value)
        ? current.filter((item) => item !== value)
        : [...current, value];
      batchCustom = { ...batchCustom, [question.id]: false };
      setBatchValue(question, selected);
    } else {
      batchCustom = { ...batchCustom, [question.id]: false };
      setBatchValue(question, value);
    }
  }

  function batchAnswers(): PiBatchAnswer[] {
    return batchQuestions.map((question) => {
      const rawValue = batchValues[question.id];
      const customValue = batchCustom[question.id] ? (batchCustomValues[question.id] ?? "") : "";
      const value = question.multiSelect
        ? [
            ...(Array.isArray(rawValue) ? rawValue : []),
            ...(customValue.trim() ? [customValue] : []),
          ]
        : rawValue;
      const answer: PiBatchAnswer = {
        id: question.id,
        type: question.type,
        value: value ?? null,
      };
      if (question.type === "select") {
        const values = Array.isArray(value) ? value : [value];
        const labels = values
          .filter((item): item is string => typeof item === "string")
          .map((item) => question.options?.find((option) => option.value === item)?.label ?? item);
        answer.label = labels.length > 0 ? labels.join(", ") : undefined;
        answer.wasCustom = batchCustom[question.id] === true;
      }
      return answer;
    });
  }

  async function handleBatchAccept() {
    if (!current || !batchAsk || submitting || !batchComplete) return;
    submitting = true;
    dbg("ElicitationDialog", "batch accept", { requestId: current.requestId });
    try {
      await onRespond(current.requestId, "accept", {
        value: encodePiBatchAskResponse(batchAnswers()),
      });
    } catch (e) {
      dbgWarn("ElicitationDialog", "batch accept error", e);
    } finally {
      submitting = false;
    }
  }

  function goToFirstIncompleteBatchQuestion() {
    const index = batchQuestions.findIndex((question) => !batchQuestionComplete(question));
    if (index >= 0) activeBatchTab = index;
  }

  async function handleBatchPrimaryAction() {
    if (!batchAsk || submitting) return;
    if (!batchComplete) {
      goToFirstIncompleteBatchQuestion();
      return;
    }
    if (batchAsk.review && !isBatchReviewTab) {
      activeBatchTab = batchQuestions.length;
      return;
    }
    await handleBatchAccept();
  }
</script>

{#if current && !isLegacyWorkPlanApproval}
  <div class="mx-auto w-full py-2 {isWorkSurface ? 'max-w-6xl sm:py-3' : 'max-w-lg'}">
    <div
      class="transition-all animate-in fade-in zoom-in-95 duration-150 {isWorkSurface
        ? 'rounded-3xl border border-border/60 bg-background p-4 shadow-[0_8px_30px_rgb(0_0_0/0.08)] sm:p-5'
        : 'rounded-xl border border-primary/30 bg-card p-4 shadow-lg'}"
      role="dialog"
      aria-label={isWorkspaceKnowledgeProposal
        ? "确认保存 Workspace 知识"
        : isWorkConfirm
          ? "确认 Work 操作"
          : isWorkSurface
            ? "Work 需要输入"
            : isPiExtensionPrompt
              ? "Pi 扩展输入"
              : t("elicitation_title")}
    >
      <!-- Header -->
      <div class="mb-3 flex items-center gap-2.5">
        <div
          class="flex h-7 w-7 items-center justify-center rounded-lg bg-primary/10 text-xs font-bold text-primary shrink-0"
        >
          ?
        </div>
        <div class="flex-1 min-w-0">
          <div class="text-sm font-semibold text-foreground truncate">
            {isWorkspaceKnowledgeProposal
              ? "确认保存 Workspace 知识"
              : isWorkConfirm
                ? "确认 Work 操作"
                : isWorkSurface
                  ? "Work 需要输入"
                  : isPiPermissionSelect
                    ? t("elicitation_piPermissionTitle")
                    : isPiConfirm
                      ? "确认操作"
                      : isBatchAsk || isPiExtensionPrompt
                        ? "Pi 扩展需要输入"
                        : t("elicitation_title")}
          </div>
          <div class="text-xs text-muted-foreground truncate">
            {isWorkSurface
              ? isWorkConfirm
                ? "Work 权限控制"
                : "Work 运行时输入"
              : current.mcpServerName}
            {#if elicitations.size > 1}
              <span class="ml-1 text-muted-foreground/70">
                ({t("elicitation_pending", { count: String(elicitations.size) })})
              </span>
            {/if}
          </div>
        </div>
      </div>

      <!-- Message -->
      {#if isBatchAsk}
        <p
          class="mb-3 whitespace-pre-wrap text-xs text-foreground/90 leading-relaxed bg-muted/30 p-2.5 rounded-lg border border-border/40"
        >
          {#if batchAsk?.title}
            <span class="font-medium">{batchAsk.title}</span><br />
          {/if}
          请完成下面的问题{batchAsk?.review ? "，最后在审阅页提交" : ""}。
        </p>
      {:else if current.message}
        <p
          class="mb-3 whitespace-pre-wrap text-xs text-foreground/90 leading-relaxed bg-muted/30 p-2.5 rounded-lg border border-border/40"
        >
          {current.message}
        </p>
      {/if}

      <!-- URL mode -->
      {#if current.url}
        <div class="mb-3">
          <button
            class="text-xs font-medium text-primary underline hover:text-primary/80 transition-colors"
            onclick={() => current?.url && openElicitationUrl(current.url)}
          >
            {t("elicitation_open_url")}
          </button>
        </div>
      {/if}

      <!-- Pi permission system's select is a permission decision, not generic input. -->
      {#if isPiPermissionSelect}
        <div class="mb-3 space-y-2 border-t border-border/40 pt-3">
          {#each piPermissionOptions as option}
            <button
              class="flex w-full items-center justify-between rounded-lg border border-border px-3 py-2 text-left text-xs transition-colors hover:bg-accent disabled:opacity-50
                {option === 'Yes, for this session'
                ? 'border-primary/40 bg-primary/5 text-primary'
                : option.startsWith('No')
                  ? 'text-destructive hover:bg-destructive/5'
                  : 'text-foreground'}"
              disabled={submitting}
              onclick={() => handlePiPermissionChoice(option)}
            >
              <span>{piPermissionLabel(option)}</span>
              {#if option === "Yes, for this session"}
                <span class="text-[10px] text-primary/70">
                  {t("elicitation_piPermissionSessionHint")}
                </span>
              {:else if option === "Yes"}
                <span class="text-[10px] text-muted-foreground">
                  {t("elicitation_piPermissionOnceHint")}
                </span>
              {/if}
            </button>
          {/each}
        </div>
        <div class="flex items-center justify-end border-t border-border/40 pt-2">
          <button
            class="rounded-lg border border-border bg-muted/40 px-4 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50"
            disabled={submitting}
            onclick={handleDecline}
          >
            {t("elicitation_cancel")}
          </button>
        </div>
      {:else if isBatchAsk}
        <div class="mb-3 space-y-3">
          <div class="flex gap-1 overflow-x-auto border-b border-border/40 pb-1">
            {#each batchQuestions as question, index}
              <button
                type="button"
                class="shrink-0 rounded-md px-2.5 py-1.5 text-[11px] transition-colors
                  {activeBatchTab === index
                  ? 'bg-primary/10 text-primary font-medium'
                  : batchQuestionComplete(question)
                    ? 'text-foreground/70 hover:bg-accent'
                    : 'text-muted-foreground hover:bg-accent'}"
                aria-selected={activeBatchTab === index}
                onclick={() => (activeBatchTab = index)}
              >
                {index + 1}. {question.id}
                {#if batchQuestionComplete(question)}
                  <span class="ml-1 text-emerald-500">✓</span>
                {/if}
              </button>
            {/each}
            {#if batchAsk?.review}
              <button
                type="button"
                class="shrink-0 rounded-md px-2.5 py-1.5 text-[11px] transition-colors
                  {isBatchReviewTab
                  ? 'bg-primary/10 text-primary font-medium'
                  : 'text-muted-foreground hover:bg-accent'}"
                aria-selected={isBatchReviewTab}
                onclick={() => (activeBatchTab = batchQuestions.length)}
              >
                审阅
              </button>
            {/if}
          </div>

          {#if isBatchReviewTab}
            <div class="space-y-2 rounded-lg border border-border/40 bg-muted/20 p-3">
              {#each batchQuestions as question}
                <div class="rounded-md border border-border/40 bg-background p-2.5">
                  <div class="text-[11px] font-medium text-foreground/80">{question.question}</div>
                  <div class="mt-1 whitespace-pre-wrap text-xs text-foreground">
                    {String(batchValues[question.id] ?? "")}
                  </div>
                </div>
              {/each}
            </div>
          {:else if activeBatchQuestion}
            <div class="rounded-lg border border-border/40 bg-muted/20 p-3">
              <div class="mb-2 text-xs font-medium leading-relaxed text-foreground/90">
                {activeBatchQuestion.question}
                {#if activeBatchQuestion.type !== "confirm"}
                  <span class="text-destructive">*</span>
                {/if}
              </div>

              {#if activeBatchQuestion.type === "select"}
                <div class="space-y-2">
                  {#each activeBatchQuestion.options ?? [] as option}
                    {@const activeValue = batchValues[activeBatchQuestion.id]}
                    {@const optionSelected = activeBatchQuestion.multiSelect
                      ? Array.isArray(activeValue) && activeValue.includes(option.value)
                      : activeValue === option.value}
                    <button
                      type="button"
                      class="flex w-full items-start justify-between gap-3 rounded-lg border px-3 py-2 text-left text-xs transition-colors
                        {optionSelected && !batchCustom[activeBatchQuestion.id]
                        ? 'border-primary/50 bg-primary/5 text-primary'
                        : 'border-border hover:bg-accent'}"
                      onclick={() => setBatchSelectValue(activeBatchQuestion, option.value)}
                    >
                      <span>{option.label}</span>
                      {#if option.description}
                        <span class="text-[10px] text-muted-foreground">{option.description}</span>
                      {/if}
                    </button>
                  {/each}
                  {#if activeBatchQuestion.allowOther !== false}
                    <button
                      type="button"
                      class="w-full rounded-lg border border-dashed px-3 py-2 text-left text-xs transition-colors hover:bg-accent
                        {batchCustom[activeBatchQuestion.id]
                        ? 'border-primary/50 bg-primary/5 text-primary'
                        : 'border-border text-muted-foreground'}"
                      onclick={() => setBatchSelectValue(activeBatchQuestion, "__other__")}
                    >
                      ✎ 自行输入...
                    </button>
                  {/if}
                  {#if batchCustom[activeBatchQuestion.id]}
                    <input
                      type="text"
                      value={activeBatchQuestion.multiSelect
                        ? (batchCustomValues[activeBatchQuestion.id] ?? "")
                        : String(batchValues[activeBatchQuestion.id] ?? "")}
                      oninput={(e) =>
                        activeBatchQuestion?.multiSelect
                          ? setBatchCustomValue(
                              activeBatchQuestion,
                              (e.target as HTMLInputElement).value,
                            )
                          : setBatchValue(
                              activeBatchQuestion!,
                              (e.target as HTMLInputElement).value,
                            )}
                      class="w-full rounded-lg border border-input bg-background px-3 py-2 text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
                      placeholder="请输入自定义答案"
                    />
                  {/if}
                </div>
              {:else if activeBatchQuestion.type === "confirm"}
                <div class="grid grid-cols-2 gap-2">
                  <button
                    type="button"
                    class="rounded-lg border px-3 py-2 text-xs transition-colors hover:bg-accent
                      {batchValues[activeBatchQuestion.id] === true
                      ? 'border-primary/50 bg-primary/5 text-primary'
                      : 'border-border'}"
                    onclick={() => setBatchValue(activeBatchQuestion!, true)}
                  >
                    是
                  </button>
                  <button
                    type="button"
                    class="rounded-lg border px-3 py-2 text-xs transition-colors hover:bg-accent
                      {batchValues[activeBatchQuestion.id] === false
                      ? 'border-primary/50 bg-primary/5 text-primary'
                      : 'border-border'}"
                    onclick={() => setBatchValue(activeBatchQuestion!, false)}
                  >
                    否
                  </button>
                </div>
              {:else if activeBatchQuestion.type === "editor"}
                <textarea
                  rows="6"
                  value={String(
                    batchValues[activeBatchQuestion.id] ?? activeBatchQuestion.prefill ?? "",
                  )}
                  oninput={(e) =>
                    setBatchValue(activeBatchQuestion!, (e.target as HTMLTextAreaElement).value)}
                  class="w-full resize-y rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
                  placeholder={activeBatchQuestion.placeholder ?? "请输入内容"}
                ></textarea>
              {:else}
                <input
                  type="text"
                  value={String(batchValues[activeBatchQuestion.id] ?? "")}
                  oninput={(e) =>
                    setBatchValue(activeBatchQuestion!, (e.target as HTMLInputElement).value)}
                  class="w-full rounded-lg border border-input bg-background px-3 py-2 text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
                  placeholder={activeBatchQuestion.placeholder ?? "请输入答案"}
                />
              {/if}
            </div>
          {/if}
        </div>
      {:else if current.requestedSchema?.properties}
        <div class="mb-3 space-y-3">
          {#each Object.entries(current.requestedSchema.properties) as [key, field]}
            {@const fieldType = renderFieldType(field)}
            {@const isRequired = current.requestedSchema?.required?.includes(key) ?? field.required}
            <div>
              <label class="mb-1 block text-xs font-medium text-foreground/90" for="elic-{key}">
                {field.title ?? key}
                {#if isRequired}
                  <span class="text-destructive">*</span>
                {/if}
              </label>
              {#if field.description}
                <p class="mb-1 text-[11px] text-muted-foreground">{field.description}</p>
              {/if}

              {#if fieldType === "boolean"}
                <label class="flex items-center gap-2 cursor-pointer">
                  <input
                    id="elic-{key}"
                    type="checkbox"
                    checked={!!formValues[key]}
                    onchange={(e) => updateField(key, (e.target as HTMLInputElement).checked)}
                    class="h-4 w-4 rounded border-input text-primary focus:ring-primary/30"
                  />
                  <span class="text-xs text-muted-foreground">{field.title ?? key}</span>
                </label>
              {:else if fieldType === "enum" && field.enum}
                <select
                  id="elic-{key}"
                  value={String(formValues[key] ?? "")}
                  onchange={(e) => updateField(key, (e.target as HTMLSelectElement).value)}
                  class="w-full rounded-lg border border-input bg-background px-3 py-2 text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40 transition-colors cursor-pointer"
                >
                  <option value="">-- 请选择 --</option>
                  {#each field.enum as opt}
                    <option value={opt}>{opt}</option>
                  {/each}
                </select>
              {:else if fieldType === "number"}
                <input
                  id="elic-{key}"
                  type="number"
                  value={formValues[key] as number}
                  oninput={(e) => updateField(key, Number((e.target as HTMLInputElement).value))}
                  class="w-full rounded-lg border border-input bg-background px-3 py-2 text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40 transition-colors"
                />
              {:else if isPiEditor && key === "value"}
                <textarea
                  id="elic-{key}"
                  rows="6"
                  value={String(formValues[key] ?? "")}
                  oninput={(e) => updateField(key, (e.target as HTMLTextAreaElement).value)}
                  class="w-full resize-y rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40 transition-colors"
                  placeholder={field.title ?? key}
                ></textarea>
              {:else}
                <input
                  id="elic-{key}"
                  type="text"
                  value={String(formValues[key] ?? "")}
                  oninput={(e) => updateField(key, (e.target as HTMLInputElement).value)}
                  class="w-full rounded-lg border border-input bg-background px-3 py-2 text-xs text-foreground shadow-sm focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40 transition-colors"
                  placeholder={field.title ?? key}
                />
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <!-- Actions -->
      {#if isPiConfirm}
        <div class="flex items-center justify-end gap-2 pt-1 border-t border-border/40">
          <button
            class="rounded-lg border border-border bg-muted/40 px-4 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors disabled:opacity-50"
            disabled={submitting}
            onclick={() => handlePiConfirm(false)}
          >
            {isWorkspaceKnowledgeProposal ? "暂不保存" : t("statusbar_no")}
          </button>
          <button
            class="rounded-lg bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shadow-sm disabled:opacity-50"
            disabled={submitting}
            onclick={() => handlePiConfirm(true)}
          >
            {submitting ? "..." : isWorkspaceKnowledgeProposal ? "确认保存" : t("statusbar_yes")}
          </button>
        </div>
      {:else if isBatchAsk}
        <div class="flex items-center justify-end gap-2 pt-1 border-t border-border/40">
          <button
            class="rounded-lg border border-border bg-muted/40 px-4 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors disabled:opacity-50"
            disabled={submitting}
            onclick={handleDecline}
          >
            取消
          </button>
          <button
            class="rounded-lg bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shadow-sm disabled:opacity-50"
            disabled={submitting || !batchComplete}
            onclick={handleBatchPrimaryAction}
          >
            {submitting ? "..." : batchAsk?.review && !isBatchReviewTab ? "去审阅" : "提交"}
          </button>
        </div>
      {:else if !isPiPermissionSelect}
        <div class="flex items-center justify-end gap-2 pt-1 border-t border-border/40">
          <button
            class="rounded-lg border border-border bg-muted/40 px-4 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors disabled:opacity-50"
            disabled={submitting}
            onclick={handleDecline}
          >
            {t("elicitation_decline")}
          </button>
          <button
            class="rounded-lg bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors shadow-sm disabled:opacity-50"
            disabled={submitting || missingRequired.length > 0}
            onclick={handleAccept}
          >
            {submitting ? "..." : t("elicitation_accept")}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
