import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'es2022',
    assetsInlineLimit: 0,
  },
});
