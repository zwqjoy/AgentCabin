export type WorkExecutionFailureCategory =
  | "sandbox_denied"
  | "sandbox_setup_failed"
  | "process_start_failed"
  | "non_zero_exit"
  | "output_missing"
  | "timed_out"
  | "cancelled"
  | "user_rejected"
  | "execution_failed";

export interface WorkExecutionFailure {
  category: WorkExecutionFailureCategory;
  label: string;
  description: string;
  exitCode?: number;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" ? (value as Record<string, unknown>) : null;
}

function firstString(...values: unknown[]): string {
  return (
    values
      .find((value): value is string => typeof value === "string" && value.trim() !== "")
      ?.trim() || ""
  );
}

function firstNumber(...values: unknown[]): number | undefined {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim() !== "" && Number.isFinite(Number(value))) {
      return Number(value);
    }
  }
  return undefined;
}

function outputText(record: Record<string, unknown>, details: Record<string, unknown>): string {
  const content = record.content;
  const contentText = Array.isArray(content)
    ? content
        .map((item) => asRecord(item)?.text)
        .filter((value): value is string => typeof value === "string")
        .join("\n")
    : "";
  return [
    firstString(
      record.error,
      details.error,
      record.stderr,
      details.stderr,
      record.stdout,
      details.stdout,
    ),
    contentText,
  ]
    .filter(Boolean)
    .join("\n")
    .toLowerCase();
}

/**
 * Classify the authoritative Work command result without guessing from the
 * tool's generic success boolean. Older timeline entries may only contain
 * stderr/content, so the textual fallbacks intentionally remain here.
 */
export function classifyWorkExecutionFailure(
  status: string,
  output: unknown,
): WorkExecutionFailure | null {
  const record = asRecord(output) ?? {};
  const details = asRecord(record.details) ?? record;
  const failureKind = firstString(
    record.failureKind,
    record.failure_kind,
    details.failureKind,
    details.failure_kind,
  ).toLowerCase();
  const resultStatus = firstString(record.status, details.status, status).toLowerCase();
  const exitCode = firstNumber(
    record.exitCode,
    record.exit_code,
    details.exitCode,
    details.exit_code,
  );
  const text = outputText(record, details);

  if (resultStatus === "success" && record.ok !== false && details.ok !== false) return null;

  if (
    failureKind === "sandbox_denied" ||
    (resultStatus === "denied" && /sandbox|seatbelt|operation not permitted|deny /.test(text)) ||
    /sandbox(?:[- ]exec|[- ]apply)?.*(?:denied|reject|blocked)|deny (?:file|process)-|operation not permitted/.test(
      text,
    )
  ) {
    return {
      category: "sandbox_denied",
      label: "沙箱拒绝",
      description: "Work 沙箱阻止了这次命令访问或执行。请检查授权目录、沙箱策略和命令所需的资源。",
      exitCode,
    };
  }

  if (
    /failed to spawn|spawn(?:ed)? .*?(?:failed|error)|failed to create process|process creation|enoent.*(?:spawn|executable)|failed to start/.test(
      text,
    )
  ) {
    return {
      category: "process_start_failed",
      label: "进程启动失败",
      description: "命令进程没有成功启动。请检查可执行文件、PATH、工作目录和进程启动权限。",
      exitCode,
    };
  }

  if (
    failureKind === "sandbox_infrastructure_failure" ||
    /(?:work|native) sandbox (?:layout|confinement)|sandbox provider|sandbox policy|fail-closed/.test(
      text,
    )
  ) {
    return {
      category: "sandbox_setup_failed",
      label: "沙箱初始化失败",
      description: "Work 没有成功建立命令沙箱，命令尚未可靠执行。请检查运行环境后重试。",
      exitCode,
    };
  }

  if (
    failureKind === "output_missing" ||
    /obligation failed|expected output(?: file)? .*?(?:not created|missing|could not be resolved)/.test(
      text,
    )
  ) {
    return {
      category: "output_missing",
      label: "输出缺失",
      description: "命令返回后没有找到声明的 expected_outputs 文件或成果。",
      exitCode,
    };
  }

  if (
    resultStatus === "timeout" ||
    resultStatus === "timed_out" ||
    /timed out|timeout/.test(text)
  ) {
    return {
      category: "timed_out",
      label: "执行超时",
      description: "命令超过允许时间，Work 已终止它及其子进程。",
      exitCode,
    };
  }

  if (
    resultStatus === "cancelled" ||
    resultStatus === "canceled" ||
    /cancelled|canceled/.test(text)
  ) {
    return {
      category: "cancelled",
      label: "执行已取消",
      description: "这次命令执行被取消，没有完成。",
      exitCode,
    };
  }

  if (resultStatus === "rejected" || resultStatus === "user_rejected") {
    return {
      category: "user_rejected",
      label: "用户拒绝",
      description: "这次命令没有获得用户确认。",
      exitCode,
    };
  }

  if (exitCode !== undefined && exitCode !== 0) {
    return {
      category: "non_zero_exit",
      label: "非零退出",
      description: `命令进程已启动，但以退出码 ${exitCode} 结束。请查看 stderr 了解命令自身的报错。`,
      exitCode,
    };
  }

  return {
    category: "execution_failed",
    label: "执行失败",
    description: "命令执行未成功，但运行时没有提供更具体的失败类型。",
    exitCode,
  };
}
