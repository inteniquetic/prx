import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app') as HTMLElement;

// The styleguide is a dev-only page: `import.meta.env.DEV` is replaced with a
// literal `false` in a production build, so this whole branch — and with it the
// dynamic import — is dropped before the bundle is written. Nothing of the
// styleguide reaches the binary.
if (import.meta.env.DEV && new URLSearchParams(location.search).has('styleguide')) {
  void import('./lib/styleguide/Styleguide.svelte').then(({ default: Styleguide }) => {
    mount(Styleguide, { target });
  });
} else {
  // Svelte 5 replaced `new App({ target })` with `mount()`; the old form throws
  // at runtime rather than failing the build.
  mount(App, { target });
}
