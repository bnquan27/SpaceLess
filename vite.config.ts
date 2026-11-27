import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  root: 'src',
  envDir: '..',
  publicDir: '../public',
  build: { outDir: '../dist', emptyOutDir: true },
  plugins: [react(), tailwindcss()],
  resolve: { preserveSymlinks: true, alias: [{ find: /^@\//, replacement: '/' }] },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/docs/qa/**'] },
  },
  clearScreen: false,
})
