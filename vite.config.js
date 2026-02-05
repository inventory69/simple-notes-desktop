import { defineConfig } from 'vite'
import { resolve } from 'path'

// Tauri expects a fixed port, fail if that port is not available
export default defineConfig({
  clearScreen: false,
  
  // WICHTIG: Base-URL für Tauri - muss relativ sein!
  base: './',
  
  // Vite-Konfiguration für Dev-Server
  server: {
    port: 5173,
    strictPort: true,
    // Für Tauri-spezifische Sicherheit
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },

  // Environment Variables für Tauri
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  
  // Build-Konfiguration
  build: {
    // Tauri unterstützt es2021
    target: process.env.TAURI_ENV_PLATFORM === 'windows' 
      ? 'chrome105' 
      : 'safari14',
    // Debug builds oder production
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // Output-Verzeichnis
    outDir: 'dist',
    emptyOutDir: true,
  },

  // Resolve für Module
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
})
