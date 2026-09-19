import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url))
    }
  },
  build: {
    // The UI is embedded in the prx binary and served over a closed network,
    // so weight is a feature. Warn well before anything gets fat.
    chunkSizeWarningLimit: 300,
    rollupOptions: {
      output: {
        manualChunks(id) {
          // CodeMirror is the config editor's alone (T307). Naming the chunk
          // keeps it out of the entry bundle and lets scripts/bundle-budget.mjs
          // hold it to a budget of its own.
          if (id.includes('/node_modules/@codemirror/') || id.includes('/node_modules/@lezer/')) {
            return 'editor';
          }
          return undefined;
        }
      }
    }
  },
  server: {
    port: 5173,
    host: true
  }
});
