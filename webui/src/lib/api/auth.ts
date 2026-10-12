import { writable } from 'svelte/store';

const KEY = 'prx-admin-authorization';
let authorization = '';
try { authorization = sessionStorage.getItem(KEY) || ''; } catch { /* memory-only */ }
export const loginRequired = writable(false);
export const authScheme = writable<'Bearer' | 'Basic'>('Bearer');

export function setCredential(value: string, scheme: 'Bearer' | 'Basic' = 'Bearer'): void {
  authorization = value ? `${scheme} ${scheme === 'Basic' ? btoa(value) : value}` : '';
  try {
    if (authorization) sessionStorage.setItem(KEY, authorization);
    else sessionStorage.removeItem(KEY);
  } catch { /* storage may be disabled */ }
}

/** Never send admin credentials to a different origin or across redirects. */
export async function adminFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = new URL(input instanceof Request ? input.url : String(input), window.location.href);
  if (url.origin !== window.location.origin) throw new Error('Admin requests must be same-origin');
  const headers = new Headers(init?.headers);
  const sent = authorization;
  if (sent) headers.set('Authorization', sent);
  const response = await fetch(input, { ...init, headers, redirect: 'error', credentials: 'omit' });
  if (response.status === 401 && authorization === sent) {
    setCredential('');
    authScheme.set(response.headers.get('WWW-Authenticate')?.startsWith('Basic') ? 'Basic' : 'Bearer');
    loginRequired.set(true);
  }
  return response;
}

/** Header-bearing SSE: credentials must never be placed in an EventSource URL. */
export class AdminEventSource extends EventTarget {
  static readonly CLOSED = 2;
  readyState = 0;
  private controller = new AbortController();
  constructor(url: string) {
    super();
    void this.read(url);
  }
  close(): void {
    this.readyState = AdminEventSource.CLOSED;
    this.controller.abort();
  }
  private async read(url: string): Promise<void> {
    try {
      const response = await adminFetch(url, { signal: this.controller.signal, headers: { Accept: 'text/event-stream' } });
      if (!response.ok || !response.body) throw new Error('Stream unavailable');
      this.readyState = 1;
      this.dispatchEvent(new Event('open'));
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let pending = '';
      while (!this.controller.signal.aborted) {
        const { value, done } = await reader.read();
        if (done) break;
        pending += decoder.decode(value, { stream: true }).replace(/\r\n/g, '\n');
        let boundary: number;
        while ((boundary = pending.indexOf('\n\n')) >= 0) {
          const frame = pending.slice(0, boundary);
          pending = pending.slice(boundary + 2);
          let name = 'message';
          const data: string[] = [];
          for (const line of frame.split('\n')) {
            if (line.startsWith('event:')) name = line.slice(6).trimStart();
            if (line.startsWith('data:')) data.push(line.slice(5).trimStart());
          }
          if (data.length) this.dispatchEvent(new MessageEvent(name, { data: data.join('\n') }));
        }
        if (pending.length > 1024 * 1024) throw new Error('Stream frame too large');
      }
    } catch { /* Never surface a credential-bearing Request in an error. */ }
    if (!this.controller.signal.aborted) {
      this.readyState = AdminEventSource.CLOSED;
      this.dispatchEvent(new Event('error'));
    }
  }
}
