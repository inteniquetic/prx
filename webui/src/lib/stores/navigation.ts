import { writable, derived, get } from 'svelte/store';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type NavPage = 'dashboard' | 'services' | 'routes' | 'settings';

export interface NavItem {
  id: NavPage;
  label: string;
  icon: string;
}

export interface NavigationState {
  currentPage: NavPage;
  sidebarCollapsed: boolean;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const navItems: NavItem[] = [
  { id: 'dashboard', label: 'Dashboard', icon: '◉' },
  { id: 'services', label: 'Services', icon: '◆' },
  { id: 'routes', label: 'Routes', icon: '⇌' },
  { id: 'settings', label: 'Settings', icon: '⚙' },
];

const VALID_PAGES = new Set<NavPage>(navItems.map((item) => item.id));
const DEFAULT_PAGE: NavPage = 'dashboard';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Extract a NavPage from the current URL hash (e.g. `#dashboard`). */
function pageFromHash(): NavPage {
  const hash = window.location.hash.replace(/^#\/?/, '').toLowerCase();
  if (VALID_PAGES.has(hash as NavPage)) {
    return hash as NavPage;
  }
  return DEFAULT_PAGE;
}

/** Update the URL hash without triggering a full navigation event. */
function setHash(page: NavPage): void {
  const target = `#${page}`;
  if (window.location.hash !== target) {
    history.replaceState(null, '', target);
  }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

const initialState: NavigationState = {
  currentPage: pageFromHash(),
  sidebarCollapsed: false,
};

export const navigationStore = writable<NavigationState>(initialState, (set) => {
  // Set the initial hash to match the resolved page
  setHash(get(navigationStore).currentPage);

  const handleHashChange = () => {
    const page = pageFromHash();
    const current = get(navigationStore);
    if (current.currentPage === page) {
      return;
    }
    set({ ...current, currentPage: page });
  };

  window.addEventListener('hashchange', handleHashChange);

  return () => {
    window.removeEventListener('hashchange', handleHashChange);
  };
});

// ---------------------------------------------------------------------------
// Derived stores (convenience accessors)
// ---------------------------------------------------------------------------

export const currentPage = derived(navigationStore, ($nav) => $nav.currentPage);
export const sidebarCollapsed = derived(navigationStore, ($nav) => $nav.sidebarCollapsed);

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/** Navigate to a different page. Updates both the store and the URL hash. */
export function navigate(page: NavPage): void {
  setHash(page);
  navigationStore.update((state) => ({
    ...state,
    currentPage: page,
  }));
}

/** Toggle the sidebar between collapsed and expanded states. */
export function toggleSidebar(): void {
  navigationStore.update((state) => ({
    ...state,
    sidebarCollapsed: !state.sidebarCollapsed,
  }));
}

/** Programmatically set the sidebar collapsed state. */
export function setSidebarCollapsed(collapsed: boolean): void {
  navigationStore.update((state) => ({
    ...state,
    sidebarCollapsed: collapsed,
  }));
}