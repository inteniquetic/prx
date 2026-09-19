<script lang="ts">
  export let title: string = '';
  export let subtitle: string = '';
</script>

<div class="flex flex-1 flex-col overflow-hidden">
  <!-- Top Header Bar -->
  <header class="flex shrink-0 items-center justify-between border-b border-border bg-background/80 px-6 py-4 backdrop-blur-sm">
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold text-foreground">{title}</h1>
      {#if subtitle}
        <p class="mt-0.5 truncate text-sm text-muted-foreground">{subtitle}</p>
      {/if}
    </div>
    <div class="ml-4 flex shrink-0 items-center gap-3">
      <slot name="header-actions" />
    </div>
  </header>

  <!-- Scrollable content. A plain div: the shell owns the page's one <main>,
       and nesting a second inside it leaves neither of them nameable. -->
  <div class="flex-1 overflow-y-auto overflow-x-hidden">
    <slot />
  </div>
</div>