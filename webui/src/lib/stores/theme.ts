import { writable } from 'svelte/store';

export type ThemeChoice = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'prx-theme';

/** Kept in sync with the inline script in index.html — see the note there. */
function readStoredChoice(): ThemeChoice {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  } catch {
    // Private mode or blocked site data: fall through to the OS setting.
  }
  return 'system';
}

function systemTheme(): ResolvedTheme {
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function apply(choice: ThemeChoice): ResolvedTheme {
  const resolved = choice === 'system' ? systemTheme() : choice;
  document.documentElement.classList.toggle('dark', resolved === 'dark');
  return resolved;
}

export const themeChoice = writable<ThemeChoice>(readStoredChoice());
export const resolvedTheme = writable<ResolvedTheme>('dark');

export function setTheme(choice: ThemeChoice): void {
  themeChoice.set(choice);
  resolvedTheme.set(apply(choice));
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // Not being able to remember the choice is not a reason to reject it.
  }
}

/** Cycles light -> dark -> system, which is how the toggle button reads. */
export function cycleTheme(current: ThemeChoice): ThemeChoice {
  const next: ThemeChoice = current === 'light' ? 'dark' : current === 'dark' ? 'system' : 'light';
  setTheme(next);
  return next;
}

/**
 * Applies the stored choice and keeps `system` tracking the OS while the page
 * stays open. Returns a teardown function.
 */
export function initTheme(): () => void {
  let choice = readStoredChoice();
  themeChoice.set(choice);
  resolvedTheme.set(apply(choice));

  const unsubscribe = themeChoice.subscribe((value) => {
    choice = value;
  });

  const media = window.matchMedia?.('(prefers-color-scheme: dark)');
  const onSystemChange = () => {
    if (choice === 'system') resolvedTheme.set(apply('system'));
  };
  media?.addEventListener('change', onSystemChange);

  return () => {
    media?.removeEventListener('change', onSystemChange);
    unsubscribe();
  };
}
