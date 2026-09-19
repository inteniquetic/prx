/**
 * The admin API as the TOML editor uses it (T307).
 *
 * `admin.ts` speaks in parsed configs; this speaks in the file itself —
 * the text on disk, comments and all — plus the version tag that says which
 * version of it a draft is based on.
 */

import { normalizePrxConfig } from '../configNormalize';
import { reportFailure, reportSuccess } from '../stores/connection';
import type { PrxConfig } from '../types/config';

const CONFIG_ENDPOINT = '/web/config';
const VALIDATE_ENDPOINT = '/web/config/validate';
const REQUEST_TIMEOUT_MS = 10000;

/** One problem in the draft, with the place in the file to mark. */
export interface ConfigDiagnostic {
  severity: 'error' | 'warning';
  /** Stable identifier, e.g. `unknown_service`. */
  code: string;
  /** Field path, e.g. `route[2].service`. Empty for a syntax error. */
  path: string;
  message: string;
  hint?: string;
  /** 1-based. */
  line: number;
  column: number;
  end_line: number;
  end_column: number;
}

export interface ValidationReport {
  valid: boolean;
  errors: ConfigDiagnostic[];
  warnings: ConfigDiagnostic[];
  /** The parsed config, when the draft is valid. */
  config: PrxConfig | null;
  /** The version of the file on disk when this was checked. */
  currentEtag: string | null;
}

export interface ConfigText {
  toml: string;
  /** Opaque version tag; sent back as `If-Match` when applying. */
  etag: string | null;
}

export type ApplyResult =
  | { status: 'applied'; message: string; etag: string | null }
  | { status: 'conflict'; expectedEtag: string; currentEtag: string; currentToml: string }
  | { status: 'rejected'; message: string };

const fetchWithTimeout = async (
  input: RequestInfo | URL,
  init?: RequestInit,
  timeoutMs = REQUEST_TIMEOUT_MS
): Promise<Response> => {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(input, { ...init, signal: controller.signal });
    reportSuccess();
    return response;
  } catch (error) {
    reportFailure(error);
    throw error;
  } finally {
    window.clearTimeout(timeoutId);
  }
};

/** The config file as it is on disk, with the version it is at. */
export const loadConfigText = async (): Promise<ConfigText> => {
  const response = await fetchWithTimeout(CONFIG_ENDPOINT, {
    method: 'GET',
    headers: { Accept: 'text/plain' },
    cache: 'no-store'
  });

  if (!response.ok) {
    const reason = (await response.text()).trim() || response.statusText;
    throw new Error(`load_config failed (${response.status}): ${reason}`);
  }

  return { toml: await response.text(), etag: response.headers.get('ETag') };
};

/**
 * The version of the file on disk, without its contents.
 *
 * Used while a draft is open to notice that someone else changed the config —
 * from another tab, the Routes page, or by editing the file directly.
 */
export const loadConfigEtag = async (): Promise<string | null> => {
  const response = await fetchWithTimeout(CONFIG_ENDPOINT, { method: 'HEAD', cache: 'no-store' });
  return response.ok ? response.headers.get('ETag') : null;
};

export const validateConfigText = async (toml: string): Promise<ValidationReport> => {
  const response = await fetchWithTimeout(VALIDATE_ENDPOINT, {
    method: 'POST',
    headers: { 'Content-Type': 'text/plain; charset=utf-8', Accept: 'application/json' },
    body: toml
  });

  if (!response.ok) {
    const reason = (await response.text()).trim() || response.statusText;
    throw new Error(`validate_config failed (${response.status}): ${reason}`);
  }

  const payload = (await response.json()) as {
    valid: boolean;
    errors?: ConfigDiagnostic[];
    warnings?: ConfigDiagnostic[];
    config?: unknown;
    current_etag?: string;
  };

  return {
    valid: payload.valid,
    errors: payload.errors ?? [],
    warnings: payload.warnings ?? [],
    config: payload.config ? normalizePrxConfig(payload.config as Partial<PrxConfig>) : null,
    currentEtag: payload.current_etag ?? null
  };
};

/**
 * Applies a draft.
 *
 * `baseEtag` is the version the draft was made from: the server refuses the
 * write when the file has moved on since, and hands back what it has now so
 * the UI can show all three sides.
 */
export const applyConfigText = async (
  toml: string,
  baseEtag: string | null
): Promise<ApplyResult> => {
  const response = await fetchWithTimeout(CONFIG_ENDPOINT, {
    method: 'PUT',
    headers: {
      'Content-Type': 'text/plain; charset=utf-8',
      ...(baseEtag ? { 'If-Match': baseEtag } : {})
    },
    body: toml
  });

  if (response.status === 409) {
    const payload = (await response.json()) as {
      expected_etag?: string;
      current_etag?: string;
      current_toml?: string;
    };
    return {
      status: 'conflict',
      expectedEtag: payload.expected_etag ?? (baseEtag ?? ''),
      currentEtag: payload.current_etag ?? '',
      currentToml: payload.current_toml ?? ''
    };
  }

  const body = (await response.text()).trim();
  if (!response.ok) {
    return {
      status: 'rejected',
      message: body || response.statusText || `apply failed (${response.status})`
    };
  }

  return {
    status: 'applied',
    message: body || 'config_applied',
    etag: response.headers.get('ETag')
  };
};
