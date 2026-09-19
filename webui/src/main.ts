import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';

// Svelte 5 replaced `new App({ target })` with `mount()`; the old form throws
// at runtime rather than failing the build.
const app = mount(App, {
  target: document.getElementById('app') as HTMLElement
});

export default app;
