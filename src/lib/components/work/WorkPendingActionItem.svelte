<script lang="ts">
  import { platform } from "$lib/platform";
  import type { InboxItem, InboxItemStatus } from "$lib/types/work";
  import { authorizeWorkApp, startWorkConnectorAuth } from "$lib/api/work";
  import {
    buildStandingRuleFromProposal,
    formatDirectoryPath,
    getDirectoryPathFromItem,
    getInteractionDescription,
    getInteractionTitle,
    isAccessRootRequest,
    isAppConnectionRequest,
    isConnectorAuthRequest,
    isQuestionInteraction,
  } from "$lib/utils/work-interactions";
  import { workTaskStore } from "$lib/stores/work-task-store.svelte";

  interface Props {
    item: InboxItem;
    readOnly?: boolean;
    taskLabel?: string;
    workspaceLabel?: string;
    taskHref?: string | null;
    onResolve?: (item: InboxItem, status: InboxItemStatus, response?: unknown) => Promise<void>;
  }

  let {
    item,
    readOnly = false,
    taskLabel = "",
    workspaceLabel = "",
    taskHref = null,
    onResolve,
  }: Props = $props();

  let resolving = $state(false);
  let openingAuth = $state(false);
  let error = $state("");
  let showDetails = $state(false);
  let freeformAnswer = $state("");

  type QuestionOption = { label: string; value: string; description?: string };
  type QuestionSpec = {
    id: string;
    question: string;
    header?: string;
    options: QuestionOption[];
    multiSelect: boolean;
    allowOther: boolean;
    placeholder?: string;
    required: boolean;
  };
  let questionAnswers = $state<Record<string, string | string[]>>({});
  let questionCustomAnswers = $state<Record<string, string>>({});

  let questionList = $derived.by((): QuestionSpec[] => {
    const parameters = item.payload.parameters;
    if (!parameters || typeof parameters !== "object" || Array.isArray(parameters)) return [];
    const questions = (parameters as { questions?: unknown }).questions;
    if (!Array.isArray(questions)) return [];
    return questions.flatMap((raw, index) => {
      if (!raw || typeof raw !== "object" || Array.isArray(raw)) return [];
      const value = raw as Record<string, unknown>;
      const question = typeof value.question === "string" ? value.question.trim() : "";
      if (!question) return [];
      const options = Array.isArray(value.options)
        ? value.options.flatMap((option): QuestionOption[] => {
            if (typeof option === "string") return [{ label: option, value: option }];
            if (!option || typeof option !== "object" || Array.isArray(option)) return [];
            const optionValue = option as Record<string, unknown>;
            if (typeof optionValue.label !== "string" || !optionValue.label.trim()) return [];
            return [
              {
                label: optionValue.label,
                value:
                  typeof optionValue.value === "string" ? optionValue.value : optionValue.label,
                ...(typeof optionValue.description === "string"
                  ? { description: optionValue.description }
                  : {}),
              },
            ];
          })
        : [];
      return [
        {
          id: typeof value.id === "string" && value.id.trim() ? value.id : `q${index + 1}`,
          question,
          ...(typeof value.header === "string" ? { header: value.header } : {}),
          options,
          multiSelect: value.multi_select === true || value.multiSelect === true,
          allowOther: value.allow_other !== false && value.allowOther !== false,
          ...(typeof value.placeholder === "string" ? { placeholder: value.placeholder } : {}),
          required: value.required !== false,
        },
      ];
    });
  });

  $effect(() => {
    const itemId = item.id;
    if (!itemId) return;
    questionAnswers = {};
    questionCustomAnswers = {};
    freeformAnswer = "";
  });

  function questionValue(question: QuestionSpec): string | string[] {
    return questionAnswers[question.id] ?? (question.multiSelect ? [] : "");
  }

  function questionComplete(question: QuestionSpec): boolean {
    if (!question.required) return true;
    const value = questionValue(question);
    if (question.multiSelect) {
      return (
        (Array.isArray(value) && value.length > 0) ||
        (questionCustomAnswers[question.id] ?? "").trim().length > 0
      );
    }
    return (
      (questionCustomAnswers[question.id] ?? (Array.isArray(value) ? "" : value)).trim().length > 0
    );
  }

  let questionsComplete = $derived(
    questionList.length > 0 && questionList.every((question) => questionComplete(question)),
  );
  let freeformAnswerComplete = $derived(freeformAnswer.trim().length > 0);

  function setQuestionValue(question: QuestionSpec, value: string) {
    questionAnswers = { ...questionAnswers, [question.id]: value };
  }

  function setQuestionCustomValue(question: QuestionSpec, value: string) {
    questionCustomAnswers = { ...questionCustomAnswers, [question.id]: value };
  }

  function submittedQuestionAnswers(): Record<string, string | string[]> {
    return Object.fromEntries(
      questionList.map((question) => {
        const selected = questionValue(question);
        const custom = (questionCustomAnswers[question.id] ?? "").trim();
        if (question.multiSelect) {
          return [
            question.id,
            [...(Array.isArray(selected) ? selected : []), ...(custom ? [custom] : [])],
          ];
        }
        return [question.id, custom || (Array.isArray(selected) ? "" : selected)];
      }),
    );
  }

  function toggleQuestionOption(question: QuestionSpec, option: string) {
    if (!question.multiSelect) {
      setQuestionValue(question, option);
      return;
    }
    const selected = Array.isArray(questionValue(question)) ? [...questionValue(question)] : [];
    const index = selected.indexOf(option);
    if (index >= 0) selected.splice(index, 1);
    else selected.push(option);
    questionAnswers = { ...questionAnswers, [question.id]: selected };
  }

  async function submitQuestions(event: SubmitEvent) {
    event.preventDefault();
    if (!questionsComplete || resolving) return;
    await handleResolve("answered", {
      answers: submittedQuestionAnswers(),
      source: "ask_questions",
    });
  }

  async function submitFreeformAnswer(event: SubmitEvent) {
    event.preventDefault();
    if (!freeformAnswerComplete || resolving) return;
    await handleResolve("answered", {
      answer: freeformAnswer.trim(),
      source: "inbox",
    });
  }

  let isDir = $derived(isAccessRootRequest(item));
  let isApp = $derived(isAppConnectionRequest(item));
  let isConnector = $derived(isConnectorAuthRequest(item));
  let isQuestion = $derived(isQuestionInteraction(item));
  let isHostFallback = $derived(item.payload.executionLane === "host_fallback");
  let isDependencyInstall = $derived(Boolean(item.payload.packageManager || item.payload.packages));
  let dirPath = $derived(isDir ? getDirectoryPathFromItem(item) : "");
  let displayPath = $derived(isDir ? formatDirectoryPath(dirPath) : "");
  let title = $derived(getInteractionTitle(item));
  let description = $derived(getInteractionDescription(item));
  let hasStandingRule = $derived(
    !isHostFallback && Boolean(buildStandingRuleFromProposal(item)) && !isDir && !isApp,
  );
  let isRecovery = $derived(Boolean(item.payload.recoveryKey || item.payload.recoveryAction));
  let isRunFailure = $derived(item.payload.failureKind === "automation_failure");

  async function openAppAuth() {
    if (openingAuth) return;
    openingAuth = true;
    error = "";
    let url: string | null = item.payload.authUrl ?? null;
    try {
      if (!url) {
        const connectorId = (item.payload.connectorId || item.payload.appId || "").trim();
        if (!connectorId) throw new Error("连接请求缺少应用或连接器标识");
        if (isConnector) {
          if (item.payload.runtimeKind === "cli") {
            throw new Error("该连接器使用 CLI 认证，请在任务中执行它声明的 auth 操作");
          }
          const response = await startWorkConnectorAuth(connectorId, item.payload.accountId);
          url = response.authorizationUrl;
        } else {
          const response = await authorizeWorkApp(connectorId);
          url = response.authorizationUrl;
        }
      }
      if (!url) throw new Error("Provider 没有返回授权地址");
      await platform.shell.openExternal(url);
    } catch (cause) {
      if (url) {
        window.open(url, "_blank");
      } else {
        error = cause instanceof Error ? cause.message : String(cause);
      }
    } finally {
      openingAuth = false;
    }
  }

  async function handleResolve(status: InboxItemStatus, response?: unknown) {
    if (!onResolve || resolving) return;
    resolving = true;
    error = "";
    try {
      await onResolve(item, status, response);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      resolving = false;
    }
  }

  async function handleGrantStandingRule() {
    const rule = buildStandingRuleFromProposal(item);
    if (!onResolve || resolving || !rule) return;
    resolving = true;
    error = "";
    try {
      await workTaskStore.addStandingRule(item.taskId, rule);
      await onResolve(item, "approved", { standingRuleGranted: true });
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      resolving = false;
    }
  }
</script>

<div
  class="group relative rounded-xl border border-border/70 bg-card p-4 shadow-sm transition-all hover:border-border space-y-3"
>
  {#if taskLabel}
    <div class="flex items-center gap-2 text-[10px] text-muted-foreground">
      <span class="rounded bg-muted px-1.5 py-0.5 font-semibold text-foreground/80">来自任务</span>
      <span class="min-w-0 truncate font-medium text-foreground/85">{taskLabel}</span>
      {#if workspaceLabel}
        <span class="truncate text-muted-foreground/70">· {workspaceLabel}</span>
      {/if}
      {#if taskHref}
        <a
          href={taskHref}
          class="ml-auto shrink-0 rounded-md px-2 py-1 font-semibold text-primary hover:bg-primary/10"
          >回到任务 →</a
        >
      {/if}
    </div>
  {/if}
  <div class="flex items-start gap-3">
    <div
      class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-orange-500/10 text-orange-600 dark:text-orange-400"
    >
      {#if isDir}
        <svg
          viewBox="0 0 24 24"
          class="h-4 w-4"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />
        </svg>
      {:else if isApp}
        <svg
          viewBox="0 0 24 24"
          class="h-4 w-4"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
        </svg>
      {:else}
        <svg
          viewBox="0 0 24 24"
          class="h-4 w-4"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
      {/if}
    </div>
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
          {isRecovery
            ? "恢复确认"
            : isHostFallback
              ? "主机执行授权"
              : isDir
                ? "目录授权"
                : isConnector
                  ? "服务授权"
                  : isApp
                    ? "应用连接"
                    : isQuestion
                      ? "补充信息"
                      : isDependencyInstall
                        ? "依赖安装确认"
                        : "操作确认"}
        </span>
        <h4 class="text-xs font-semibold leading-snug text-foreground">{title}</h4>
      </div>
      {#if displayPath}
        <div
          class="mt-1 inline-block break-all rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground"
        >
          {displayPath}
        </div>
      {/if}
    </div>
  </div>

  {#if description}
    <p class="text-xs leading-relaxed text-muted-foreground">{description}</p>
  {/if}

  {#if isHostFallback}
    <div class="rounded-xl border border-amber-500/30 bg-amber-500/5 p-3 space-y-2 text-xs">
      <div class="flex items-center gap-1.5 font-semibold text-amber-800 dark:text-amber-300">
        <svg
          class="h-4 w-4 shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M12 9v4m0 4h.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" />
        </svg>
        <span>为什么需要主机执行</span>
      </div>
      <p class="text-muted-foreground leading-relaxed">
        {item.payload.fallbackReason || "Work 沙盒受系统环境限制阻止了该命令。"}
      </p>
      {#if item.payload.sandboxStderr}
        <div
          class="rounded bg-background/80 p-2 font-mono text-[10px] text-destructive break-all max-h-24 overflow-auto border border-border/50"
        >
          {item.payload.sandboxStderr}
        </div>
      {/if}
      <div class="text-[11px] text-muted-foreground">
        <span class="font-medium text-foreground/80">影响范围：</span>
        仅对本次调用授权一次性主机重试；使用相同命令、参数和工作目录执行，不扩大全局权限。
      </div>
    </div>
  {/if}

  {#if isDependencyInstall}
    <div class="rounded-xl border border-blue-500/30 bg-blue-500/5 p-3 space-y-2 text-xs">
      <div class="flex items-center gap-1.5 font-semibold text-blue-800 dark:text-blue-300">
        <svg
          class="h-4 w-4 shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="m21 16-4 4-4-4m4 4V4M3 8l4-4 4 4M7 4v16" />
        </svg>
        <span>依赖安装详情</span>
      </div>
      <div class="grid grid-cols-2 gap-2 text-[11px]">
        <div>
          <span class="text-muted-foreground">包管理器:</span>
          <span class="ml-1 font-mono font-medium text-foreground"
            >{item.payload.packageManager || "未知"}</span
          >
        </div>
        {#if item.payload.source}
          <div>
            <span class="text-muted-foreground">源地址:</span>
            <span class="ml-1 font-mono font-medium text-foreground">{item.payload.source}</span>
          </div>
        {/if}
      </div>
      {#if Array.isArray(item.payload.packages) && item.payload.packages.length > 0}
        <div>
          <span class="text-muted-foreground">目标包:</span>
          <div class="mt-1 flex flex-wrap gap-1">
            {#each item.payload.packages as pkg}
              <span
                class="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] font-medium text-foreground"
                >{pkg}</span
              >
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}

  {#if isQuestion && questionList.length > 0}
    <form
      class="space-y-3 rounded-xl border border-primary/20 bg-primary/[0.03] p-3"
      onsubmit={submitQuestions}
    >
      {#each questionList as question}
        <fieldset class="space-y-2">
          <legend class="text-xs font-medium leading-relaxed text-foreground">
            {#if question.header}<span class="mr-1 text-muted-foreground">{question.header}：</span
              >{/if}{question.question}
            {#if question.required}<span class="text-destructive">*</span>{/if}
          </legend>
          {#if question.options.length > 0}
            <div class="space-y-1.5">
              {#each question.options as option}
                {@const selected = Array.isArray(questionValue(question))
                  ? questionValue(question).includes(option.value)
                  : questionValue(question) === option.value}
                <button
                  type="button"
                  aria-pressed={selected}
                  class="flex w-full items-start justify-between gap-3 rounded-lg border px-3 py-2 text-left text-xs transition-colors {selected
                    ? 'border-primary/50 bg-primary/10 text-primary'
                    : 'border-border/70 hover:bg-accent'}"
                  onclick={() => toggleQuestionOption(question, option.value)}
                >
                  <span>{option.label}</span>
                  {#if option.description}<span class="text-[10px] text-muted-foreground"
                      >{option.description}</span
                    >{/if}
                </button>
              {/each}
            </div>
          {/if}
          {#if question.options.length === 0 || question.allowOther}
            <input
              value={questionCustomAnswers[question.id] ?? ""}
              placeholder={question.options.length > 0
                ? "也可以自行填写"
                : question.placeholder || "请输入答案"}
              class="w-full rounded-lg border border-border/70 bg-background px-3 py-2 text-xs outline-none focus:border-primary/50 focus:ring-2 focus:ring-primary/10"
              oninput={(event) =>
                setQuestionCustomValue(question, (event.currentTarget as HTMLInputElement).value)}
            />
          {/if}
        </fieldset>
      {/each}
      <div class="flex items-center justify-end border-t border-border/40 pt-2">
        <button
          type="submit"
          disabled={!questionsComplete || resolving || readOnly}
          class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-semibold text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 disabled:opacity-50"
        >
          {resolving ? "提交中…" : "提交答案并继续"}
        </button>
      </div>
    </form>
  {/if}

  {#if isQuestion && questionList.length === 0}
    <form
      class="space-y-2 rounded-xl border border-primary/20 bg-primary/[0.03] p-3"
      onsubmit={submitFreeformAnswer}
    >
      <label class="sr-only" for={`inbox-answer-${item.id}`}>回答</label>
      <textarea
        id={`inbox-answer-${item.id}`}
        bind:value={freeformAnswer}
        rows="3"
        placeholder="请输入回答"
        class="w-full resize-y rounded-lg border border-border/70 bg-background px-3 py-2 text-xs outline-none focus:border-primary/50 focus:ring-2 focus:ring-primary/10"
      ></textarea>
      <div class="flex justify-end">
        <button
          type="submit"
          disabled={!freeformAnswerComplete || resolving || readOnly}
          class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-semibold text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 disabled:opacity-50"
        >
          {resolving ? "提交中…" : "提交回答并继续"}
        </button>
      </div>
    </form>
  {/if}

  <!-- Cause & Action Guidance Note -->
  <div
    class="flex items-center gap-1.5 text-[10px] text-muted-foreground/80 bg-muted/40 rounded px-2 py-1"
  >
    <svg
      class="h-3 w-3 shrink-0 text-primary"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
    >
      <circle cx="12" cy="12" r="10" />
      <line x1="12" y1="16" x2="12" y2="12" />
      <line x1="12" y1="8" x2="12.01" y2="8" />
    </svg>
    <span>
      {#if isRecovery}
        {isRunFailure
          ? "任务运行失败，请选择重试或取消任务"
          : "任务遇到执行中断，请确认操作是否生效或选择重试"}
      {:else if isDir}
        任务需要读取或写入外部工作目录，授权后自动继续执行
      {:else if isConnector || isApp}
        任务需要连接外部工具服务，完成授权后自动恢复执行
      {:else if isQuestion}
        {questionList.length > 0
          ? "任务需要你的补充决策信息，填写并提交后自动继续"
          : "任务需要你的补充决策信息，请在这里填写回答后提交"}
      {:else if item.itemType === "plan_approval"}
        任务方案已规划完毕，确认后即刻开始执行
      {:else if item.itemType === "artifact_validation"}
        交付物已生成，确认文件格式与内容是否符合预期
      {:else}
        任务正在等待此操作授权，批准后将自动恢复当前任务
      {/if}
    </span>
  </div>

  {#if isRecovery && item.payload.sideEffectClass === "external_mutating"}
    <p
      class="rounded-lg border border-red-500/25 bg-red-500/[0.06] px-2.5 py-2 text-[11px] leading-4 text-red-700 dark:text-red-300"
    >
      该操作可能已经产生外部副作用。请选择它是否已经完成；系统不会盲目重放旧调用。
    </p>
  {/if}

  {#if error}
    <div
      class="flex items-center justify-between gap-2 rounded-md border border-red-500/20 bg-red-500/5 px-2.5 py-1.5 text-[11px] leading-4 text-red-600 dark:text-red-300"
      role="alert"
    >
      <span class="min-w-0 flex-1">{error}</span>
      <button
        type="button"
        class="shrink-0 text-red-500/70 hover:text-red-600 dark:hover:text-red-200"
        onclick={() => (error = "")}
        aria-label="关闭提示"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  {/if}

  {#if item.payload.toolName || item.payload.parameters}
    <div>
      <button
        type="button"
        class="text-[10px] text-muted-foreground/80 hover:text-foreground inline-flex items-center gap-1"
        onclick={() => (showDetails = !showDetails)}
      >
        <span>{showDetails ? "收起详情" : "查看调用参数"}</span>
        <svg
          class="h-3 w-3 transition-transform duration-150 {showDetails ? 'rotate-180' : ''}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {#if showDetails}
        <div class="mt-1.5 space-y-1 rounded-lg bg-muted/40 p-2 text-[10px] text-muted-foreground">
          {#if item.payload.toolName}
            <div>
              工具: <code class="font-mono text-foreground font-semibold"
                >{item.payload.toolName}</code
              >
            </div>
          {/if}
          {#if item.payload.parameters}
            <pre
              class="max-h-28 overflow-auto font-mono text-[9px] bg-background/50 rounded p-1.5 mt-1 border border-border/40">{JSON.stringify(
                item.payload.parameters,
                null,
                2,
              )}</pre>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <div class="flex items-center justify-end gap-2 border-t border-border/40 pt-2.5">
    {#if isRecovery}
      <button
        type="button"
        disabled={resolving || readOnly}
        class="rounded-lg border border-border/70 px-3 py-1.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent disabled:opacity-50"
        onclick={() => handleResolve("cancelled", { recoveryAction: "cancel" })}
      >
        取消任务
      </button>
      <button
        type="button"
        disabled={resolving || readOnly}
        class="rounded-lg border border-orange-500/35 bg-orange-500/10 px-3 py-1.5 text-xs font-semibold text-orange-700 transition-colors hover:bg-orange-500/20 dark:text-orange-300 disabled:opacity-50"
        onclick={() => handleResolve("rejected", { recoveryAction: "retry" })}
      >
        未完成，重试
      </button>
      {#if !isRunFailure}
        <button
          type="button"
          disabled={resolving || readOnly}
          class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-semibold text-primary-foreground shadow-sm transition-all hover:bg-primary/90 disabled:opacity-50"
          onclick={() => handleResolve("approved", { recoveryAction: "continue" })}
        >
          {resolving ? "处理中…" : "已完成，继续"}
        </button>
      {/if}
    {:else}
      <button
        type="button"
        disabled={resolving || readOnly}
        class="rounded-lg border border-border/70 px-3 py-1.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
        onclick={() => handleResolve("rejected")}
      >
        {isQuestion ? "暂不处理" : "拒绝"}
      </button>
      {#if hasStandingRule}
        <button
          type="button"
          disabled={resolving || readOnly}
          title={`本任务内总是允许 ${item.payload.standingRuleProposal?.toolName}（${item.payload.standingRuleProposal?.targetPattern}）`}
          class="rounded-lg border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs font-medium text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
          onclick={handleGrantStandingRule}
        >
          总是允许
        </button>
      {/if}
      {#if isQuestion}{:else if isConnector || isApp}
        {#if !isConnector || item.payload.runtimeKind !== "cli"}
          <button
            type="button"
            disabled={readOnly || openingAuth}
            class="rounded-lg border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs font-medium text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
            onclick={openAppAuth}
          >
            {openingAuth ? "准备授权…" : "打开授权页面 ↗"}
          </button>
        {:else}
          <span
            class="rounded-lg border border-border/70 px-3 py-1.5 text-[11px] text-muted-foreground"
          >
            请在任务中完成服务授权
          </span>
        {/if}
        <button
          type="button"
          disabled={resolving || readOnly}
          class="flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-semibold text-primary-foreground shadow-sm transition-all hover:bg-primary/90 disabled:opacity-50"
          onclick={() => handleResolve("approved")}
        >
          {#if resolving}
            <span
              class="h-3 w-3 animate-spin rounded-full border-2 border-primary-foreground/20 border-t-primary-foreground"
            ></span>
            <span>恢复中…</span>
          {:else}
            <span>{isConnector ? "认证完成并继续" : "已完成授权并继续"}</span>
          {/if}
        </button>
      {:else}
        <button
          type="button"
          disabled={resolving || readOnly}
          class="flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-semibold text-primary-foreground shadow-sm transition-all hover:bg-primary/90 disabled:opacity-50"
          onclick={() => handleResolve("approved")}
        >
          {#if resolving}
            <span
              class="h-3 w-3 animate-spin rounded-full border-2 border-primary-foreground/20 border-t-primary-foreground"
            ></span>
            <span>处理中…</span>
          {:else if isDir}
            <span>允许访问并继续</span>
          {:else if isHostFallback}
            <span>仅本次在主机运行</span>
          {:else}
            <span>批准执行</span>
          {/if}
        </button>
      {/if}
    {/if}
  </div>
</div>
