<script lang="ts">
  /**
   * SegmentedControl — a group of mutually exclusive buttons (pill style).
   * Fully keyboard-accessible via arrow keys + Enter/Space.
   */

  interface Option<T extends string> {
    value: T;
    label: string;
  }

  interface Props<T extends string> {
    options: Option<T>[];
    value: T;
    disabled?: boolean;
    id?: string;
    ariaLabel?: string;
    onChange: (value: T) => void;
  }

  let { options, value, disabled = false, id, ariaLabel, onChange }: Props<string> = $props();

  function handleKey(e: KeyboardEvent, idx: number) {
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      const next = (idx + 1) % options.length;
      onChange(options[next].value);
      // Move focus to newly selected button
      const el = (e.currentTarget as HTMLElement)
        .closest(".segmented-control")
        ?.querySelectorAll<HTMLButtonElement>("button")[next];
      el?.focus();
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      const prev = (idx - 1 + options.length) % options.length;
      onChange(options[prev].value);
      const el = (e.currentTarget as HTMLElement)
        .closest(".segmented-control")
        ?.querySelectorAll<HTMLButtonElement>("button")[prev];
      el?.focus();
    }
  }
</script>

<div class="segmented-control" role="group" aria-label={ariaLabel} {id}>
  {#each options as option, idx}
    <button
      type="button"
      class="seg-btn"
      class:seg-active={value === option.value}
      class:opacity-50={disabled}
      role="radio"
      aria-checked={value === option.value}
      {disabled}
      tabindex={value === option.value ? 0 : -1}
      onclick={() => onChange(option.value)}
      onkeydown={(e) => handleKey(e, idx)}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .segmented-control {
    display: inline-flex;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-default);
    background: var(--bg-subtle);
    padding: 2px;
    gap: 2px;
  }

  .seg-btn {
    padding: 0 12px;
    height: 28px;
    font-size: 0.8125rem;
    font-weight: 500;
    border-radius: calc(var(--radius-control) - 2px);
    color: var(--text-secondary);
    background: transparent;
    border: none;
    cursor: pointer;
    transition:
      background-color var(--duration-normal) var(--ease-standard),
      color var(--duration-normal) var(--ease-standard),
      box-shadow var(--duration-fast) var(--ease-standard);
    white-space: nowrap;
  }

  .seg-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-hover);
  }

  .seg-active {
    background: var(--bg-surface) !important;
    color: var(--text-primary) !important;
    box-shadow:
      0 1px 3px rgba(17, 24, 39, 0.12),
      0 0 0 0.5px var(--border-default);
  }

  .seg-btn:focus-visible {
    outline: none;
    box-shadow: var(--shadow-control-focus);
  }

  /* pointer-cursor support */
  :global(.use-pointer-cursor) .seg-btn {
    cursor: pointer;
  }
</style>
