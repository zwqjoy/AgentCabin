<script lang="ts">
  let {
    id = "",
    value = $bindable(""),
    placeholder = "",
    type = "text",
    disabled = false,
    class: className = "",
    oninput,
    onblur,
    onpaste,
  }: {
    id?: string;
    value?: string;
    placeholder?: string;
    type?: string;
    disabled?: boolean;
    class?: string;
    oninput?: (e: Event) => void;
    onblur?: (e: FocusEvent) => void;
    onpaste?: (e: ClipboardEvent) => void;
  } = $props();
</script>

<input
  {id}
  {type}
  {placeholder}
  {disabled}
  bind:value
  {oninput}
  {onblur}
  {onpaste}
  class="ui-input flex w-full bg-transparent px-3 py-1 shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 {className}"
/>
