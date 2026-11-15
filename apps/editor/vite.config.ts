// Vite config for the editor frontend. The same build runs in a browser and inside Tauri.
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri reads the dev server at this fixed port (see src-tauri/tauri.conf.json).
  // The repo root is allowed so the shared brand/ folder and examples/ can be imported.
  server: { port: 5183, strictPort: true, fs: { allow: ['../..'] } },
  clearScreen: false,
  build: { target: 'es2023' },
});
