<script lang="ts">
    import { createEventDispatcher } from "svelte";
    import type { RouteHealthItem } from "../../api/admin";
    import type { RouteConfig } from "../../types/config";

    interface RouteRow {
        route: RouteConfig;
    }

    export let routeQuery = "";
    export let rows: RouteRow[] = [];
    export let selectedRouteIndex: number | null = null;
    export let routeHealthByIndex: Record<number, RouteHealthItem> = {};
    export let healthLoading = false;
    export let healthError = "";

    const dispatch = createEventDispatcher<{
        addRoute: void;
        refreshHealth: void;
        search: string;
        view: number;
        edit: number;
        delete: number;
    }>();

    const inputValue = (event: Event): string =>
        (event.currentTarget as HTMLInputElement).value;

    const getRouteHealth = (row: RouteRow): RouteHealthItem | null =>
        routeHealthByIndex[row.route.route_index] ?? null;

    const healthTooltip = (health: RouteHealthItem): string =>
        health.upstreams
            .map((upstream) =>
                upstream.healthy
                    ? `${upstream.addr}: UP (${upstream.latency_ms ?? 0}ms)`
                    : `${upstream.addr}: DOWN (${upstream.error ?? "unreachable"})`,
            )
            .join("\n");

    const routeHealthTooltip = (row: RouteRow): string => {
        const health = getRouteHealth(row);
        return health ? healthTooltip(health) : "";
    };

    const routeHealthLabel = (row: RouteRow): string => {
        const health = getRouteHealth(row);
        if (!health) {
            return "UNKNOWN";
        }
        if (!health.healthy) {
            return "DOWN";
        }
        if (health.reachable_upstreams < health.total_upstreams) {
            return "DEGRADED";
        }
        return "UP";
    };

    const routeHealthLabelClass = (row: RouteRow): string => {
        const label = routeHealthLabel(row);
        if (label === "UP") {
            return "rounded-full border border-emerald-300 bg-emerald-50 px-2 py-0.5 text-xs font-semibold text-emerald-700";
        }
        if (label === "DEGRADED") {
            return "rounded-full border border-amber-300 bg-amber-50 px-2 py-0.5 text-xs font-semibold text-amber-700";
        }
        if (label === "DOWN") {
            return "rounded-full border border-rose-300 bg-rose-50 px-2 py-0.5 text-xs font-semibold text-rose-700";
        }
        return "rounded-full border border-slate-300 bg-slate-100 px-2 py-0.5 text-xs font-semibold text-slate-600";
    };

    const upstreamHealthClass = (healthy: boolean): string =>
        healthy
            ? "rounded border border-emerald-300 bg-emerald-50 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-700"
            : "rounded border border-rose-300 bg-rose-50 px-1.5 py-0.5 text-[10px] font-semibold text-rose-700";

    const upstreamHealthText = (row: RouteRow): string[] => {
        const health = getRouteHealth(row);
        if (!health) {
            return [];
        }
        return health.upstreams.map((upstream, idx) =>
            upstream.healthy ? `U${idx + 1}:UP` : `U${idx + 1}:DOWN`,
        );
    };
</script>

<article
    class="rounded-2xl border border-white/60 bg-white/85 p-4 shadow-panel backdrop-blur"
>
    <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 class="text-base font-bold text-slate-800">Routes</h2>
        <div class="flex w-full items-center gap-2 md:w-auto">
            <input
                class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm md:w-72"
                placeholder="Search route name / host / path / strategy"
                value={routeQuery}
                on:input={(e) => dispatch("search", inputValue(e))}
            />
            <button
                class="rounded-md border border-aqua/40 bg-aqua/10 px-3 py-2 text-xs font-semibold text-sky-800 hover:bg-aqua/20"
                on:click={() => dispatch("addRoute")}
            >
                Add Route
            </button>
            <button
                class="rounded-md border border-emerald-300/70 bg-emerald-50 px-3 py-2 text-xs font-semibold text-emerald-800 hover:bg-emerald-100 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={healthLoading}
                on:click={() => dispatch("refreshHealth")}
            >
                {healthLoading ? "Checking..." : "Check Health"}
            </button>
        </div>
    </div>

    {#if healthError}
        <div
            class="mb-3 rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-xs font-medium text-rose-700"
        >
            Health check failed: {healthError}
        </div>
    {/if}

    <div class="overflow-hidden rounded-xl border border-slate-200 bg-white">
        <div class="max-h-[46vh] overflow-auto">
            <table class="min-w-full divide-y divide-slate-200 text-sm">
                <thead class="sticky top-0 z-10 bg-slate-100 text-slate-700">
                    <tr>
                        <th class="px-4 py-3 text-left font-semibold">Route</th>
                        <th class="px-4 py-3 text-left font-semibold">Host</th>
                        <th class="px-4 py-3 text-left font-semibold">Path</th>
                        <th class="px-4 py-3 text-left font-semibold">LB</th>
                        <th class="px-4 py-3 text-left font-semibold"
                            >Upstreams</th
                        >
                        <th class="px-4 py-3 text-left font-semibold"
                            >Actions</th
                        >
                    </tr>
                </thead>
                <tbody class="divide-y divide-slate-200">
                    {#if rows.length === 0}
                        <tr>
                            <td
                                class="px-4 py-6 text-center text-sm text-slate-500"
                                colspan="7">No routes found.</td
                            >
                        </tr>
                    {:else}
                        {#each rows as row}
                            <tr
                                class={selectedRouteIndex ===
                                row.route.route_index
                                    ? "bg-cyan-50/70"
                                    : "hover:bg-slate-50"}
                            >
                                <td
                                    class="px-4 py-3 font-medium text-slate-900"
                                >
                                    {row.route.name}
                                    {#if row.route.is_default}
                                        <span
                                            class="ml-2 rounded-full bg-emerald-100 px-2 py-0.5 text-xs font-semibold text-emerald-700"
                                            >default</span
                                        >
                                    {/if}
                                </td>
                                <td class="px-4 py-3 text-slate-700"
                                    >{row.route.host || "-"}</td
                                >
                                <td class="px-4 py-3 text-slate-700"
                                    >{row.route.path_prefix}</td
                                >
                                <td class="px-4 py-3 text-slate-700"
                                    >{row.route.lb}</td
                                >
                                <td class="px-4 py-3 text-slate-700"
                                    >{row.route.upstreams.length}</td
                                >
                                <td class="px-4 py-3">
                                    <div class="flex flex-wrap gap-2">
                                        <button
                                            class="rounded-md border border-slate-300 bg-white px-2.5 py-1 text-xs font-semibold text-slate-700 hover:bg-slate-100"
                                            on:click={() =>
                                                dispatch(
                                                    "view",
                                                    row.route.route_index,
                                                )}>View</button
                                        >
                                        <button
                                            class="rounded-md border border-aqua/40 bg-aqua/10 px-2.5 py-1 text-xs font-semibold text-sky-800 hover:bg-aqua/20"
                                            on:click={() =>
                                                dispatch(
                                                    "edit",
                                                    row.route.route_index,
                                                )}>Edit</button
                                        >
                                        <button
                                            class="rounded-md border border-rose-200 bg-rose-50 px-2.5 py-1 text-xs font-semibold text-rose-700 hover:bg-rose-100"
                                            on:click={() =>
                                                dispatch(
                                                    "delete",
                                                    row.route.route_index,
                                                )}>Delete</button
                                        >
                                    </div>
                                </td>
                            </tr>
                        {/each}
                    {/if}
                </tbody>
            </table>
        </div>
    </div>
</article>
