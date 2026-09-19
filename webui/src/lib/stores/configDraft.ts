/**
 * The config draft (T307).
 *
 * One place owns everything about editing the config file by hand: what the
 * proxy is running, what the tab has typed, what the server says about it, and
 * whether someone else has moved the file underneath. T308 edits the same draft
 * through forms, so the rule is the same either way — nothing reaches the proxy
 * without a diff and an Apply.
 *
 * The draft survives a closed tab in `localStorage`, keyed by the version it
 * was based on: a draft restored onto a config that has since changed is
 * announced rather than silently applied over the top.
 */

import { derived, get, writable } from 'svelte/store';

import {
  applyConfigText,
  editConfigText,
  loadConfigEtag,
  loadConfigText,
  validateConfigText,
  type ApplyResult,
  type ConfigEditOp,
  type ValidationReport
} from '../api/configText';
import { summarizeChanges, type ChangeSummary } from '../configChanges';
import { diffStats, diffText } from '../configDiff';
import type { PrxConfig } from '../types/config';

const STORAGE_KEY = 'prx-config-draft';
const VALIDATE_DEBOUNCE_MS = 400;
/** How often the file on disk is checked for edits from elsewhere. */
const WATCH_INTERVAL_MS = 10_000;

export interface ConfigBase {
  /** The file as the proxy has it. */
  toml: string;
  /** Version tag sent back as `If-Match`. */
  etag: string | null;
  /** The parsed form of the same file, for the change summary. */
  config: PrxConfig | null;
}

export interface StoredDraft {
  toml: string;
  baseEtag: string | null;
  savedAt: number;
}

export const base = writable<ConfigBase>({ toml: '', etag: null, config: null });
export const draft = writable<string>('');
export const report = writable<ValidationReport | null>(null);
export const validating = writable(false);
export const loading = writable(false);
export const applying = writable(false);
export const loadError = writable<string>('');
/** Why the last field edit could not be applied, if it could not. */
export const editError = writable<string>('');
/**
 * The version on disk when it stopped matching what this draft is based on.
 * Set by the watcher, and by a validate round that noticed first.
 */
export const externalEtag = writable<string | null>(null);
/** Set when a draft was restored from storage rather than typed just now. */
export const restoredAt = writable<number | null>(null);

export const isDirty = derived([base, draft], ([$base, $draft]) => $draft !== $base.toml);

/**
 * The draft as a config, for the Settings forms to read their values from.
 *
 * It is the last valid parse: a draft with a typo in it keeps showing the
 * values the forms had a moment ago rather than emptying every field.
 */
export const draftConfig = derived([report, base], ([$report, $base]) =>
  $report?.config ?? $base.config
);

export const diffOps = derived([base, draft], ([$base, $draft]) =>
  diffText($base.toml, $draft)
);

export const stats = derived(diffOps, ($ops) => diffStats($ops));

/** What the change means, once the draft is valid enough to be parsed. */
export const changeSummary = derived<
  [typeof base, typeof report],
  ChangeSummary | null
>([base, report], ([$base, $report]) => {
  if (!$base.config || !$report?.config) return null;
  return summarizeChanges($base.config, $report.config);
});

const toErrorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

export function readStoredDraft(): StoredDraft | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as StoredDraft;
    return typeof parsed?.toml === 'string' ? parsed : null;
  } catch {
    // Blocked or corrupt storage is not a reason to fail to open the editor.
    return null;
  }
}

function writeStoredDraft(stored: StoredDraft | null): void {
  try {
    if (stored) localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Same: the draft still lives in the tab.
  }
}

// ---------------------------------------------------------------------------
// Editing
// ---------------------------------------------------------------------------

let validateTimer: number | null = null;
let validateToken = 0;

/** Records a keystroke: stores the draft and schedules a validation. */
export function setDraft(text: string): void {
  draft.set(text);

  const current = get(base);
  if (text === current.toml) {
    writeStoredDraft(null);
    restoredAt.set(null);
  } else {
    writeStoredDraft({ toml: text, baseEtag: current.etag, savedAt: Date.now() });
  }

  scheduleValidate(text);
}

function scheduleValidate(text: string): void {
  if (validateTimer !== null) window.clearTimeout(validateTimer);
  validateTimer = window.setTimeout(() => {
    validateTimer = null;
    void runValidate(text);
  }, VALIDATE_DEBOUNCE_MS);
}

/** Validates now, cancelling any pending debounce. */
export async function validateNow(): Promise<ValidationReport | null> {
  if (validateTimer !== null) {
    window.clearTimeout(validateTimer);
    validateTimer = null;
  }
  return runValidate(get(draft));
}

async function runValidate(text: string): Promise<ValidationReport | null> {
  const token = ++validateToken;
  validating.set(true);
  try {
    const result = await validateConfigText(text);
    // A slower answer to an older keystroke must not overwrite a newer one.
    if (token !== validateToken) return null;
    report.set(result);
    noticeEtag(result.currentEtag);
    return result;
  } catch (error) {
    if (token !== validateToken) return null;
    loadError.set(toErrorMessage(error));
    return null;
  } finally {
    if (token === validateToken) validating.set(false);
  }
}

/** Flags a file that has moved on from what this draft is based on. */
function noticeEtag(etag: string | null): void {
  if (!etag) return;
  const current = get(base);
  externalEtag.set(current.etag && etag !== current.etag ? etag : null);
}

// ---------------------------------------------------------------------------
// Loading and applying
// ---------------------------------------------------------------------------

/**
 * Loads the file the proxy is running.
 *
 * An unapplied draft is kept unless `discardDraft` says otherwise — reloading
 * to see what someone else changed should not throw away an hour of work.
 */
export async function loadBase(options: { discardDraft?: boolean } = {}): Promise<void> {
  loading.set(true);
  loadError.set('');
  try {
    const { toml, etag } = await loadConfigText();
    const validation = await validateConfigText(toml);
    base.set({ toml, etag, config: validation.config });
    externalEtag.set(null);

    const stored = readStoredDraft();
    const keepDraft = !options.discardDraft && get(isDirty) && get(draft) !== '';

    if (keepDraft) {
      // The draft now sits on a different base, so it is stored against it.
      writeStoredDraft({ toml: get(draft), baseEtag: etag, savedAt: Date.now() });
      await runValidate(get(draft));
      return;
    }

    if (!options.discardDraft && stored && stored.toml !== toml) {
      draft.set(stored.toml);
      restoredAt.set(stored.savedAt);
      // Restored onto a file that has changed since: the diff is against what
      // is running now, which is what the operator has to review.
      writeStoredDraft({ ...stored, baseEtag: etag });
      await runValidate(stored.toml);
      return;
    }

    draft.set(toml);
    restoredAt.set(null);
    writeStoredDraft(null);
    report.set(validation);
  } catch (error) {
    loadError.set(toErrorMessage(error));
  } finally {
    loading.set(false);
  }
}

/** Throws the draft away and goes back to what the proxy is running. */
export function revertDraft(): void {
  const current = get(base);
  draft.set(current.toml);
  restoredAt.set(null);
  writeStoredDraft(null);
  void runValidate(current.toml);
}

/** Replaces the draft, e.g. when taking the other side of a conflict. */
export function replaceDraft(text: string): void {
  setDraft(text);
}

/**
 * Changes fields in the draft (T308).
 *
 * The rewrite happens on the server, where `toml_edit` can change the keys
 * named without touching anything else in the file, and the answer carries the
 * validation report so a form that has just broken the config says so at once.
 *
 * Edits are queued: two fields changed in quick succession are applied in
 * order, each to the result of the one before, instead of racing to overwrite
 * each other's text.
 */
export async function editDraft(ops: ConfigEditOp[]): Promise<boolean> {
  if (ops.length === 0) return true;

  editQueue = editQueue
    .catch(() => undefined)
    .then(async () => {
      const result = await editConfigText(get(draft), ops);
      setDraftText(result.toml);
      report.set(result.report);
      noticeEtag(result.report.currentEtag);
    });

  try {
    await editQueue;
    editError.set('');
    return true;
  } catch (error) {
    // A draft that is not valid TOML cannot be patched key by key; the editor
    // is the place to fix that, and the message says so.
    editError.set(toErrorMessage(error));
    return false;
  }
}

/** Serialises field edits so they compose instead of racing. */
let editQueue: Promise<void> = Promise.resolve();

/** Stores the text and persists it, without scheduling another validation. */
function setDraftText(text: string): void {
  draft.set(text);
  const current = get(base);
  if (text === current.toml) {
    writeStoredDraft(null);
    restoredAt.set(null);
  } else {
    writeStoredDraft({ toml: text, baseEtag: current.etag, savedAt: Date.now() });
  }
}

export async function applyDraft(): Promise<ApplyResult> {
  applying.set(true);
  try {
    const current = get(base);
    const text = get(draft);
    const result = await applyConfigText(text, current.etag);

    if (result.status === 'applied') {
      const validation = await validateConfigText(text);
      base.set({ toml: text, etag: result.etag ?? null, config: validation.config });
      report.set(validation);
      draft.set(text);
      restoredAt.set(null);
      externalEtag.set(null);
      writeStoredDraft(null);
    } else if (result.status === 'conflict') {
      externalEtag.set(result.currentEtag);
    }

    return result;
  } finally {
    applying.set(false);
  }
}

/**
 * Takes the version now on disk as the base, keeping the draft on top of it.
 *
 * This is what "keep my changes" does after a conflict: the diff is recomputed
 * against what the proxy is actually running.
 */
export async function rebaseOnCurrent(): Promise<void> {
  const { toml, etag } = await loadConfigText();
  const validation = await validateConfigText(toml);
  base.set({ toml, etag, config: validation.config });
  externalEtag.set(null);
  writeStoredDraft({ toml: get(draft), baseEtag: etag, savedAt: Date.now() });
}

/**
 * Watches the file for changes made elsewhere. Returns a teardown function.
 *
 * It is a HEAD request, so watching costs a header and never re-reads the
 * config just to find out that nothing happened.
 */
export function watchExternalChanges(intervalMs = WATCH_INTERVAL_MS): () => void {
  let stopped = false;

  const tick = async () => {
    if (stopped || document.hidden) return;
    try {
      noticeEtag(await loadConfigEtag());
    } catch {
      // The connection badge already says the admin API is unreachable.
    }
  };

  const timer = window.setInterval(() => void tick(), intervalMs);
  return () => {
    stopped = true;
    window.clearInterval(timer);
    if (validateTimer !== null) {
      window.clearTimeout(validateTimer);
      validateTimer = null;
    }
  };
}
