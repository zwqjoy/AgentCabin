<script lang="ts">
  export type QuestionPromptOption = {
    label: string;
    value: string;
    description?: string;
  };

  export type QuestionPrompt = {
    id: string;
    type: "select" | "confirm" | "input" | "editor";
    question: string;
    header?: string;
    options?: QuestionPromptOption[];
    multiSelect?: boolean;
    allowOther?: boolean;
    placeholder?: string;
    prefill?: string;
    required?: boolean;
  };

  type AnswerValue = string | string[] | boolean;

  interface Props {
    questions: QuestionPrompt[];
    eyebrow?: string;
    title?: string;
    description?: string;
    review?: boolean;
    embedded?: boolean;
    readOnly?: boolean;
    submitLabel?: string;
    cancelLabel?: string;
    onSubmit?: (answers: Record<string, AnswerValue>) => void | Promise<void>;
    onCancel?: () => void | Promise<void>;
  }

  let {
    questions,
    eyebrow = "AgentCabin 内置问题",
    title = "请补充一点信息",
    description = "回答后任务会继续执行。",
    review = false,
    embedded = false,
    readOnly = false,
    submitLabel = "提交",
    cancelLabel = "取消",
    onSubmit,
    onCancel,
  }: Props = $props();

  let activeIndex = $state(0);
  let reviewPage = $state(false);
  let submitting = $state(false);
  let values = $state<Record<string, AnswerValue>>({});
  let customValues = $state<Record<string, string>>({});
  let skipped = $state<Record<string, boolean>>({});
  let initializedKey = "";

  let activeQuestion = $derived(questions[activeIndex] ?? null);
  let isLastQuestion = $derived(activeIndex >= questions.length - 1);
  let allComplete = $derived(
    questions.length > 0 && questions.every((question) => questionComplete(question)),
  );

  function questionKey(): string {
    return questions.map((question) => `${question.id}:${question.prefill ?? ""}`).join("|");
  }

  function resetForQuestions() {
    const nextValues: Record<string, AnswerValue> = {};
    for (const question of questions) {
      if (question.type === "confirm") continue;
      if (question.prefill !== undefined) nextValues[question.id] = question.prefill;
    }
    values = nextValues;
    customValues = {};
    skipped = {};
    activeIndex = 0;
    reviewPage = false;
  }

  $effect(() => {
    const key = questionKey();
    if (key === initializedKey) return;
    initializedKey = key;
    resetForQuestions();
  });

  function valueFor(question: QuestionPrompt): AnswerValue | undefined {
    return values[question.id];
  }

  function questionComplete(question: QuestionPrompt): boolean {
    if (question.required === false || skipped[question.id]) return true;
    const value = valueFor(question);
    if (question.type === "confirm") return typeof value === "boolean";
    if (question.multiSelect) {
      return (
        (Array.isArray(value) && value.length > 0) ||
        (customValues[question.id] ?? "").trim().length > 0
      );
    }
    return (
      (customValues[question.id] ?? (typeof value === "string" ? value : "")).trim().length > 0
    );
  }

  function setValue(question: QuestionPrompt, value: AnswerValue) {
    values = { ...values, [question.id]: value };
    if (skipped[question.id]) {
      const nextSkipped = { ...skipped };
      delete nextSkipped[question.id];
      skipped = nextSkipped;
    }
  }

  function setCustomValue(question: QuestionPrompt, value: string) {
    customValues = { ...customValues, [question.id]: value };
    if (value.trim() && skipped[question.id]) {
      const nextSkipped = { ...skipped };
      delete nextSkipped[question.id];
      skipped = nextSkipped;
    }
  }

  function toggleOption(question: QuestionPrompt, option: string) {
    if (!question.multiSelect) {
      setValue(question, option);
      return;
    }
    const selected = Array.isArray(valueFor(question)) ? [...(valueFor(question) as string[])] : [];
    const index = selected.indexOf(option);
    if (index >= 0) selected.splice(index, 1);
    else selected.push(option);
    setValue(question, selected);
  }

  function isSelected(question: QuestionPrompt, option: string): boolean {
    const value = valueFor(question);
    return Array.isArray(value) ? value.includes(option) : value === option;
  }

  function answerSnapshot(skipState = skipped): Record<string, AnswerValue> {
    return Object.fromEntries(
      questions
        .filter((question) => !skipState[question.id])
        .map((question) => {
          const value = valueFor(question);
          const custom = (customValues[question.id] ?? "").trim();
          if (question.multiSelect) {
            return [
              question.id,
              [...(Array.isArray(value) ? value : []), ...(custom ? [custom] : [])],
            ];
          }
          return [question.id, custom || (value ?? "")];
        }),
    );
  }

  async function handlePrimary() {
    if (readOnly || submitting) return;
    if (!allComplete) {
      const firstIncomplete = questions.findIndex((question) => !questionComplete(question));
      if (firstIncomplete >= 0) {
        activeIndex = firstIncomplete;
        reviewPage = false;
      }
      return;
    }
    if (review && !reviewPage) {
      reviewPage = true;
      return;
    }
    if (!onSubmit) return;
    submitting = true;
    try {
      await onSubmit(answerSnapshot());
    } finally {
      submitting = false;
    }
  }

  function nextQuestion() {
    if (!activeQuestion || !questionComplete(activeQuestion)) return;
    if (!isLastQuestion) activeIndex += 1;
    else if (review) reviewPage = true;
  }

  function previousQuestion() {
    if (reviewPage) {
      reviewPage = false;
      activeIndex = Math.max(questions.length - 1, 0);
    } else {
      activeIndex = Math.max(activeIndex - 1, 0);
    }
  }

  async function skipQuestion() {
    if (readOnly || submitting || !activeQuestion) return;
    const nextSkipped = { ...skipped, [activeQuestion.id]: true };
    skipped = nextSkipped;
    if (!isLastQuestion) {
      activeIndex += 1;
    } else if (review) {
      reviewPage = true;
    } else if (onSubmit) {
      submitting = true;
      try {
        await onSubmit(answerSnapshot(nextSkipped));
      } finally {
        submitting = false;
      }
    }
  }
</script>

{#if questions.length > 0}
  <section
    class={embedded
      ? "w-full"
      : "overflow-hidden rounded-2xl border border-border/80 bg-card shadow-[0_12px_35px_rgb(15_23_42/0.08)]"}
    aria-label={title}
  >
    {#if !embedded}
      <div class="flex items-start gap-3 px-5 pb-2 pt-5 sm:px-6">
        <div
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary"
        >
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <circle cx="12" cy="12" r="9" />
            <path d="M12 10v6m0-9h.01" stroke-linecap="round" />
          </svg>
        </div>
        <div class="min-w-0 flex-1">
          <div class="text-[11px] font-medium tracking-wide text-muted-foreground">{eyebrow}</div>
          <h3 class="mt-1 text-base font-semibold leading-snug text-foreground">{title}</h3>
          {#if description}<p class="mt-1 text-xs leading-relaxed text-muted-foreground">
              {description}
            </p>{/if}
        </div>
        {#if onCancel}
          <button
            type="button"
            class="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            aria-label={cancelLabel}
            disabled={readOnly || submitting}
            onclick={() => onCancel?.()}
          >
            <svg
              viewBox="0 0 24 24"
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
            >
              <path d="m6 6 12 12M18 6 6 18" stroke-linecap="round" />
            </svg>
          </button>
        {/if}
      </div>
    {/if}

    <div class={embedded ? "" : "px-5 pb-5 sm:px-6"}>
      <div class="mb-4 flex items-center gap-1.5 overflow-x-auto border-b border-border/60 pb-2">
        {#each questions as question, index}
          <button
            type="button"
            class="shrink-0 rounded-lg px-2.5 py-1.5 text-[11px] transition-colors {activeIndex ===
              index && !reviewPage
              ? 'bg-primary/10 font-semibold text-primary'
              : questionComplete(question)
                ? 'text-foreground/70 hover:bg-muted'
                : 'text-muted-foreground hover:bg-muted'}"
            aria-current={activeIndex === index && !reviewPage ? "step" : undefined}
            onclick={() => {
              activeIndex = index;
              reviewPage = false;
            }}
          >
            {index + 1}. {question.header || question.id}
            {#if skipped[question.id]}
              <span class="ml-1 text-muted-foreground">—</span>
            {:else if questionComplete(question)}<span class="ml-1 text-emerald-500">✓</span>{/if}
          </button>
        {/each}
        {#if review}
          <button
            type="button"
            class="shrink-0 rounded-lg px-2.5 py-1.5 text-[11px] transition-colors {reviewPage
              ? 'bg-primary/10 font-semibold text-primary'
              : 'text-muted-foreground hover:bg-muted'}"
            aria-current={reviewPage ? "page" : undefined}
            onclick={() => (reviewPage = true)}>审阅</button
          >
        {/if}
      </div>

      {#if reviewPage}
        <div class="space-y-2 rounded-xl border border-border/60 bg-muted/20 p-3">
          {#each questions as question, index}
            <button
              type="button"
              class="block w-full rounded-lg border border-border/50 bg-background px-3 py-2.5 text-left transition-colors hover:border-primary/40"
              onclick={() => {
                activeIndex = index;
                reviewPage = false;
              }}
            >
              <div class="whitespace-pre-wrap break-words text-xs font-medium text-foreground">
                {question.question}
              </div>
              <div class="mt-1 whitespace-pre-wrap text-xs text-muted-foreground">
                {skipped[question.id]
                  ? "已跳过"
                  : String(valueFor(question) ?? customValues[question.id] ?? "未回答")}
              </div>
            </button>
          {/each}
        </div>
      {:else if activeQuestion}
        <div class="rounded-xl border border-border/60 bg-muted/20 p-4">
          <div
            class="mb-4 min-w-0 whitespace-pre-wrap break-words text-sm font-semibold leading-relaxed text-foreground"
          >
            {activeQuestion.question}
            {#if activeQuestion.required !== false}<span class="ml-1 text-destructive">*</span>{/if}
          </div>

          {#if activeQuestion.type === "select" && activeQuestion.options?.length}
            <div class="space-y-2">
              {#each activeQuestion.options as option, index}
                <button
                  type="button"
                  aria-pressed={isSelected(activeQuestion, option.value)}
                  class="flex w-full items-center gap-3 rounded-xl border px-3 py-3 text-left transition-all {isSelected(
                    activeQuestion,
                    option.value,
                  )
                    ? 'border-primary/50 bg-primary/10 shadow-sm'
                    : 'border-border/70 bg-background hover:border-primary/30 hover:bg-primary/[0.03]'}"
                  onclick={() => toggleOption(activeQuestion!, option.value)}
                >
                  <span
                    class="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-muted text-xs font-semibold text-muted-foreground"
                    >{index + 1}</span
                  >
                  <span class="min-w-0 flex-1">
                    <span class="block text-sm font-medium text-foreground">{option.label}</span>
                    {#if option.description}<span class="mt-0.5 block text-xs text-muted-foreground"
                        >{option.description}</span
                      >{/if}
                  </span>
                  {#if isSelected(activeQuestion, option.value)}<span class="text-primary">✓</span
                    >{/if}
                </button>
              {/each}
            </div>
          {:else if activeQuestion.type === "confirm"}
            <div class="grid grid-cols-2 gap-2">
              {#each [{ label: "是", value: true }, { label: "否", value: false }] as option}
                <button
                  type="button"
                  class="rounded-xl border px-3 py-3 text-sm font-medium transition-colors {valueFor(
                    activeQuestion,
                  ) === option.value
                    ? 'border-primary/50 bg-primary/10 text-primary'
                    : 'border-border/70 bg-background hover:bg-muted'}"
                  onclick={() => setValue(activeQuestion!, option.value)}>{option.label}</button
                >
              {/each}
            </div>
          {:else if activeQuestion.type === "editor"}
            <textarea
              rows="6"
              value={String(valueFor(activeQuestion) ?? "")}
              placeholder={activeQuestion.placeholder ?? "输入你的答案"}
              class="w-full resize-y rounded-xl border border-border/70 bg-background px-3 py-3 text-sm outline-none transition-colors focus:border-primary/50 focus:ring-2 focus:ring-primary/10"
              oninput={(event) =>
                setValue(activeQuestion!, (event.currentTarget as HTMLTextAreaElement).value)}
            ></textarea>
          {:else}
            <input
              type="text"
              value={String(valueFor(activeQuestion) ?? "")}
              placeholder={activeQuestion.placeholder ?? "输入你的答案"}
              class="w-full rounded-xl border border-border/70 bg-background px-3 py-3 text-sm outline-none transition-colors focus:border-primary/50 focus:ring-2 focus:ring-primary/10"
              oninput={(event) =>
                setValue(activeQuestion!, (event.currentTarget as HTMLInputElement).value)}
            />
          {/if}

          {#if activeQuestion.allowOther && activeQuestion.type === "select"}
            <input
              type="text"
              value={customValues[activeQuestion.id] ?? ""}
              placeholder="也可以自行填写"
              class="mt-3 w-full rounded-xl border border-dashed border-border/80 bg-background px-3 py-3 text-sm outline-none transition-colors focus:border-primary/50 focus:ring-2 focus:ring-primary/10"
              oninput={(event) =>
                setCustomValue(activeQuestion!, (event.currentTarget as HTMLInputElement).value)}
            />
          {/if}
        </div>
      {/if}

      <div class="mt-4 flex items-center justify-between gap-3 border-t border-border/60 pt-3">
        <div class="flex items-center gap-2 text-sm tabular-nums text-muted-foreground">
          <button
            type="button"
            class="rounded-lg p-1.5 transition-colors hover:bg-muted disabled:opacity-30"
            disabled={activeIndex === 0 && !reviewPage}
            aria-label="上一题"
            onclick={previousQuestion}>‹</button
          >
          <span>{reviewPage ? "审阅" : `${activeIndex + 1}/${questions.length}`}</span>
          <button
            type="button"
            class="rounded-lg p-1.5 transition-colors hover:bg-muted disabled:opacity-30"
            disabled={reviewPage || isLastQuestion}
            aria-label="下一题"
            onclick={nextQuestion}>›</button
          >
        </div>
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="rounded-xl border border-border px-3.5 py-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
            disabled={readOnly || submitting}
            onclick={skipQuestion}>跳过本题</button
          >
          <button
            type="button"
            class="rounded-xl bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-45"
            disabled={readOnly ||
              submitting ||
              (!allComplete && isLastQuestion && (!review || reviewPage))}
            onclick={handlePrimary}
            >{submitting
              ? "提交中…"
              : review && !reviewPage && isLastQuestion
                ? "去审阅"
                : reviewPage
                  ? submitLabel
                  : isLastQuestion
                    ? submitLabel
                    : "下一题"}</button
          >
        </div>
      </div>
    </div>
  </section>
{/if}
