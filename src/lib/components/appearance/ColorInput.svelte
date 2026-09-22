<script lang="ts">
  /**
   * ColorInput — color swatch + HEX text input combo.
   * Emits valid HEX strings only. Invalid input is shown in the text box
   * but does not fire onChange, preventing theme corruption.
   */

  interface Props {
    value: string; // HEX "#rrggbb" or ""
    placeholder?: string;
    disabled?: boolean;
    id?: string;
    onChange: (hex: string) => void;
  }

  let { value, placeholder = "#000000", disabled = false, id, onChange }: Props = $props();

  // Draft text shown in the input (may be temporarily invalid)
  let draft = $state(value);

  // Keep draft in sync if value changes from outside
  $effect(() => {
    draft = value;
  });

  function isValidHex(s: string): boolean {
    return /^#[0-9a-fA-F]{6}$/.test(s);
  }

  function handleTextInput(e: Event) {
    const raw = (e.target as HTMLInputElement).value;
    draft = raw;
    if (raw.trim() === "") {
      onChange("");
      return;
    }
    // Auto-prefix "#" if user typed 6 hex chars without it
    const normalized = raw.startsWith("#") ? raw : `#${raw}`;
    if (isValidHex(normalized)) {
      onChange(normalized);
      return;
    }
    // If valid as-is
    if (isValidHex(raw)) {
      onChange(raw);
    }
  }

  function handleColorPicker(e: Event) {
    const hex = (e.target as HTMLInputElement).value;
    if (isValidHex(hex)) {
      draft = hex;
      onChange(hex);
    }
  }

  // Displayed swatch color (only valid HEX)
  let swatchColor = $derived(isValidHex(value) ? value : "transparent");
  let showBorder = $derived(!isValidHex(value));
</script>

<div class="color-input-wrap">
  <!-- Native color picker (hidden, triggered by swatch click) -->
  <label class="color-swatch-label" aria-label="Open color picker" {id}>
    <input
      type="color"
      class="color-picker-native"
      value={isValidHex(value) ? value : "#000000"}
      {disabled}
      oninput={handleColorPicker}
      aria-hidden="true"
      tabindex="-1"
    />
    <span
      class="color-swatch"
      class:swatch-border={showBorder}
      style="background: {swatchColor};"
      role="presentation"
    ></span>
  </label>

  <!-- HEX text input -->
  <input
    type="text"
    class="color-hex-input ui-input"
    value={draft}
    {disabled}
    maxlength={7}
    {placeholder}
    aria-label="HEX color value"
    oninput={handleTextInput}
    onblur={() => {
      // On blur: if empty, preserve empty and notify
      if (draft.trim() === "") {
        draft = "";
        onChange("");
      } else if (!isValidHex(draft) && !isValidHex(draft.startsWith("#") ? draft : `#${draft}`)) {
        draft = value;
      }
    }}
  />
</div>

<style>
  .color-input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .color-swatch-label {
    position: relative;
    cursor: pointer;
    flex-shrink: 0;
  }

  .color-picker-native {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
    border: none;
    padding: 0;
  }

  .color-swatch {
    display: block;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid var(--border-default);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
    transition: border-color 150ms;
  }

  .color-swatch-label:hover .color-swatch {
    border-color: var(--border-strong);
  }

  .swatch-border {
    border: 2px dashed var(--border-strong);
  }

  .color-hex-input {
    width: 106px;
    padding: 0 8px;
    height: 32px;
    font-size: 0.8125rem;
    font-family: var(--font-code, ui-monospace, monospace);
    letter-spacing: 0.01em;
  }
</style>
