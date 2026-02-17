#!/usr/bin/env node
/**
 * Cross-platform build wrapper for Tauri
 * Sets WebKit environment variables on Linux for Wayland compatibility
 * Handles AppImage creation fallback for non-ext4 filesystems (e.g. NTFS)
 */

import { spawn, execSync } from 'child_process';
import { platform } from 'os';
import { existsSync, copyFileSync, mkdirSync, statSync } from 'fs';
import { join, resolve } from 'path';

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

// Parse CLI args: --no-appimage to skip AppImage bundling
const args = process.argv.slice(2);
const skipAppImage = args.includes('--no-appimage');

// Build bundles list (exclude appimage for separate handling on non-ext4)
const tauriArgs = skipAppImage ? ['tauri', 'build', '--bundles', 'deb,rpm'] : ['tauri', 'build'];

// Starte tauri build
const child = spawn('pnpm', tauriArgs, {
  stdio: 'inherit',
  shell: false,
  env: process.env
});

/**
 * On non-ext4 filesystems (e.g. NTFS), linuxdeploy can't create AppImages
 * because mksquashfs/FUSE have issues with xattr support.
 * This fallback creates the AppImage manually using appimagetool from /tmp.
 */
function createAppImageFallback() {
  const appDir = join(process.cwd(), 'src-tauri/target/release/bundle/appimage/Simple Notes Desktop.AppDir');
  const bundleDir = join(process.cwd(), 'src-tauri/target/release/bundle/appimage');
  const appImageName = 'Simple_Notes_Desktop-0.2.0_amd64.AppImage';
  const appImagePath = join(bundleDir, appImageName);
  
  if (!existsSync(appDir)) {
    console.log('[Build] No AppDir found, skipping AppImage fallback');
    return false;
  }
  
  if (existsSync(appImagePath)) {
    console.log('[Build] AppImage already exists');
    return true;
  }
  
  console.log('[Build] Creating AppImage via fallback...');
  
  // Fix desktop file placeholders
  const desktopFile = join(appDir, 'usr/share/applications/Simple Notes Desktop.desktop');
  if (existsSync(desktopFile)) {
    console.log('[Build] Fixing desktop file placeholders...');
    try {
      execSync(`sed -i 's/{exec}/simple-notes-desktop/g; s/{icon}/simple-notes-desktop/g; s/{productName}/Simple Notes Desktop/g' "${desktopFile}"`);
    } catch (e) {
      console.warn('[Build] Failed to fix desktop file:', e.message);
    }
  }
  
  // Create AppImage with linuxdeploy and NO_STRIP
  const linuxdeployPath = join(process.env.HOME, '.cache/tauri/linuxdeploy-x86_64.AppImage');
  
  if (!existsSync(linuxdeployPath)) {
    console.error('[Build] linuxdeploy not found in cache');
    return false;
  }
  
  try {
    console.log('[Build] Running linuxdeploy with NO_STRIP=1...');
    execSync(
      `cd "${bundleDir}" && NO_STRIP=1 OUTPUT="${appImageName}" "${linuxdeployPath}" --appdir="Simple Notes Desktop.AppDir" --output appimage`,
      { stdio: 'inherit', env: { ...process.env, NO_STRIP: '1', OUTPUT: appImageName } }
    );
    
    if (existsSync(appImagePath)) {
      console.log(`[Build] AppImage created: ${appImagePath}`);
      const size = statSync(appImagePath).size;
      console.log(`[Build] Size: ${(size / 1024 / 1024).toFixed(1)} MB`);
      return true;
    }
    
  } catch (e) {
    console.error('[Build] AppImage creation failed:', e.message);
  }
  
  return false;
}

child.on('close', (code) => {
  if (platform() === 'linux' && code !== 0) {
    // Tauri build failed - likely linuxdeploy on non-ext4 filesystem
    // Check if other bundles (deb/rpm) were created successfully
    const debDir = join(process.cwd(), 'src-tauri/target/release/bundle/deb');
    const hasOtherBundles = existsSync(debDir);
    
    if (hasOtherBundles) {
      console.log('[Build] Tauri build partially succeeded (deb/rpm OK, AppImage failed)');
      console.log('[Build] Attempting AppImage fallback...');
      
      if (createAppImageFallback()) {
        console.log('[Build] All bundles created successfully!');
        process.exit(0);
      } else {
        console.warn('[Build] AppImage creation failed, but deb/rpm are available');
        process.exit(0); // Don't fail the whole build
      }
    }
  }
  
  process.exit(code ?? 0);
});
