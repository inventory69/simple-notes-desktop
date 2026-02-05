#!/usr/bin/env node
/**
 * Cross-platform build wrapper for Tauri
 * Sets WebKit environment variables on Linux for Wayland compatibility
 * Also handles AppImage icon naming fix for proper bundling
 */

import { spawn, execSync } from 'child_process';
import { platform } from 'os';
import { existsSync, copyFileSync } from 'fs';
import { join } from 'path';

// Linux-spezifische WebKit-Fixes für Wayland
if (platform() === 'linux') {
  // Fix für GBM/DMABUF Fehler unter Wayland
  process.env.WEBKIT_DISABLE_DMABUF_RENDERER = '1';
  // Alternativ für ältere WebKit-Versionen - wichtig für Wayland!
  process.env.WEBKIT_DISABLE_COMPOSITING_MODE = '1';
  
  console.log('[Build] Linux detected - WebKit environment variables set:');
  console.log('  WEBKIT_DISABLE_DMABUF_RENDERER=1');
  console.log('  WEBKIT_DISABLE_COMPOSITING_MODE=1');
}

// Starte tauri build
const child = spawn('pnpm', ['tauri', 'build'], {
  stdio: 'inherit',
  shell: false,
  env: process.env
});

child.on('close', (code) => {
  // Post-build: Fix AppImage icon naming if linuxdeploy failed
  if (platform() === 'linux' && code !== 0) {
    console.log('[Build] Checking if AppImage needs manual creation...');
    
    const appDir = join(process.cwd(), 'src-tauri/target/release/bundle/appimage/Simple Notes Desktop.AppDir');
    const appImage = join(process.cwd(), 'src-tauri/target/release/bundle/appimage/Simple Notes Desktop_0.1.0_amd64.AppImage');
    
    if (existsSync(appDir) && !existsSync(appImage)) {
      console.log('[Build] Creating AppImage manually...');
      
      // Fix icon naming
      const srcIcon = join(appDir, 'Simple Notes Desktop.png');
      const dstIcon = join(appDir, 'simple-notes-desktop.png');
      
      if (existsSync(srcIcon) && !existsSync(dstIcon)) {
        copyFileSync(srcIcon, dstIcon);
        console.log('[Build] Fixed icon naming');
      }
      
      // Try to create AppImage with appimagetool
      try {
        execSync(`ARCH=x86_64 appimagetool "${appDir}" "${appImage}"`, { 
          stdio: 'inherit',
          env: process.env 
        });
        console.log('[Build] AppImage created successfully!');
        process.exit(0);
      } catch (e) {
        console.error('[Build] Failed to create AppImage manually');
        process.exit(1);
      }
    }
  }
  
  process.exit(code ?? 0);
});
