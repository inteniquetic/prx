<script lang="ts">
  // Credential lives only in memory and sessionStorage; never echoed.
  import { adminFetch, authScheme, setCredential } from '$lib/api/auth';
  import { t } from '$lib/i18n';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  let value = '';
  let busy = false;
  let failed = false;
  async function login() {
    busy = true;
    failed = false;
    try {
      setCredential(value, $authScheme);
      value = '';
      const response = await adminFetch('/web/config');
      if (response.ok) {
        window.location.reload();
        return;
      }
      failed = true;
      setCredential('');
    } catch {
      setCredential('');
      failed = true;
    } finally { busy = false; }
  }
</script>

<main class="flex min-h-screen items-center justify-center bg-background p-6 text-foreground">
  <form class="w-full max-w-sm space-y-4 rounded-xl border bg-card p-6" onsubmit={(event) => { event.preventDefault(); void login(); }}>
    <h1 class="text-xl font-semibold">{$t('auth.title')}</h1>
    <p class="text-sm text-muted-foreground">{$t('auth.description')}</p>
    <label class="block text-sm" for="admin-credential">{$authScheme === 'Basic' ? $t('auth.basic') : $t('auth.token')}</label>
    <Input id="admin-credential" type="password" autocomplete="off" bind:value required />
    {#if failed}<p class="text-sm text-destructive" role="alert">{$t('auth.failed')}</p>{/if}
    <Button type="submit" disabled={busy || !value}>{$t('auth.submit')}</Button>
  </form>
</main>
