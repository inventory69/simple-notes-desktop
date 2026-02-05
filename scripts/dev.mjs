#!/usr/bin/env node
/**
 * Cross-platform dev wrapper for Tauri
 * Sets WebKit environment variables on Linux for Wayland compatibility
 */

import { spawn } from 'child_process';
import { platform } from 'os';

// Linux-spezifische WebKit-Fixes für Wayland
if (platform() === 'linux') {
  // Fix für GBM/DMABUF Fehler unter Wayland
  process.env.WEBKIT_DISABLE_DMABUF_RENDERER = '1';
  // Alternativ für ältere WebKit-Versionen - wichtig für Wayland!
  process.env.WEBKIT_DISABLE_COMPOSITING_MODE = '1';
  
  console.log('[Dev] Linux detected - WebKit environment variables set:');
  console.log('  WEBKIT_DISABLE_DMABUF_RENDERER=1');
  console.log('  WEBKIT_DISABLE_COMPOSITING_MODE=1');
}

// Starte tauri dev
const child = spawn('pnpm', ['tauri', 'dev'], {
  stdio: 'inherit',
  shell: false,
  env: process.env
});

child.on('close', (code) => {
  process.exit(code ?? 0);
});
