<script lang="ts">
  export let title: string = '';
  export let subtitle: string = '';
</script>

<div class="flex flex-1 flex-col overflow-hidden">
  <!-- Top Header Bar -->
  <header class="flex shrink-0 items-center justify-between border-b border-slate-800 bg-slate-950/80 px-6 py-4 backdrop-blur-sm">
    <div class="min-w-0">
      <h1 class="truncate text-xl font-semibold text-slate-100">{title}</h1>
      {#if subtitle}
        <p class="mt-0.5 truncate text-sm text-slate-400">{subtitle}</p>
      {/if}
    </div>
    <div class="ml-4 flex shrink-0 items-center gap-3">
      <slot name="header-actions" />
    </div>
  </header>

  <!-- Scrollable Content -->
  <main class="flex-1 overflow-y-auto overflow-x-hidden">
    <slot />
  </main>
</div>