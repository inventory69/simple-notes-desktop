import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dialogService } from '../services/DialogService.js';
import * as tauri from '../services/tauri.js';

vi.mock('../services/tauri.js');
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));
vi.mock('../services/DialogService.js', () => ({
  dialogService: {
    error: vi.fn().mockResolvedValue(undefined),
    info: vi.fn().mockResolvedValue(undefined),
    alert: vi.fn().mockResolvedValue(undefined),
    success: vi.fn().mockResolvedValue(undefined),
  },
}));

// Minimal DOM setup for SettingsDialog
function setupDOM() {
  document.body.innerHTML = `
    <div id="settings-dialog" class="dialog hidden">
      <div class="dialog-content">
        <button id="settings-back-btn" class="hidden"></button>
        <div id="theme-grid"></div>
        <div id="settings-home">
          <button class="settings-nav-card" data-section="appearance" type="button">Appearance</button>
          <button class="settings-nav-card" data-section="connection" type="button">Connection</button>
          <button class="settings-nav-card" data-section="system" type="button">System</button>
          <button id="updates-card" class="settings-nav-card hidden" data-section="updates" type="button">Updates</button>
          <button class="settings-nav-card" data-section="about" type="button">About</button>
        </div>
        <div class="settings-section hidden" data-section="appearance">
          <select id="default-open-mode-select">
            <option value="edit">Edit mode</option>
            <option value="preview">Preview</option>
          </select>
          <input type="checkbox" id="autosave-checkbox" />
        </div>
        <div class="settings-section hidden" data-section="connection">
          <input type="checkbox" id="offline-mode-checkbox" />
          <input type="text" id="settings-server-url" />
          <input type="text" id="settings-username" />
          <input type="password" id="settings-password" />
          <button id="test-connection-btn" class="btn-secondary" type="button">Test connection</button>
          <span id="connection-status"></span>
          <input type="text" id="sync-folder-input" placeholder="notes" maxlength="50" />
        </div>
        <div class="settings-section hidden" data-section="system">
          <input type="checkbox" id="tray-checkbox" />
          <input type="checkbox" id="autostart-checkbox" />
          <input type="text" id="device-id" readonly />
        </div>
        <div id="updates-section" class="settings-section hidden" data-section="updates">
          <input type="checkbox" id="update-notifications-checkbox" />
          <button id="check-updates-btn">Check for Updates</button>
          <span id="update-status" class="hidden"></span>
          <button id="install-update-btn" class="hidden">Install Update</button>
        </div>
        <div class="settings-section hidden" data-section="about">
          <span id="app-version">Loading...</span>
          <a href="#" id="github-link">GitHub</a>
        </div>
        <div class="chip-selector" id="font-size-chips">
          <button class="chip" data-value="small" type="button"><span class="chip-preview">Aa</span><span>Small</span></button>
          <button class="chip" data-value="system" type="button"><span class="chip-preview">Aa</span><span>System</span></button>
          <button class="chip" data-value="normal" type="button"><span class="chip-preview">Aa</span><span>Normal</span></button>
          <button class="chip" data-value="large" type="button"><span class="chip-preview">Aa</span><span>Large</span></button>
          <button class="chip" data-value="xlarge" type="button"><span class="chip-preview">Aa</span><span>XLarge</span></button>
        </div>
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
      sync_folder: 'notes',
      update_notifications: true,
      default_open_mode: 'edit',
      font_size: 'system',
      offline_mode: true,
    });
    tauri.getDeviceId.mockResolvedValue('tauri-abc123');
    tauri.getCredentials.mockResolvedValue(null);
    tauri.saveSettings.mockResolvedValue();
    tauri.saveCredentials.mockResolvedValue();
    tauri.updateTraySetting.mockResolvedValue();
    tauri.disconnect.mockResolvedValue();
    tauri.connect.mockResolvedValue(true);
    tauri.testConnection.mockResolvedValue(true);
    tauri.getPlatform.mockResolvedValue('linux');
    tauri.checkForUpdates.mockResolvedValue(null);
    tauri.installUpdate.mockResolvedValue();

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
      expect(dialog.themeGrid).toBeTruthy();
      expect(dialog.autosaveCheckbox).toBeTruthy();
      expect(dialog.trayCheckbox).toBeTruthy();
      expect(dialog.autostartCheckbox).toBeTruthy();
      expect(dialog.deviceIdInput).toBeTruthy();
    });

    it('should render theme cards in the grid', () => {
      const dialog = new SettingsDialog();
      const cards = dialog.themeGrid.querySelectorAll('.theme-card');
      expect(cards.length).toBeGreaterThan(0);
      const ids = [...cards].map((c) => c.dataset.themeId);
      expect(ids).toContain('system');
      expect(ids).toContain('catppuccin-macchiato');
      expect(ids).toContain('nord');
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
        sync_folder: 'my-sync',
      });
      tauri.getDeviceId.mockResolvedValue('device-xyz');

      const dialog = new SettingsDialog();
      await dialog.show();

      const darkCard = dialog.themeGrid.querySelector('[data-theme-id="dark"]');
      expect(darkCard.getAttribute('aria-pressed')).toBe('true');
      expect(dialog._currentTheme).toBe('dark');
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
        sync_folder: 'notes',
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

  describe('_applyOfflineState()', () => {
    it('should disable server inputs when offline', async () => {
      const dialog = new SettingsDialog();
      await dialog.show(); // offline_mode: true from default mock
      expect(dialog.serverUrlInput.disabled).toBe(true);
      expect(dialog.serverUsernameInput.disabled).toBe(true);
      expect(dialog.serverPasswordInput.disabled).toBe(true);
    });

    it('should set status label to Offline when offline', async () => {
      const dialog = new SettingsDialog();
      await dialog.show(); // offline_mode: true
      expect(dialog.connectionStatus.textContent).toBe('Status: Offline');
    });
  });

  describe('_testConnection()', () => {
    it('should show error when server fields are empty', async () => {
      const dialog = new SettingsDialog();
      await dialog._testConnection();
      expect(dialogService.error).toHaveBeenCalled();
      expect(tauri.connect).not.toHaveBeenCalled();
    });

    it('should test (not connect) and show info on success, without side effects', async () => {
      const dialog = new SettingsDialog();
      await dialog.show(); // offline_mode: true
      dialog.serverUrlInput.value = 'http://test.local';
      dialog.serverUsernameInput.value = 'admin';
      dialog.serverPasswordInput.value = 'pw';
      await dialog._testConnection();
      expect(tauri.testConnection).toHaveBeenCalledWith('http://test.local', 'admin', 'pw', 'notes');
      expect(dialogService.info).toHaveBeenCalled();
      expect(dialog.connectionStatus.textContent).toBe('Status: Reachable');
      // test_connection has no side effects: never stores a client, never disconnects
      expect(tauri.connect).not.toHaveBeenCalled();
      expect(tauri.disconnect).not.toHaveBeenCalled();
    });

    it('should show error dialog when test returns false', async () => {
      tauri.testConnection.mockResolvedValue(false);
      const dialog = new SettingsDialog();
      dialog.serverUrlInput.value = 'http://test.local';
      dialog.serverUsernameInput.value = 'admin';
      dialog.serverPasswordInput.value = 'pw';
      await dialog._testConnection();
      expect(dialogService.error).toHaveBeenCalled();
    });
  });

  describe('handleSave()', () => {
    it('should save credentials whenever server fields are filled, even if offline', async () => {
      const dialog = new SettingsDialog();
      await dialog.show(); // offline_mode: true
      dialog.serverUrlInput.value = 'http://test.local';
      dialog.serverUsernameInput.value = 'admin';
      dialog.serverPasswordInput.value = 'pw';
      await dialog.handleSave();
      expect(tauri.saveCredentials).toHaveBeenCalledWith({
        url: 'http://test.local',
        username: 'admin',
        password: 'pw',
      });
    });

    it('should NOT save credentials when server fields are empty', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();
      await dialog.handleSave();
      expect(tauri.saveCredentials).not.toHaveBeenCalled();
    });

    it('should save all settings including tray and autostart', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();

      // Change settings
      dialog.selectTheme('dark');
      dialog.autosaveCheckbox.checked = false;
      dialog.trayCheckbox.checked = true;
      dialog.autostartCheckbox.checked = true;

      await dialog.handleSave();

      expect(tauri.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          theme: 'dark',
          autosave: false,
          minimize_to_tray: true,
          autostart: true,
          sync_folder: 'notes',
          offline_mode: true,
        }),
      );
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
          offline_mode: true,
        }),
      );
    });

    it('should hide dialog after save', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();
      await dialog.handleSave();

      expect(dialog.dialog.classList.contains('hidden')).toBe(true);
    });

    it('should call disconnect when toggling from online to offline', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'system',
        autosave: true,
        minimize_to_tray: false,
        autostart: false,
        sync_folder: 'notes',
        update_notifications: true,
        default_open_mode: 'edit',
        font_size: 'system',
        offline_mode: false, // starts online
      });
      const dialog = new SettingsDialog();
      await dialog.show();
      dialog.offlineCheckbox.checked = true; // user toggles to offline
      await dialog.handleSave();

      expect(tauri.disconnect).toHaveBeenCalled();
    });

    it('should include offline_mode in saved settings', async () => {
      const dialog = new SettingsDialog();
      await dialog.show();
      await dialog.handleSave();

      expect(tauri.saveSettings).toHaveBeenCalledWith(expect.objectContaining({ offline_mode: true }));
    });
  });

  describe('handleCancel()', () => {
    it('should restore original theme on cancel', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'light',
        autosave: true,
        minimize_to_tray: false,
        autostart: false,
        sync_folder: 'notes',
      });

      const dialog = new SettingsDialog();
      await dialog.show();

      // User changes theme but cancels
      dialog.selectTheme('dark');
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
    it('should set dark theme and data-mode', () => {
      const dialog = new SettingsDialog();
      dialog.applyTheme('dark');
      expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
      expect(document.documentElement.getAttribute('data-mode')).toBe('dark');
    });

    it('should set light theme by removing data-theme, mode=light', () => {
      document.documentElement.setAttribute('data-theme', 'dark');
      const dialog = new SettingsDialog();
      dialog.applyTheme('light');
      expect(document.documentElement.getAttribute('data-theme')).toBeNull();
      expect(document.documentElement.getAttribute('data-mode')).toBe('light');
    });

    it('should use system preference for system theme (light OS)', () => {
      const dialog = new SettingsDialog();
      dialog.applyTheme('system');
      // matchMedia is mocked to return false for prefers-color-scheme: dark
      expect(document.documentElement.getAttribute('data-theme')).toBeNull();
      expect(document.documentElement.getAttribute('data-mode')).toBe('light');
    });

    it('should apply catppuccin-macchiato as dark mode', () => {
      const dialog = new SettingsDialog();
      dialog.applyTheme('catppuccin-macchiato');
      expect(document.documentElement.getAttribute('data-theme')).toBe('catppuccin-macchiato');
      expect(document.documentElement.getAttribute('data-mode')).toBe('dark');
    });

    it('should fall back to light for unknown theme', () => {
      const dialog = new SettingsDialog();
      dialog.applyTheme('unknown-theme-xyz');
      expect(document.documentElement.getAttribute('data-theme')).toBeNull();
      expect(document.documentElement.getAttribute('data-mode')).toBe('light');
    });
  });

  describe('selectTheme()', () => {
    it('should mark the selected card with aria-pressed', () => {
      const dialog = new SettingsDialog();
      dialog.selectTheme('nord');
      const nordCard = dialog.themeGrid.querySelector('[data-theme-id="nord"]');
      const otherCard = dialog.themeGrid.querySelector('[data-theme-id="dark"]');
      expect(nordCard.getAttribute('aria-pressed')).toBe('true');
      expect(otherCard.getAttribute('aria-pressed')).toBe('false');
    });

    it('should restore selected card on cancel', async () => {
      tauri.getSettings.mockResolvedValue({
        theme: 'catppuccin-mocha',
        autosave: true,
        minimize_to_tray: false,
        autostart: false,
        sync_folder: 'notes',
      });
      const dialog = new SettingsDialog();
      await dialog.show();

      dialog.selectTheme('nord');
      dialog.handleCancel();

      const mochaCard = dialog.themeGrid.querySelector('[data-theme-id="catppuccin-mocha"]');
      expect(mochaCard.getAttribute('aria-pressed')).toBe('true');
    });
  });
});
