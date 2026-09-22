<script lang="ts">
  /**
   * NumberInput — a small numeric input with min/max clamping and unit suffix.
   */

  interface Props {
    value: number;
    min: number;
    max: number;
    step?: number;
    unit?: string;
    disabled?: boolean;
    id?: string;
    ariaLabel?: string;
    onChange: (value: number) => void;
  }

  let {
    value,
    min,
    max,
    step = 1,
    unit = "",
    disabled = false,
    id,
    ariaLabel,
    onChange,
  }: Props = $props();

  let draft = $state(String(value));

  $effect(() => {
    draft = String(value);
  });

  function clamp(v: number): number {
    return Math.min(max, Math.max(min, v));
  }

  function commit(raw: string) {
    const n = parseFloat(raw);
    if (!Number.isFinite(n)) {
      draft = String(value); // revert
      return;
    }
    const clamped = clamp(n);
    draft = String(clamped);
    if (clamped !== value) onChange(clamped);
  }
</script>

<div class="number-input-wrap">
  <input
    type="number"
    class="number-input ui-input"
    {id}
    {min}
    {max}
    {step}
    {disabled}
    aria-label={ariaLabel}
    value={draft}
    oninput={(e) => {
      draft = (e.target as HTMLInputElement).value;
    }}
    onchange={(e) => commit((e.target as HTMLInputElement).value)}
    onblur={(e) => commit((e.target as HTMLInputElement).value)}
  />
  {#if unit}
    <span class="number-unit">{unit}</span>
  {/if}
</div>

<style>
  .number-input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .number-input {
    width: 64px;
    height: 32px;
    padding: 0 8px;
    text-align: right;
    font-size: 0.8125rem;
    /* Hide native spinner arrows */
    -moz-appearance: textfield;
  }

  .number-input::-webkit-outer-spin-button,
  .number-input::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }

  .number-unit {
    font-size: 0.8125rem;
    color: var(--text-tertiary);
    user-select: none;
  }
</style>
