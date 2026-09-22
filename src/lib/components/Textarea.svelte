<script lang="ts">
  let {
    value = $bindable(""),
    placeholder = "",
    rows = 3,
    disabled = false,
    class: className = "",
    onkeydown,
  }: {
    value?: string;
    placeholder?: string;
    rows?: number;
    disabled?: boolean;
    class?: string;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();
</script>

<textarea
  {placeholder}
  {rows}
  {disabled}
  bind:value
  {onkeydown}
  class="ui-input flex min-h-[60px] w-full bg-transparent px-3 py-2 shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 resize-none {className}"
></textarea>
