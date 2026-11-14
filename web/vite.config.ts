// How the pages are built: one page, its script, its styles and its fonts, each a file
// of Away's own with its content's hash in its name. Nothing is written into the page
// itself and nothing is turned into a data: address, so the rule the server sends with
// every page, which allows files from Away's own address alone, holds for all of it.
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    assetsInlineLimit: 0,
    modulePreload: { polyfill: false },
    sourcemap: false,
  },
  // while the pages are worked on, what the server answers comes from a server that runs beside
  server: {
    proxy: Object.fromEntries(['/api', '/sign-in', '/sign-out'].map((path) => [path, 'http://127.0.0.1:8080'])),
  },
});
