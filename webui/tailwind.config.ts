import type { Config } from 'tailwindcss';

const config: Config = {
  content: ['./index.html', './src/**/*.{svelte,ts}'],
  theme: {
    extend: {
      colors: {
        ink: '#0d1b2a',
        fog: '#e0e1dd',
        tide: '#1b263b',
        aqua: '#00b4d8',
        mint: '#2ec4b6'
      },
      boxShadow: {
        panel: 'none'
      }
    }
  },
  plugins: []
};

export default config;
