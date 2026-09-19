import { derived, get, writable } from 'svelte/store';
import type { Component } from 'svelte';
import LayoutDashboardIcon from '@lucide/svelte/icons/layout-dashboard';
import RouteIcon from '@lucide/svelte/icons/route';
import ServerIcon from '@lucide/svelte/icons/server';
import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
import SettingsIcon from '@lucide/svelte/icons/settings';
import ScrollTextIcon from '@lucide/svelte/icons/scroll-text';

// ---------------------------------------------------------------------------
// Routing model
// ---------------------------------------------------------------------------
//
// Real paths, not a hash: `/routes/api-v1` has to survive a refresh and be
// worth pasting into a chat. The admin server already serves index.html for any
// path that is not an embedded asset (`handle_webui_get` in src/admin.rs), so
// the browser lands back here with the same URL and this module resolves it.

export type NavPage = 'dashboard' | 'routes' | 'services' | 'tls' | 'settings' | 'audit';

export interface NavItem {
  id: NavPage;
  /** i18n key; the label itself lives in the dictionaries (T309). */
  labelKey: string;
  icon: Component;
  path: string;
  /** Sidebar grouping. */
  section: 'overview' | 'traffic' | 'operations';
  /** Set while the page is still a placeholder, naming the task that fills it. */
  pending?: string;
  /** Pages that address a single named thing, e.g. /routes/api-v1. */
  detail?: boolean;
}

/** A count the sidebar shows next to a menu entry. */
export interface NavBadge {
  count: number;
  /** i18n key read out after the number, e.g. "upstreams down". */
  labelKey: string;
  tone: 'destructive' | 'warning';
}

export interface RouteLocation {
  page: NavPage;
  /** The `:name` part of a detail URL, already percent-decoded. */
  name: string | null;
  path: string;
}

export const navItems: NavItem[] = [
  {
    id: 'dashboard',
    labelKey: 'nav.dashboard',
    icon: LayoutDashboardIcon,
    path: '/',
    section: 'overview'
  },
  {
    id: 'routes',
    labelKey: 'nav.routes',
    icon: RouteIcon,
    path: '/routes',
    section: 'traffic',
    detail: true
  },
  {
    id: 'services',
    labelKey: 'nav.services',
    icon: ServerIcon,
    path: '/services',
    section: 'traffic',
    detail: true
  },
  { id: 'tls', labelKey: 'nav.tls', icon: ShieldCheckIcon, path: '/tls', section: 'operations' },
  {
    id: 'settings',
    labelKey: 'nav.settings',
    icon: SettingsIcon,
    path: '/settings',
    section: 'operations'
  },
  {
    id: 'audit',
    labelKey: 'nav.audit',
    icon: ScrollTextIcon,
    path: '/audit',
    section: 'operations',
    pending: 'T206'
  }
];

export const navSections: { id: NavItem['section']; labelKey: string }[] = [
  { id: 'overview', labelKey: 'nav.section.overview' },
  { id: 'traffic', labelKey: 'nav.section.traffic' },
  { id: 'operations', labelKey: 'nav.section.operations' }
];

const DEFAULT_PAGE: NavPage = 'dashboard';
const SIDEBAR_STORAGE_KEY = 'prx-sidebar-collapsed';

const itemById = new Map(navItems.map((item) => [item.id, item]));
/** `/routes` -> routes. The dashboard keeps `/` and answers to `/dashboard` too. */
const pageBySegment = new Map<string, NavPage>([
  ['', 'dashboard'],
  ...navItems.map((item) => [item.path.replace(/^\//, ''), item.id] as [string, NavPage])
]);

/** Vite's base, so a UI served under a prefix still builds correct links. */
const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');

function stripBase(pathname: string): string {
  if (BASE && pathname.startsWith(BASE)) return pathname.slice(BASE.length) || '/';
  return pathname;
}

export function pathFor(page: NavPage, name?: string | null): string {
  const item = itemById.get(page);
  const base = item?.path ?? '/';
  const path = name && item?.detail ? `${base}/${encodeURIComponent(name)}` : base;
  return `${BASE}${path}` || '/';
}

/** Turns a browser path into a location, or null when nothing matches. */
export function parsePath(pathname: string): RouteLocation | null {
  const segments = stripBase(pathname)
    .split('/')
    .filter((segment) => segment.length > 0);

  const head = (segments[0] ?? '').toLowerCase();
  const page = pageBySegment.get(head);
  if (!page) return null;

  const item = itemById.get(page);
  if (segments.length > 2 || (segments.length === 2 && !item?.detail)) return null;

  let name: string | null = null;
  if (segments.length === 2) {
    try {
      name = decodeURIComponent(segments[1]);
    } catch {
      // A malformed escape is not a route name anyone meant to type.
      return null;
    }
  }

  return { page, name, path: pathFor(page, name) };
}

function currentLocation(): RouteLocation {
  return parsePath(window.location.pathname) ?? { page: DEFAULT_PAGE, name: null, path: pathFor(DEFAULT_PAGE) };
}

// ---------------------------------------------------------------------------
// Stores
// ---------------------------------------------------------------------------

export const location = writable<RouteLocation>(currentLocation());
export const currentPage = derived(location, ($location) => $location.page);
export const currentName = derived(location, ($location) => $location.name);

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

export const sidebarCollapsed = writable<boolean>(readCollapsed());

export function setSidebarCollapsed(collapsed: boolean): void {
  sidebarCollapsed.set(collapsed);
  try {
    localStorage.setItem(SIDEBAR_STORAGE_KEY, collapsed ? '1' : '0');
  } catch {
    // Blocked storage: the choice still applies for this session.
  }
}

export function toggleSidebar(): void {
  setSidebarCollapsed(!get(sidebarCollapsed));
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

export type NavTarget = NavPage | string | { page: NavPage; name?: string | null };

function resolve(target: NavTarget): RouteLocation {
  if (typeof target === 'object') {
    return { page: target.page, name: target.name ?? null, path: pathFor(target.page, target.name) };
  }
  if (typeof target === 'string' && target.startsWith('/')) {
    return parsePath(target) ?? { page: DEFAULT_PAGE, name: null, path: pathFor(DEFAULT_PAGE) };
  }
  const page = itemById.has(target as NavPage) ? (target as NavPage) : DEFAULT_PAGE;
  return { page, name: null, path: pathFor(page) };
}

export function navigate(target: NavTarget, options: { replace?: boolean } = {}): void {
  const next = resolve(target);
  const current = get(location);
  if (next.path === current.path) return;

  if (options.replace) history.replaceState(null, '', next.path);
  else history.pushState(null, '', next.path);

  location.set(next);
}

/**
 * True for a click that the app should handle itself. Ctrl/Cmd/shift clicks and
 * middle clicks stay with the browser so "open in a new tab" keeps working.
 */
export function isPlainClick(event: MouseEvent): boolean {
  return (
    event.button === 0 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey &&
    !event.defaultPrevented
  );
}

/** Starts URL syncing. Returns a teardown function. */
export function initRouter(): () => void {
  const initial = parsePath(window.location.pathname);
  if (!initial) {
    // An address nobody can serve: land on the dashboard and say so in the bar
    // rather than showing the dashboard under a URL that means something else.
    const fallback = { page: DEFAULT_PAGE, name: null, path: pathFor(DEFAULT_PAGE) };
    history.replaceState(null, '', fallback.path);
    location.set(fallback);
  } else {
    location.set(initial);
  }

  const onPopState = () => location.set(currentLocation());
  window.addEventListener('popstate', onPopState);
  return () => window.removeEventListener('popstate', onPopState);
}
