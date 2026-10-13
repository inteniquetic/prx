/** Config history (T202) and the audit log (T206). */

import { adminFetch } from './auth';

export interface ConfigVersion {
  id: string;
  ts_ms: number;
  fingerprint: string;
  bytes: number;
}

export interface AuditRecord {
  ts_ms: number;
  actor: string;
  method: string;
  path: string;
  status: number;
  /** Config fingerprints; they match `ConfigVersion.fingerprint`. */
  before: string;
  after: string;
}

async function getJson<T>(url: string): Promise<T> {
  const response = await adminFetch(url, { cache: 'no-store' });
  if (!response.ok) throw new Error(`${url} -> ${response.status}`);
  return (await response.json()) as T;
}

export const loadHistory = () => getJson<ConfigVersion[]>('/web/config/history');
export const loadAudit = (limit = 200) => getJson<AuditRecord[]>(`/web/audit?limit=${limit}`);

export async function loadVersionText(id: string): Promise<string> {
  const response = await adminFetch(`/web/config/history/${encodeURIComponent(id)}`, {
    cache: 'no-store'
  });
  if (!response.ok) throw new Error(`version ${id} -> ${response.status}`);
  return response.text();
}

/** Rejects with the server's reason when that version no longer validates. */
export async function rollbackTo(id: string): Promise<void> {
  const response = await adminFetch(`/web/config/rollback/${encodeURIComponent(id)}`, {
    method: 'POST'
  });
  if (!response.ok) throw new Error((await response.text()).trim() || `HTTP ${response.status}`);
}
