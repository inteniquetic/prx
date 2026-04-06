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

  function handleOverlayClick(event: MouseEvent): void {
    // Only close if clicking directly on the overlay, not the modal content
    if (event.target === event.currentTarget) {
      handleCancel();
    }
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

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<!-- Modal Overlay -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center p-4"
  on:click={handleOverlayClick}
  role="dialog"
  aria-modal="true"
  aria-labelledby="modal-title"
>
  <!-- Backdrop -->
  <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" />

  <!-- Modal Panel -->
  <div class="relative z-10 w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-xl border border-slate-700/80 bg-slate-900 shadow-2xl shadow-black/50">
    <!-- ========================================================================= -->
    <!-- Header -->
    <!-- ========================================================================= -->
    <div class="flex items-center justify-between border-b border-slate-700/80 px-6 py-4">
      <div>
        <h2 id="modal-title" class="text-lg font-bold text-slate-100">
          {isEditing ? 'Edit Route' : 'Create Route'}
        </h2>
        <p class="mt-0.5 text-sm text-slate-400">
          {isEditing ? `Editing "${originalName}"` : 'Configure a new routing rule'}
        </p>
      </div>
      <button
        type="button"
        class="flex h-8 w-8 items-center justify-center rounded-lg border border-slate-600 bg-slate-800 text-slate-400 transition-colors hover:bg-slate-700 hover:text-slate-200"
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
      <section class="border-b border-slate-700/80">
        <div class="flex items-center gap-3 px-6 pt-5 pb-3">
          <div class="h-8 w-1 rounded-full bg-cyan-400" />
          <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
            Basic Configuration
          </h3>
        </div>

        <div class="grid gap-4 px-6 pb-5">
          <!-- Route Name -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-slate-300">
              Route Name <span class="text-rose-400">*</span>
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 {nameError ? 'border-rose-500' : ''}"
              value={name}
              placeholder="e.g., api-gateway"
              on:input={(e) => (name = inputValue(e))}
            />
            {#if nameError}
              <p class="text-xs text-rose-400">{nameError}</p>
            {/if}
          </label>

          <!-- Service -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-slate-300">
              Service <span class="text-rose-400">*</span>
            </span>
            <select
              class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 {serviceError ? 'border-rose-500' : ''}"
              value={service}
              on:change={(e) => (service = selectValue(e))}
            >
              <option value="">— Select a service —</option>
              {#each services as svc}
                <option value={svc.name}>{svc.name}</option>
              {/each}
            </select>
            {#if serviceError}
              <p class="text-xs text-rose-400">{serviceError}</p>
            {/if}
            {#if services.length === 0}
              <p class="text-xs text-amber-400">No services available. Create a service first.</p>
            {/if}
          </label>

          <!-- Host -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-slate-300">
              Host <span class="text-slate-500">(optional)</span>
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30"
              value={host}
              placeholder="e.g., api.example.com"
              on:input={(e) => (host = inputValue(e))}
            />
            <p class="text-xs text-slate-500">Leave empty to match any host</p>
          </label>

          <!-- Path Prefix -->
          <label class="space-y-1.5">
            <span class="text-sm font-medium text-slate-300">
              Path Prefix <span class="text-rose-400">*</span>
            </span>
            <input
              type="text"
              class="w-full rounded-lg border border-slate-600 bg-slate-950/70 px-3 py-2.5 text-sm font-mono text-slate-100 placeholder:text-slate-500 transition-colors focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500/30 {pathPrefixError ? 'border-rose-500' : ''}"
              value={pathPrefix}
              placeholder="/"
              on:input={(e) => (pathPrefix = inputValue(e))}
            />
            {#if pathPrefixError}
              <p class="text-xs text-rose-400">{pathPrefixError}</p>
            {:else if pathPrefix && !pathPrefix.startsWith('/')}
              <p class="text-xs text-amber-400">Path prefix must start with /</p>
            {/if}
          </label>
        </div>
      </section>

      <!-- Section: Methods -->
      <section class="border-b border-slate-700/80">
        <div class="flex items-center gap-3 px-6 pt-5 pb-3">
          <div class="h-8 w-1 rounded-full bg-emerald-400" />
          <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
            HTTP Methods
          </h3>
        </div>

        <div class="px-6 pb-5">
          <p class="mb-3 text-xs text-slate-500">
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
                    ? 'border-cyan-400/60 bg-cyan-500/20 text-cyan-200 shadow-sm shadow-cyan-500/10'
                    : 'border-slate-600 bg-slate-800/60 text-slate-400 hover:border-slate-500 hover:bg-slate-800 hover:text-slate-300'}"
                on:click={() => toggleMethod(method)}
                title="{isSelected ? 'Remove' : 'Add'} {method}"
              >
                {#if isSelected}
                  <span class="mr-1.5 text-xs text-cyan-400">✓</span>
                {/if}
                {method}
              </button>
            {/each}
          </div>

          {#if selectedMethods.size === 0}
            <p class="mt-2 text-xs italic text-slate-500">
              No methods selected — route will match all HTTP methods
            </p>
          {:else}
            <p class="mt-2 text-xs text-slate-500">
              Matching {selectedMethods.size} method{selectedMethods.size !== 1 ? 's' : ''}: {Array.from(selectedMethods).join(', ')}
            </p>
          {/if}
        </div>
      </section>

      <!-- Section: Advanced -->
      <section>
        <div class="flex items-center gap-3 px-6 pt-5 pb-3">
          <div class="h-8 w-1 rounded-full bg-amber-400" />
          <h3 class="text-sm font-semibold uppercase tracking-wider text-slate-200">
            Advanced
          </h3>
        </div>

        <div class="px-6 pb-5">
          <!-- Default Route Toggle -->
          <div class="flex items-center justify-between">
            <div>
              <span class="text-sm font-medium text-slate-300">Default Route</span>
              <p class="mt-0.5 text-xs text-slate-500">
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
                class="inline-flex h-6 w-11 cursor-pointer items-center rounded-full border border-slate-600 bg-slate-700 transition-colors peer-checked:border-cyan-500 peer-checked:bg-cyan-500/30"
              >
                <span class="ml-0.5 h-5 w-5 rounded-full border border-slate-500 bg-slate-300 shadow-sm transition-transform peer-checked:translate-x-5 peer-checked:border-cyan-400 peer-checked:bg-cyan-300" />
              </label>
            </div>
          </div>

          {#if isDefaultWarning}
            <div class="mt-3 rounded-lg border border-amber-400/40 bg-amber-500/10 px-3 py-2">
              <p class="text-xs text-amber-200">
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
    <div class="flex items-center justify-end gap-3 border-t border-slate-700/80 bg-slate-900/50 px-6 py-4">
      <button
        type="button"
        class="rounded-lg border border-slate-600 bg-slate-800 px-4 py-2 text-sm font-medium text-slate-300 transition-colors hover:bg-slate-700 hover:text-slate-100"
        on:click={handleCancel}
      >
        Cancel
      </button>
      <button
        type="button"
        class="rounded-lg border border-cyan-400/40 bg-cyan-500/10 px-4 py-2 text-sm font-semibold text-cyan-200 transition-colors hover:bg-cyan-500/20 disabled:cursor-not-allowed disabled:opacity-50"
        disabled={!canSave}
        on:click={handleSave}
      >
        {isEditing ? 'Save Changes' : 'Create Route'}
      </button>
    </div>
  </div>
</div>