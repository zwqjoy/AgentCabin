/**
 * Formatters and metrics utilities for conversation turns, tokens, throughput, and duration.
 * Adapted and optimized for AgentCabin. Attribution: original percentage algorithms adapted
 * from DeepSeek Harness (@deepseek-ai/dsh-client-ui-chat:formatCacheHitPercent).
 */

/**
 * Compact token count: 517 / 12.2K / 517K / 1.2M.
 */
export function formatTokens(value: number): string {
  if (value < 0 || !Number.isFinite(value)) return "0";
  const scaled = (candidate: number) =>
    candidate >= 100 ? String(Math.round(candidate)) : String(Math.round(candidate * 10) / 10);
  if (value < 1e3) return String(value);
  if (value < 1e6) return `${scaled(value / 1e3)}K`;
  return `${scaled(value / 1e6)}M`;
}

/**
 * Exact integer token count with digit grouping (e.g. 261,281).
 */
export function formatExactTokens(value: number): string {
  if (value < 0 || !Number.isFinite(value)) return "0";
  return Math.round(value).toLocaleString();
}

/** Round a cache-read ratio to exact percentage units, with positive ties rounded up. */
function roundedPercentUnits(
  cacheReadTokens: number,
  denominator: number,
  decimalPlaces: number,
): number {
  const scale = (decimalPlaces === 0 ? 1 : 10) * 100;
  const doubledScale = scale * 2;
  const denominatorQuotient = Math.floor(denominator / doubledScale);
  const denominatorRemainder = denominator % doubledScale;
  let lower = 0;
  let upper = scale;
  while (lower < upper) {
    const candidate = Math.floor((lower + upper + 1) / 2);
    const factor = candidate * 2 - 1;
    if (
      cacheReadTokens >=
      factor * denominatorQuotient + Math.ceil((factor * denominatorRemainder) / doubledScale)
    ) {
      lower = candidate;
    } else {
      upper = candidate - 1;
    }
  }
  return lower;
}

function displayPercentUnits(units: number, decimalPlaces: number): string {
  if (decimalPlaces === 0) return String(units);
  const whole = Math.floor(units / 10);
  const tenths = units % 10;
  return tenths === 0 ? String(whole) : `${whole}.${tenths}`;
}

/**
 * Display-ready cache-hit share without rounding a partial hit to 100%.
 */
export function formatCacheHitPercent(
  cacheReadTokens: number,
  promptTokens: number,
  decimalPlaces = 1,
): string | null {
  if (!promptTokens || promptTokens <= 0) return null;
  const missedInputTokens = promptTokens - cacheReadTokens;
  if (missedInputTokens <= 0) return "100";
  const roundedUnits = roundedPercentUnits(cacheReadTokens, promptTokens, decimalPlaces);
  if (roundedUnits < (decimalPlaces === 0 ? 100 : 1e3)) {
    return displayPercentUnits(roundedUnits, decimalPlaces);
  }
  let distinguishingPlaces = 1;
  let scaledDoubleGap = missedInputTokens * 200;
  const denominatorTens = Math.floor(promptTokens / 10);
  while (scaledDoubleGap <= denominatorTens) {
    scaledDoubleGap *= 10;
    distinguishingPlaces += 1;
  }
  const denominatorOnes = promptTokens % 10;
  let roundedLoss = 5;
  for (let loss = 1; loss < 5; loss += 1) {
    const factor = loss * 2 + 1;
    const threshold = factor * denominatorTens + Math.floor((factor * denominatorOnes) / 10);
    if (scaledDoubleGap <= threshold) {
      roundedLoss = loss;
      break;
    }
  }
  return `99.${"9".repeat(distinguishingPlaces - 1)}${10 - roundedLoss}`;
}

/**
 * Format duration in human readable form (e.g. 5.8秒, 1分46秒, 10分26秒).
 */
export function formatRunDuration(ms: number, isEn = false): string {
  if (!ms || ms <= 0) return isEn ? "0s" : "0秒";
  const totalSecs = Math.max(1, Math.round(ms / 1000));
  if (totalSecs < 60) {
    return isEn ? `${totalSecs}s` : `${totalSecs}秒`;
  }
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  if (secs === 0) {
    return isEn ? `${mins}m` : `${mins}分`;
  }
  return isEn ? `${mins}m${secs}s` : `${mins}分${secs}秒`;
}

/**
 * Format seconds with one decimal place for TTFT (e.g. 5.8秒).
 */
export function formatLatencySeconds(ms: number, isEn = false): string {
  const secs = (ms / 1000).toFixed(1);
  return isEn ? `${secs}s` : `${secs}秒`;
}

/**
 * Format throughput tokens per second (e.g. 45 tok/s).
 */
export function formatTokensPerSecond(tps: number): string {
  if (!tps || tps <= 0) return "0";
  return String(Math.round(tps));
}
