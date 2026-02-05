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
  const appImageName = 'simple-notes-desktop_0.1.0_amd64.AppImage';
  const appImagePath = join(bundleDir, appImageName);
  
  if (!existsSync(appDir)) {
    console.log('[Build] No AppDir found, skipping AppImage fallback');
    return false;
  }
  
  if (existsSync(appImagePath)) {
    console.log('[Build] AppImage already exists');
    return true;
  }
  
  console.log('[Build] Creating AppImage via fallback (non-ext4 filesystem detected)...');
  
  // Use /tmp for AppImage creation to avoid NTFS issues
  const tmpDir = '/tmp/tauri-appimage-build';
  try {
    execSync(`rm -rf "${tmpDir}" && mkdir -p "${tmpDir}"`);
    
    // Copy AppDir to tmp
    execSync(`cp -a "${appDir}" "${tmpDir}/app.AppDir"`, { stdio: 'inherit' });
    
    // Create AppImage from tmp using appimagetool or linuxdeploy
    const linuxdeployPath = join(process.env.HOME, '.cache/tauri/linuxdeploy-x86_64.AppImage');
    const tmpAppImage = join(tmpDir, appImageName);
    
    if (existsSync(linuxdeployPath)) {
      execSync(
        `cd "${tmpDir}" && NO_STRIP=true ARCH=x86_64 "${linuxdeployPath}" --appimage-extract-and-run --appdir app.AppDir --plugin gtk --output appimage`,
        { stdio: 'inherit', env: { ...process.env, OUTPUT: tmpAppImage, NO_STRIP: 'true', ARCH: 'x86_64' } }
      );
    } else {
      // Fallback to appimagetool if available
      execSync(
        `cd "${tmpDir}" && ARCH=x86_64 appimagetool app.AppDir "${tmpAppImage}"`,
        { stdio: 'inherit', env: { ...process.env, ARCH: 'x86_64' } }
      );
    }
    
    // Copy result back
    if (existsSync(tmpAppImage)) {
      execSync(`cp "${tmpAppImage}" "${appImagePath}"`);
      console.log(`[Build] AppImage created: ${appImagePath}`);
      const size = statSync(appImagePath).size;
      console.log(`[Build] Size: ${(size / 1024 / 1024).toFixed(1)} MB`);
      return true;
    }
    
    // linuxdeploy might have output with different name, find any .AppImage
    const findResult = execSync(`find "${tmpDir}" -name "*.AppImage" -type f 2>/dev/null`).toString().trim();
    if (findResult) {
      const firstResult = findResult.split('\n')[0];
      execSync(`cp "${firstResult}" "${appImagePath}"`);
      console.log(`[Build] AppImage created: ${appImagePath}`);
      return true;
    }
    
  } catch (e) {
    console.error('[Build] AppImage fallback failed:', e.message);
  } finally {
    execSync(`rm -rf "${tmpDir}" 2>/dev/null || true`);
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
