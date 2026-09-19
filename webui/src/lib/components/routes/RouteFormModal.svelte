<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RouteConfig, ServiceConfig } from '../../types/config';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  export let route: RouteConfig | null = null;
  export let existingNames: string[] = [];
  export let services: ServiceConfig[] = [];

  const dispatch = createEventDispatcher<{
    save: RouteConfig;
    cancel: void;
  }>();

  // ---------------------------------------------------------------------------
  // Constants
  // ---------------------------------------------------------------------------

  const VALID_METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS'] as const;
  type HttpMethod = (typeof VALID_METHODS)[number];

  // ---------------------------------------------------------------------------
  // Form State
  // ---------------------------------------------------------------------------

  let name = route?.name ?? '';
  let service = route?.service ?? '';
  let host = route?.host ?? '';
  let pathPrefix = route?.path_prefix ?? '/';
  let selectedMethods: Set<HttpMethod> = new Set(
    (route?.methods ?? []) as HttpMethod[]
  );
  let isDefault = route?.is_default ?? false;

  $: isEditing = route !== null;
  $: originalName = route?.name ?? '';

  // ---------------------------------------------------------------------------
  // Validation
  // ---------------------------------------------------------------------------

  let errors: Array<{ field: string; message: string }> = [];

  $: nameError = errors.find((e) => e.field === 'name')?.message ?? '';
  $: serviceError = errors.find((e) => e.field === 'service')?.message ?? '';
  $: pathPrefixError = errors.find((e) => e.field === 'pathPrefix')?.message ?? '';
  $: isDefaultWarning = checkDefaultWarning();

  function checkDefaultWarning(): string {
    if (!isDefault) return '';
    // Check if there's already a default route (excluding current one if editing)
    // We can't directly check existing routes for is_default, but we can show a general warning
    // The parent component should handle this logic, but we can show a hint
    if (isEditing && route?.is_default) return '';
    return 'Only one route can be the default. This will replace the existing default route.';
  }

  function validate(): boolean {
    errors = [];

    // Name validation
    if (!name.trim()) {
      errors.push({ field: 'name', message: 'Route name is required' });
    } else {
      const trimmedName = name.trim();
      const otherNames = existingNames.filter((n) => n !== originalName);
      if (otherNames.includes(trimmedName)) {
        errors.push({ field: 'name', message: 'A route with this name already exists' });
      }
    }

    // Service validation
    if (!service) {
      errors.push({ field: 'service', message: 'Service is required' });
    } else if (!services.some((s) => s.name === service)) {
      errors.push({ field: 'service', message: 'Selected service does not exist' });
    }

    // Path prefix validation
    if (!pathPrefix.trim()) {
      errors.push({ field: 'pathPrefix', message: 'Path prefix is required' });
    } else if (!pathPrefix.startsWith('/')) {
      errors.push({ field: 'pathPrefix', message: 'Path prefix must start with /' });
    }

    return errors.length === 0;
  }

  // ---------------------------------------------------------------------------
  // Event Handlers
  // ---------------------------------------------------------------------------

  function handleSave(): void {
    if (!validate()) return;

    const routeData: RouteConfig = {
      name: name.trim(),
      service,
      host: host.trim(),
      path_prefix: pathPrefix,
      methods: Array.from(selectedMethods),
      is_default: isDefault,
    };

    dispatch('save', routeData);
  }

  function handleCancel(): void {
    dispatch('cancel');
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      handleCancel();
    }
  }

  function toggleMethod(method: HttpMethod): void {
    const newMethods = new Set(selectedMethods);
    if (newMethods.has(method)) {
      newMethods.delete(method);
    } else {
      newMethods.add(method);
    }
    selectedMethods = newMethods;
  }

  // ---------------------------------------------------------------------------
  // Input Helpers
  // ---------------------------------------------------------------------------

  function inputValue(event: Event): string {
    return (event.currentTarget as HTMLInputElement).value;
  }

  function selectValue(event: Event): string {
    return (event.currentTarget as HTMLSelectElement).value;
  }

  function checkedValue(event: Event): boolean {
    return (event.currentTarget as HTMLInputElement).checked;
  }

  // ---------------------------------------------------------------------------
  // Computed
  // ---------------------------------------------------------------------------

  $: canSave = name.trim() !== '' && service !== '' && pathPrefix.trim() !== '' && pathPrefix.startsWith('/');
</script>

<svelte:window on:keydown={handleKeydown} />

<!-- Modal Overlay -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
  aria-labelledby="modal-title"
  tabindex="-1"
>
  <!-- Backdrop. A button rather than a div so dismissing the dialog by
       clicking outside it is reachable from the keyboard too. -->
  <button
    type="button"
    class="absolute inset-0 bg-black/60 backdrop-blur-sm"
    aria-label="Close dialog"
    on:click={handleCancel}
  ></button>

  <!-- Modal Panel -->
  <div class="relative z-10 w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-xl border border-border/80 bg-card shadow-2xl shadow-black/50">
    <!-- ========================================================================= -->
    <!-- Header -->
    <!-- ========================================================================= -->
    <div class="flex items-center justify-between border-b border-border/80 px-6 py-4">
      <div>
        <h2 id="modal-title" class="text-lg font-bold text-foreground">
          {isEditing ? 'Edit Route' : 'Create Route'}
        </h2>
        <p class="mt-0.5 text-sm text-muted-foreground">
          {isEditing ? `Editing "${originalName}"` : 'Configure a new routing rule'}
        </p>
      </div>
      <button
        type="button"
        class="flex h-8 w-8 items-center justify-center rounded-lg border border-border bg-muted text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        on:click={handleCancel}
        title="Close"
      >
        <span class="text-lg leading-none">×</span>
      </button>
    </div>

    <!-- ========================================================================= -->
    <!-- Form Content -->
    <!-- ========================================================================= -->
    <div class="space-y-0">
      <!-- Section: Basic Configuration -->
      <section class="border-b border-border/80">
        <div class="flex items-center gap-3 px-6 pt-5 pb-3">
          <div class="h-8 w-1 rounded-full bg-primary" ></div>
          <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
            Basic Configuration
          </h3>
        </div>

        <div class="grid gap-4 px-6 pb-5">
          <!-- Route Name -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-foreground/80">
              Route Name <span class="text-destructive">*</span>
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 {nameError ? 'border-destructive' : ''}"
              value={name}
              placeholder="e.g., api-gateway"
              on:input={(e) => (name = inputValue(e))}
            />
            {#if nameError}
              <p class="text-xs text-destructive">{nameError}</p>
            {/if}
          </label>

          <!-- Service -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-foreground/80">
              Service <span class="text-destructive">*</span>
            </span>
            <select
              class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 {serviceError ? 'border-destructive' : ''}"
              value={service}
              on:change={(e) => (service = selectValue(e))}
            >
              <option value="">— Select a service —</option>
              {#each services as svc}
                <option value={svc.name}>{svc.name}</option>
              {/each}
            </select>
            {#if serviceError}
              <p class="text-xs text-destructive">{serviceError}</p>
            {/if}
            {#if services.length === 0}
              <p class="text-xs text-warning">No services available. Create a service first.</p>
            {/if}
          </label>

          <!-- Host -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-foreground/80">
              Host <span class="text-muted-foreground">(optional)</span>
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30"
              value={host}
              placeholder="e.g., api.example.com"
              on:input={(e) => (host = inputValue(e))}
            />
            <p class="text-xs text-muted-foreground">Leave empty to match any host</p>
          </label>

          <!-- Path Prefix -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-foreground/80">
              Path Prefix <span class="text-destructive">*</span>
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-border bg-background/70 px-3 py-2.5 text-sm font-mono text-foreground placeholder:text-muted-foreground transition-colors focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30 {pathPrefixError ? 'border-destructive' : ''}"
              value={pathPrefix}
              placeholder="/"
              on:input={(e) => (pathPrefix = inputValue(e))}
            />
            {#if pathPrefixError}
              <p class="text-xs text-destructive">{pathPrefixError}</p>
            {:else if pathPrefix && !pathPrefix.startsWith('/')}
              <p class="text-xs text-warning">Path prefix must start with /</p>
            {/if}
          </label>
        </div>
      </section>

      <!-- Section: Methods -->
      <section class="border-b border-border/80">
        <div class="flex items-center gap-3 px-6 pt-5 pb-3">
          <div class="h-8 w-1 rounded-full bg-success" ></div>
          <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
            HTTP Methods
          </h3>
        </div>

        <div class="px-6 pb-5">
          <p class="mb-3 text-xs text-muted-foreground">
            Select methods this route should match. Leave empty to match all methods.
          </p>

          <!-- Method Badges -->
          <div class="flex flex-wrap gap-2">
            {#each VALID_METHODS as method}
              {@const isSelected = selectedMethods.has(method)}
              <button
                type="button"
                class="inline-flex items-center rounded-lg border px-3 py-1.5 text-sm font-medium transition-all
                  {isSelected
                    ? 'border-primary/60 bg-primary/20 text-primary shadow-sm shadow-primary/10'
                    : 'border-border bg-muted/60 text-muted-foreground hover:border-border hover:bg-muted hover:text-foreground/80'}"
                on:click={() => toggleMethod(method)}
                title="{isSelected ? 'Remove' : 'Add'} {method}"
              >
                {#if isSelected}
                  <span class="mr-1.5 text-xs text-primary">✓</span>
                {/if}
                {method}
              </button>
            {/each}
          </div>

          {#if selectedMethods.size === 0}
            <p class="mt-2 text-xs italic text-muted-foreground">
              No methods selected — route will match all HTTP methods
            </p>
          {:else}
            <p class="mt-2 text-xs text-muted-foreground">
              Matching {selectedMethods.size} method{selectedMethods.size !== 1 ? 's' : ''}: {Array.from(selectedMethods).join(', ')}
            </p>
          {/if}
        </div>
      </section>

      <!-- Section: Advanced -->
      <section>
        <div class="flex items-center gap-3 px-6 pt-5 pb-3">
          <div class="h-8 w-1 rounded-full bg-warning" ></div>
          <h3 class="text-sm font-semibold uppercase tracking-wider text-foreground">
            Advanced
          </h3>
        </div>

        <div class="px-6 pb-5">
          <!-- Default Route Toggle -->
          <div class="flex items-center justify-between">
            <div>
              <span class="text-sm font-medium text-foreground/80">Default Route</span>
              <p class="mt-0.5 text-xs text-muted-foreground">
                Handle requests that don't match any other route
              </p>
            </div>
            <!-- Toggle Switch -->
            <div class="relative">
              <input
                type="checkbox"
                class="peer sr-only"
                id="modal-is-default-toggle"
                checked={isDefault}
                on:change={(e) => (isDefault = checkedValue(e))}
              />
              <label
                for="modal-is-default-toggle"
                class="inline-flex h-6 w-11 cursor-pointer items-center rounded-full border border-border bg-muted transition-colors peer-checked:border-primary peer-checked:bg-primary/30"
              >
                <span class="ml-0.5 h-5 w-5 rounded-full border border-border bg-muted-foreground shadow-sm transition-transform peer-checked:translate-x-5 peer-checked:border-primary peer-checked:bg-primary" ></span>
              </label>
            </div>
          </div>

          {#if isDefaultWarning}
            <div class="mt-3 rounded-lg border border-warning/40 bg-warning/10 px-3 py-2">
              <p class="text-xs text-warning">
                <span class="mr-1">⚠</span>
                {isDefaultWarning}
              </p>
            </div>
          {/if}
        </div>
      </section>
    </div>

    <!-- ========================================================================= -->
    <!-- Footer -->
    <!-- ========================================================================= -->
    <div class="flex items-center justify-end gap-3 border-t border-border/80 bg-card/50 px-6 py-4">
      <button
        type="button"
        class="rounded-lg border border-border bg-muted px-4 py-2 text-sm font-medium text-foreground/80 transition-colors hover:bg-muted hover:text-foreground"
        on:click={handleCancel}
      >
        Cancel
      </button>
      <button
        type="button"
        class="rounded-lg border border-primary/40 bg-primary/10 px-4 py-2 text-sm font-semibold text-primary transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-50"
        disabled={!canSave}
        on:click={handleSave}
      >
        {isEditing ? 'Save Changes' : 'Create Route'}
      </button>
    </div>
  </div>
</div>