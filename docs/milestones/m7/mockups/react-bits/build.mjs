// Build only this standalone design preview; uses already-installed project dependencies.
import { build } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('.', import.meta.url));
await build({
  configFile: false,
  root,
  plugins: [react()],
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  build: {
    outDir: 'bundle',
    emptyOutDir: false,
    lib: { entry: root + 'mount.tsx', name: 'ReviaGlassPreview', formats: ['iife'], fileName: () => 'preview.js', cssFileName: 'preview' },
  },
});
