<script lang="ts">
  import {
    currentPage,
    sidebarCollapsed,
    toggleSidebar,
    navigate,
    navItems,
  } from '../../stores/navigation';

  $: isCollapsed = $sidebarCollapsed;

  const handleNavClick = (page: typeof navItems[number]['id']): void => {
    navigate(page);
  };

  const navItemClass = (itemId: string, active: boolean, collapsed: boolean): string => {
    const base = 'group relative flex items-center w-full rounded-md transition-all duration-150 border-l-2 pl-2 py-2.5';
    const border = active ? 'border-cyan-400' : 'border-transparent';
    const bg = active ? 'bg-slate-800/80' : 'bg-transparent';
    const pr = collapsed ? 'pr-2' : 'pr-3';
    return `${base} ${border} ${bg} ${pr}`;
  };

  const iconClass = (active: boolean): string => {
    const base = 'flex items-center justify-center w-8 h-8 rounded-md shrink-0 transition-colors duration-150';
    const color = active ? 'text-cyan-300' : 'text-slate-400';
    return `${base} ${color}`;
  };

  const labelClass = (active: boolean): string => {
    const base = 'ml-3 text-sm font-medium truncate transition-colors duration-150';
    const color = active ? 'text-cyan-300' : 'text-slate-400';
    return `${base} ${color}`;
  };
</script>

<aside
  class="relative flex flex-col h-screen bg-slate-950 border-r border-slate-800 select-none shrink-0 transition-all duration-300 ease-in-out overflow-hidden"
  style:width={isCollapsed ? '64px' : '240px'}
>
  <!-- Logo / Brand -->
  <div class="flex items-center h-16 px-4 border-b border-slate-800/60">
    <div class="flex items-center gap-3 min-w-0">
      <div
        class="flex items-center justify-center w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-500 to-cyan-700 text-white font-bold text-sm shrink-0"
      >
        P
      </div>
      {#if !isCollapsed}
        <div class="flex flex-col min-w-0 transition-opacity duration-200">
          <span class="text-lg font-bold text-slate-100 leading-tight tracking-tight">PRX</span>
          <span class="text-[10px] font-medium text-slate-500 leading-tight uppercase tracking-widest">Proxy</span>
        </div>
      {/if}
    </div>
  </div>

  <!-- Navigation Links -->
  <nav class="flex-1 py-4 px-2 space-y-1 overflow-y-auto overflow-x-hidden">
    {#each navItems as item (item.id)}
      {@const isActive = $currentPage === item.id}
      <button
        type="button"
        class={navItemClass(item.id, isActive, isCollapsed)}
        on:click={() => handleNavClick(item.id)}
        title={isCollapsed ? item.label : ''}
      >
        <span class={iconClass(isActive)}>
          <span class="text-base">{item.icon}</span>
        </span>
        {#if !isCollapsed}
          <span class={labelClass(isActive)}>
            {item.label}
          </span>
        {/if}

        <!-- Tooltip when collapsed -->
        {#if isCollapsed}
          <div
            class="absolute left-full ml-3 px-2.5 py-1.5 rounded-md bg-slate-800 text-sm text-slate-200 whitespace-nowrap shadow-lg border border-slate-700 opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all duration-150 pointer-events-none z-50"
          >
            {item.label}
            <div class="absolute top-1/2 -translate-y-1/2 -left-1 w-2 h-2 rotate-45 bg-slate-800 border-l border-b border-slate-700"></div>
          </div>
        {/if}
      </button>
    {/each}
  </nav>

  <!-- Bottom Section: Status + Collapse Toggle -->
  <div class="border-t border-slate-800/60 px-2 py-3 space-y-3">
    <!-- Proxy Status -->
    <div
      class="flex items-center gap-2.5 px-2 py-1.5 rounded-md"
      title="Proxy status: Connected"
    >
      <span class="relative flex items-center justify-center w-2.5 h-2.5 shrink-0">
        <span class="absolute inline-flex w-full h-full rounded-full bg-emerald-400/40 animate-ping"></span>
        <span class="relative inline-flex w-2 h-2 rounded-full bg-emerald-400"></span>
      </span>
      {#if !isCollapsed}
        <span class="text-xs font-medium text-slate-400 truncate">Connected</span>
      {/if}
    </div>

    <!-- Collapse Toggle -->
    <button
      type="button"
      class="flex items-center justify-center w-full py-2 rounded-md text-slate-500 hover:text-slate-300 hover:bg-slate-900/50 transition-colors duration-150"
      on:click={toggleSidebar}
      title={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
    >
      <span
        class="inline-block transition-transform duration-300"
        class:rotate-180={isCollapsed}
      >
        <svg
          class="w-5 h-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 19.5L8.25 12l7.5-7.5" />
        </svg>
      </span>
    </button>
  </div>
</aside>