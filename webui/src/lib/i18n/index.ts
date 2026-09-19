/**
 * Thai and English, switchable (T309).
 *
 * Small on purpose: a store holding the chosen locale, a lookup that falls
 * back to English, and `{placeholder}` interpolation. No library — the whole
 * of it is forty lines, and `npm run i18n:check` is what keeps it honest by
 * failing the build on a key that exists in one language and not the other, or
 * on a string rendered from a component without going through here.
 *
 * Keys read like a path through the UI: `routes.empty.title`, not `str_142`.
 */

import { derived, get, writable } from 'svelte/store';

import { en } from './en';
import { th } from './th';

export type Locale = 'en' | 'th';

export const LOCALES: { id: Locale; label: string; english: string }[] = [
  { id: 'en', label: 'English', english: 'English' },
  { id: 'th', label: 'ไทย', english: 'Thai' }
];

const STORAGE_KEY = 'prx-locale';

const DICTIONARIES: Record<Locale, Record<string, string>> = { en, th };

function readStoredLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'en' || stored === 'th') return stored;
  } catch {
    // Blocked storage: fall through to the browser's own preference.
  }
  try {
    return navigator.language?.toLowerCase().startsWith('th') ? 'th' : 'en';
  } catch {
    return 'en';
  }
}

export const locale = writable<Locale>(readStoredLocale());

/**
 * Applies the stored locale to `<html lang>`.
 *
 * Screen readers pick their voice from that attribute, so a Thai UI announced
 * with an English voice is the bug this prevents. Called once at startup;
 * `setLocale` keeps it in step after that.
 */
export function initLocale(): Locale {
  const current = readStoredLocale();
  locale.set(current);
  try {
    document.documentElement.lang = current;
  } catch {
    // No document in a test harness.
  }
  return current;
}

export function setLocale(next: Locale): void {
  locale.set(next);
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // The choice still applies to this session.
  }
  try {
    document.documentElement.lang = next;
  } catch {
    // No document in a test harness.
  }
}

export type TranslationParams = Record<string, string | number>;

function interpolate(template: string, params?: TranslationParams): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole
  );
}

function lookup(dictionary: Record<string, string>, key: string): string | undefined {
  return Object.hasOwn(dictionary, key) ? dictionary[key] : undefined;
}

/** Translates a key. Unknown keys come back as the key, which is visible. */
export function translate(locale: Locale, key: string, params?: TranslationParams): string {
  const text = lookup(DICTIONARIES[locale], key) ?? lookup(en, key);
  return interpolate(text ?? key, params);
}

/**
 * The translator, as a store: `{$t('routes.title')}` in a component.
 *
 * It re-derives when the locale changes, which is what re-renders every string
 * on screen without a reload.
 */
export const t = derived(
  locale,
  ($locale) =>
    (key: string, params?: TranslationParams): string =>
      translate($locale, key, params)
);

/**
 * Counted things: `plural('routes.count', 3)` reads `routes.count.one` or
 * `routes.count.other`, with `{count}` filled in.
 *
 * Thai does not inflect for number, so its two forms are usually the same
 * sentence — they are still both present, because the check requires it and
 * because a language that needs the distinction later gets it for free.
 */
export const plural = derived(
  locale,
  ($locale) =>
    (key: string, count: number, params?: TranslationParams): string =>
      translate($locale, `${key}.${count === 1 ? 'one' : 'other'}`, { count, ...params })
);

/** Non-reactive access, for code outside a component. */
export function tr(key: string, params?: TranslationParams): string {
  return translate(get(locale), key, params);
}

// ---------------------------------------------------------------------------
// Formatting that follows the locale
// ---------------------------------------------------------------------------

const INTL_LOCALE: Record<Locale, string> = { en: 'en-US', th: 'th-TH' };

/** A date and time, in the reader's locale but never in the Buddhist era. */
export function formatDateTime(epochMs: number, localeId: Locale = get(locale)): string {
  // `th-TH` defaults to the Buddhist calendar, which is right for a shopping
  // site and wrong next to a certificate expiry that came from a server log.
  return new Intl.DateTimeFormat(`${INTL_LOCALE[localeId]}-u-ca-gregory`, {
    dateStyle: 'medium',
    timeStyle: 'medium'
  }).format(new Date(epochMs));
}

export function formatDate(epochMs: number, localeId: Locale = get(locale)): string {
  return new Intl.DateTimeFormat(`${INTL_LOCALE[localeId]}-u-ca-gregory`, {
    dateStyle: 'medium'
  }).format(new Date(epochMs));
}

/** Every key the dictionaries define, for the check script. */
export const KEYS = Object.keys(en);
