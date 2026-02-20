import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauri from '../services/tauri.js';

vi.mock('../services/tauri.js');
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

// Minimal DOM setup for SettingsDialog
function setupDOM() {
  document.body.innerHTML = `
    <div id="settings-dialog" class="dialog hidden">
      <div class="dialog-content">
        <select id="theme-select">
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
        <input type="checkbox" id="autosave-checkbox" />
        <input type="checkbox" id="tray-checkbox" />
        <input type="checkbox" id="autostart-checkbox" />
        <input type="text" id="device-id" readonly />
        <span id="app-version">Loading...</span>
        <a href="#" id="github-link">GitHub</a>
        <button id="save-settings-btn">Save</button>
        <button id="cancel-settings-btn">Cancel</button>
      </div>
    </div>
  `;
}

describe('SettingsDialog', () => {
  let SettingsDialog;

  beforeEach(async () => {
    setupDOM();
    vi.clearAllMocks();

    // Default mocks
    tauri.getAppVersion.mockResolvedValue('0.1.0');
    tauri.getSettings.mockResolvedValue({
      theme: 'system',
      autosave: true,
      minimize_to_tray: false,
      autostart: false,
    });
    tauri.getDeviceId.mockResolvedValue('tauri-abc123');
    tauri.saveSettings.mockResolvedValue();
    tauri.updateTraySetting.mockResolvedValue();

    // Dynamic import to get fresh module with fresh DOM
    const mod = await import('../components/SettingsDialog.js');
    SettingsDialog = mod.SettingsDialog;
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  describe('constructor', () => {
    it('should find all DOM elements', () => {
      const dialog = new SettingsDialog();
      expect(dialog.dialog).toBeTruthy();
      expect(dialog.themeSelect).toBeTruthy();
      expect(dialog.autosaveCheckbox).toBeTruthy();
      expect(dialog.trayCheckbox).toBeTruthy();
      expect(dialog.autostartCheckbox).toBeTruthy();
      expect(dialog.deviceIdInput).toBeTruthy();
    });

    it('should load app version on init', async () => {
      new SettingsDialog();
      // Wait for async loadAppVersion
      await vi.waitFor(() => {
        expect(tauri.getAppVersion).toHaveBeenCalledOnce();
      });
    });
  });

  describe('show()', () => {
    it('should load settings and populate form', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'dark',
        autosave: false,
        minimize_to_tray: true,
        autostart: true,
      });
      tauri.getDeviceId.mockResolvedValue('device-xyz');

      const dialog = new SettingsDialog();
      await dialog.show();

      expect(dialog.themeSelect.value).toBe('dark');
      expect(dialog.autosaveCheckbox.checked).toBe(false);
      expect(dialog.trayCheckbox.checked).toBe(true);
      expect(dialog.autostartCheckbox.checked).toBe(true);
      expect(dialog.deviceIdInput.value).toBe('device-xyz');
    });

    it('should show the dialog', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();

      expect(dialog.dialog.classList.contains('hidden')).toBe(false);
    });

    it('should store original theme for cancel', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'dark',
        autosave: true,
        minimize_to_tray: false,
        autostart: false,
      });

      const dialog = new SettingsDialog();
      await dialog.show();

      expect(dialog.originalTheme).toBe('dark');
    });

    it('should handle missing minimize_to_tray gracefully (old settings)', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'system',
        autosave: true,
        // minimize_to_tray and autostart missing (old version)
      });

      const dialog = new SettingsDialog();
      await dialog.show();

      expect(dialog.trayCheckbox.checked).toBe(false);
      expect(dialog.autostartCheckbox.checked).toBe(false);
    });
  });

  describe('handleSave()', () => {
    it('should save all settings including tray and autostart', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();

      // Change settings
      dialog.themeSelect.value = 'dark';
      dialog.autosaveCheckbox.checked = false;
      dialog.trayCheckbox.checked = true;
      dialog.autostartCheckbox.checked = true;

      await dialog.handleSave();

      expect(tauri.saveSettings).toHaveBeenCalledWith({
        theme: 'dark',
        autosave: false,
        minimize_to_tray: true,
        autostart: true,
      });
    });

    it('should update tray runtime setting after saving', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();

      dialog.trayCheckbox.checked = true;
      await dialog.handleSave();

      expect(tauri.updateTraySetting).toHaveBeenCalledWith(true);
    });

    it('should call onSave callback with new settings', async () => {
      const callback = vi.fn();
      const dialog = new SettingsDialog();
      dialog.onSave(callback);
      await dialog.show();

      await dialog.handleSave();

      expect(callback).toHaveBeenCalledWith(
        expect.objectContaining({
          theme: 'system',
          autosave: true,
          minimize_to_tray: false,
          autostart: false,
        }),
      );
    });

    it('should hide dialog after save', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();
      await dialog.handleSave();

      expect(dialog.dialog.classList.contains('hidden')).toBe(true);
    });
  });

  describe('handleCancel()', () => {
    it('should restore original theme on cancel', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'light',
        autosave: true,
        minimize_to_tray: false,
        autostart: false,
      });

      const dialog = new SettingsDialog();
      await dialog.show();

      // User changes theme but cancels
      dialog.themeSelect.value = 'dark';
      dialog.handleCancel();

      // Theme should be reverted to 'light'
      // (applyTheme removes data-theme for light)
      expect(document.documentElement.getAttribute('data-theme')).toBeNull();
    });

    it('should hide dialog on cancel', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();
      dialog.handleCancel();

      expect(dialog.dialog.classList.contains('hidden')).toBe(true);
    });

    it('should not call saveSettings on cancel', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();
      dialog.handleCancel();

      // saveSettings is NOT called during cancel
      // (it might be called during init loadAppVersion, but not for settings)
      expect(tauri.saveSettings).not.toHaveBeenCalled();
    });
  });

  describe('applyTheme()', () => {
    it('should set dark theme', () => {
      const dialog = new SettingsDialog();
      dialog.applyTheme('dark');
      expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    });

    it('should set light theme by removing attribute', () => {
      document.documentElement.setAttribute('data-theme', 'dark');
      const dialog = new SettingsDialog();
      dialog.applyTheme('light');
      expect(document.documentElement.getAttribute('data-theme')).toBeNull();
    });

    it('should use system preference for system theme', () => {
      const dialog = new SettingsDialog();
      dialog.applyTheme('system');
      // matchMedia is mocked to return false for prefers-color-scheme: dark
      expect(document.documentElement.getAttribute('data-theme')).toBeNull();
    });
  });
});
