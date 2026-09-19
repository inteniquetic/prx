<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RouteConfig, ServiceConfig } from '../../types/config';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  export let route: RouteConfig;
  export let routeIndex: number;
  export let mode: 'view' | 'edit' = 'edit';
  export let services: ServiceConfig[] = [];
  export let saving: boolean = false;
  export let errorMessage: string = '';

  const dispatch = createEventDispatcher<{
    close: void;
    deleteRoute: number;
    save: RouteConfig;
  }>();

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let showDeleteConfirm = false;
  let newMethod = '';
  let editRoute: RouteConfig = { ...route };

  $: isViewMode = mode === 'view';

  // Sync local state when route prop changes (e.g., after refresh)
  $: if (route && !saving) {
    editRoute = { ...route };
  }

  // ---------------------------------------------------------------------------
  // Input helpers
  // ---------------------------------------------------------------------------

  const inputValue = (event: Event): string =>
    (event.currentTarget as HTMLInputElement).value;

  const checkedValue = (event: Event): boolean =>
    (event.currentTarget as HTMLInputElement).checked;

  const selectValue = (event: Event): string =>
    (event.currentTarget as HTMLSelectElement).value;

  // ---------------------------------------------------------------------------
  // Update functions (modify local state only)
  // ---------------------------------------------------------------------------

  const updateField = <K extends keyof RouteConfig>(key: K, value: RouteConfig[K]) => {
    editRoute = { ...editRoute, [key]: value };
  };

  // ---------------------------------------------------------------------------
  // Methods management
  // ---------------------------------------------------------------------------

  const VALID_METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS'];

  const isValidMethod = (method: string): boolean => {
    const upper = method.toUpperCase().trim();
    return VALID_METHODS.includes(upper);
  };

  const addMethod = (method: string) => {
    const upper = method.toUpperCase().trim();
    if (!upper) return;
    if (!isValidMethod(upper)) return;
    // Don't add duplicates
    if (editRoute.methods.includes(upper)) return;
    editRoute = { ...editRoute, methods: [...editRoute.methods, upper] };
  };

  const removeMethod = (idx: number) => {
    editRoute = { ...editRoute, methods: editRoute.methods.filter((_, i) => i !== idx) };
  };

  const handleMethodKeydown = (event: KeyboardEvent) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      addMethod(newMethod);
      newMethod = '';
    }
  };

  const handleMethodBlur = () => {
    if (newMethod.trim()) {
      addMethod(newMethod);
      newMethod = '';
    }
  };

  // ---------------------------------------------------------------------------
  // Service helpers
  // ---------------------------------------------------------------------------

  const serviceExists = (serviceName: string): boolean =>
    services.some((s) => s.name === serviceName);

  // ---------------------------------------------------------------------------
  // Delete handling
  // ---------------------------------------------------------------------------

  const handleDelete = () => {
    if (showDeleteConfirm) {
      dispatch('deleteRoute', routeIndex);
      showDeleteConfirm = false;
    } else {
      showDeleteConfirm = true;
      setTimeout(() => {
        showDeleteConfirm = false;
      }, 3000);
    }
  };

  const handleCancelDelete = () => {
    showDeleteConfirm = false;
  };

  // ---------------------------------------------------------------------------
  // Save handling
  // ---------------------------------------------------------------------------

  const handleSave = () => {
    dispatch('save', editRoute);
  };
</script>

<div class="space-y-6">
  <!-- Back Button -->
  <button
    class="inline-flex items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 text-sm font-medium text-foreground/80 transition-colors hover:bg-muted hover:text-foreground"
    on:click={() => dispatch('close')}
  >
    <span class="text-base">←</span>
    Back to Routes
  </button>

  <!-- Route Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold text-foreground">
        {editRoute.name || 'Unnamed Route'}
      </h2>
      <p class="mt-1 text-sm text-muted-foreground">
        Route #{routeIndex + 1} · {isViewMode ? 'View Mode' : 'Edit Mode'}
        {#if editRoute.is_default}
          <span class="ml-2 rounded-full border border-success/40 bg-success/10 px-2 py-0.5 text-xs font-semibold text-success">
            Default
          </span>
        {/if}
      </p>
    </div>

    <!-- Action Buttons (Edit mode only) -->
    {#if !isViewMode}
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg border border-primary/40 bg-primary/10 px-3 py-2 text-sm font-semibold text-primary transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-60"
          disabled={saving}
          on:click={handleSave}
        >
          {#if saving}
            <span class="inline-flex items-center gap-2">⟳ Saving...</span>
          {:else}
            Save Changes
          {/if}
        </button>
        <button
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm font-medium text-destructive transition-colors hover:bg-destructive/20 disabled:cursor-not-allowed disabled:opacity-60"
          disabled={saving}
          on:click={handleDelete}
        >
          {#if showDeleteConfirm}
            Confirm Delete?
          {:else}
            Delete Route
          {/if}
        </button>
        {#if showDeleteConfirm}
          <button
            class="rounded-lg border border-border bg-muted px-3 py-2 text-sm font-medium text-foreground/80 transition-colors hover:bg-muted disabled:cursor-not-allowed disabled:opacity-60"
            disabled={saving}
            on:click={handleCancelDelete}
          >
            Cancel
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Error Banner -->
  {#if errorMessage}
    <div class="rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm font-medium text-destructive">
      <span class="mr-2">⚠</span>
      {errorMessage}
    </div>
  {/if}

  <!-- ========================================================================= -->
  <!-- Section: Route Configuration -->
  <!-- ========================================================================= -->
  <section class="rounded-xl border border-border/80 bg-card/80 backdrop-blur">
    <div class="flex items-center gap-3 border-b border-border/80 px-5 py-4">
      <div class="h-8 w-1 rounded-full bg-primary" ></div>
      <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
        Route Configuration
      </h3>
    </div>

    <div class="grid gap-5 p-5 md:grid-cols-2">
      <!-- Name -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-foreground/80">Name</span>
        <input
          type="text"
          class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:cursor-not-allowed disabled:bg-muted/50 disabled:text-muted-foreground"
          disabled={isViewMode || saving}
          value={editRoute.name}
          placeholder="e.g., api-gateway"
          on:input={(e) => updateField('name', inputValue(e))}
        />
      </label>

      <!-- Service -->
      <div class="space-y-1.5">
        <span class="text-sm font-medium text-foreground/80">Service</span>
        {#if isViewMode}
          <div class="flex h-[42px] items-center gap-2 rounded-lg border border-border bg-muted/50 px-3">
            {#if serviceExists(editRoute.service)}
              <span class="rounded-lg border border-primary/40 bg-primary/10 px-2 py-0.5 text-xs font-semibold text-primary">
                {editRoute.service}
              </span>
            {:else}
              <span class="text-sm text-destructive">
                {editRoute.service || '(none)'} — not found
              </span>
            {/if}
          </div>
        {:else}
          <label class="block">
            <select
              class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={saving}
              value={editRoute.service}
              on:change={(e) => updateField('service', selectValue(e))}
            >
              <option value="">— Select a service —</option>
              {#each services as svc}
                <option value={svc.name}>{svc.name}</option>
              {/each}
            </select>
          </label>
          {#if editRoute.service && !serviceExists(editRoute.service)}
            <p class="text-xs text-destructive">Warning: Service "{editRoute.service}" not found</p>
          {/if}
        {/if}
      </div>

      <!-- Host -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-foreground/80">Host <span class="text-muted-foreground">(optional)</span></span>
        <input
          type="text"
          class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:cursor-not-allowed disabled:bg-muted/50 disabled:text-muted-foreground"
          disabled={isViewMode || saving}
          value={editRoute.host}
          placeholder="e.g., api.example.com"
          on:input={(e) => updateField('host', inputValue(e))}
        />
      </label>

      <!-- Path Prefix -->
      <label class="space-y-1.5">
        <span class="text-sm font-medium text-foreground/80">Path Prefix</span>
        <input
          type="text"
          class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm font-mono text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 disabled:cursor-not-allowed disabled:bg-muted/50 disabled:text-muted-foreground"
          disabled={isViewMode || saving}
          value={editRoute.path_prefix}
          placeholder="/"
          on:input={(e) => updateField('path_prefix', inputValue(e))}
        />
      </label>
    </div>

    <!-- Methods -->
    <div class="border-t border-border/80 px-5 py-4">
      <label class="space-y-3">
        <div>
          <span class="text-sm font-medium text-foreground/80">Methods</span>
          <p class="mt-0.5 text-xs text-muted-foreground">
            HTTP methods this route matches. Leave empty to match all methods.
          </p>
        </div>

        <!-- Method Tags -->
        <div class="flex flex-wrap items-center gap-2">
          {#each editRoute.methods as method, idx}
            <span class="inline-flex items-center gap-1.5 rounded-lg border border-border bg-muted px-2.5 py-1 text-sm">
              <span class="font-medium text-primary">{method}</span>
              {#if !isViewMode && !saving}
                <button
                  type="button"
                  class="ml-0.5 text-muted-foreground transition-colors hover:text-destructive"
                  on:click={() => removeMethod(idx)}
                  title="Remove {method}"
                >
                  ×
                </button>
              {/if}
            </span>
          {/each}

          {#if !isViewMode && !saving}
            <input
              type="text"
              class="w-28 rounded-lg border border-border bg-background/70 px-2.5 py-1 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
              placeholder="e.g. GET"
              value={newMethod}
              on:input={(e) => (newMethod = inputValue(e))}
              on:keydown={handleMethodKeydown}
              on:blur={handleMethodBlur}
            />
          {/if}

          {#if editRoute.methods.length === 0}
            <span class="text-xs text-muted-foreground italic">No methods specified = all methods allowed</span>
          {/if}
        </div>

        {#if !isViewMode}
          <p class="text-xs text-muted-foreground/70">
            Valid methods: {VALID_METHODS.join(', ')}
          </p>
        {/if}
      </label>
    </div>

    <!-- Default Route Toggle -->
    <div class="border-t border-border/80 px-5 py-4">
      <label class="flex items-center justify-between">
        <div>
          <span class="text-sm font-medium text-foreground/80">Default Route</span>
          <p class="mt-0.5 text-xs text-muted-foreground">This route will handle requests that don't match any other route</p>
        </div>
        <!-- Toggle Switch -->
        <div class="relative">
          <input
            type="checkbox"
            class="peer sr-only"
            id="is-default-toggle"
            disabled={isViewMode || saving}
            checked={editRoute.is_default}
            on:change={(e) => updateField('is_default', checkedValue(e))}
          />
          <label
            for="is-default-toggle"
            class="inline-flex h-6 w-11 cursor-pointer items-center rounded-full border border-border bg-muted transition-colors peer-checked:border-primary peer-checked:bg-primary/30 peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
          >
            <span class="ml-0.5 h-5 w-5 rounded-full border border-border bg-muted-foreground shadow-sm transition-transform peer-checked:translate-x-5 peer-checked:border-primary peer-checked:bg-primary" ></span>
          </label>
        </div>
      </label>
    </div>
  </section>

  <!-- ========================================================================= -->
  <!-- Info: Service Reference -->
  <!-- ========================================================================= -->
  <section class="rounded-xl border border-border/80 bg-card/80 backdrop-blur">
    <div class="flex items-center gap-3 border-b border-border/80 px-5 py-4">
      <div class="h-8 w-1 rounded-full bg-primary" ></div>
      <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
        About Routes
      </h3>
    </div>
    <div class="space-y-3 p-5 text-sm text-muted-foreground">
      <p>
        Routes define <span class="font-medium text-foreground/80">matching rules</span> for incoming requests
        based on host, path prefix, and HTTP methods.
      </p>
      <p>
        Each route points to a <span class="font-medium text-foreground/80">service</span> which contains
        the load balancing strategy, circuit breaker settings, and upstream servers.
      </p>
      <p>
        When a request matches a route, it is forwarded to the associated service for processing.
        Configure services on the <span class="font-medium text-primary">Services</span> page.
      </p>
    </div>
  </section>
</div>