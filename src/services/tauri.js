import { invoke } from '@tauri-apps/api/core';

/**
 * WebDAV Connection Service
 * @param {string} url - WebDAV server URL
 * @param {string} username - Username
 * @param {string} password - Password
 * @param {string|null} syncFolder - Sync folder name (default: "notes")
 */
export async function connect(url, username, password, syncFolder = null) {
  return await invoke('connect', { url, username, password, syncFolder });
}

/**
 * List all notes from WebDAV server
 * @returns {Promise<Array>} Array of note metadata objects
 */
export async function listNotes() {
  return await invoke('list_notes');
}

/**
 * Get a specific note by ID
 * @param {string} id - Note ID
 * @param {string|null} folderName - Folder name (null = root)
 * @returns {Promise<Object>} Note object
 */
export async function getNote(id, folderName = null) {
  return await invoke('get_note', { id, folderName });
}

/**
 * Save a note to WebDAV server
 * @param {Object} note - Note object to save
 * @returns {Promise<Object>} Updated note with new timestamp
 */
export async function saveNote(note) {
  return await invoke('save_note', { note });
}

/**
 * Create a new note
 * @param {string} title - Note title
 * @param {string} noteType - Note type: "TEXT" or "CHECKLIST"
 * @returns {Promise<Object>} New note object
 */
export async function createNote(title, noteType) {
  return await invoke('create_note', { title, noteType });
}

/**
 * Delete a note
 * @param {string} id - Note ID to delete
 * @param {string|null} folderName - Folder name (null = root)
 */
export async function deleteNote(id, folderName = null) {
  return await invoke('delete_note', { id, folderName });
}

/**
 * Get stored credentials
 * @returns {Promise<Object|null>} Credentials object or null
 */
export async function getCredentials() {
  return await invoke('get_credentials');
}

/**
 * Save credentials
 * @param {Object} credentials - Credentials object {url, username, password}
 */
export async function saveCredentials(credentials) {
  return await invoke('save_credentials', { credentials });
}

/**
 * Clear stored credentials
 */
export async function clearCredentials() {
  return await invoke('clear_credentials');
}

/**
 * Get device ID
 * @returns {Promise<string>} Device ID
 */
export async function getDeviceId() {
  return await invoke('get_device_id');
}

/**
 * Get settings
 * @returns {Promise<Object>} Settings object
 */
export async function getSettings() {
  return await invoke('get_settings');
}

/**
 * Save settings
 * @param {Object} settings - Settings object {theme, autosave}
 */
export async function saveSettings(settings) {
  return await invoke('save_settings', { settings });
}

/**
 * Get app version
 * @returns {Promise<string>} App version string
 */
export async function getAppVersion() {
  return await invoke('get_app_version');
}

/**
 * Get desktop environment
 * @returns {Promise<string|null>} Desktop environment name (kde, gnome, etc.) or null
 */
export async function getDesktopEnvironment() {
  return await invoke('get_desktop_environment');
}

/**
 * Update the minimize-to-tray runtime setting
 * @param {boolean} enabled - Whether minimize-to-tray is enabled
 */
export async function updateTraySetting(enabled) {
  return await invoke('update_tray_setting', { enabled });
}

/**
 * Pin or unpin multiple notes
 * @param {string[]} ids - Array of note IDs
 * @param {boolean} pinned - true = pin, false = unpin
 * @param {string|null} folderName - Current folder context (null = root)
 */
export async function pinNotes(ids, pinned, folderName = null) {
  return await invoke('pin_notes', { ids, pinned, folderName });
}

/**
 * Set or remove the background color of multiple notes
 * @param {string[]} ids - Array of note IDs
 * @param {string|null} color - Hex color string (e.g. "#F28B82") or null to remove
 * @param {string|null} folderName - Current folder context (null = root)
 */
export async function colorNotes(ids, color, folderName = null) {
  return await invoke('color_notes', { ids, color: color ?? null, folderName });
}

/**
 * List all folders
 * @returns {Promise<Array<{name: string, color: string|null}>>}
 */
export async function listFolders() {
  return await invoke('list_folders');
}

/**
 * Create a new folder
 * @param {string} name - Folder name
 * @param {string|null} color - Optional hex color
 * @returns {Promise<Array>} Updated folder list
 */
export async function createFolder(name, color = null) {
  return await invoke('create_folder', { name, color });
}

/**
 * Rename a folder (moves all contained notes)
 * @param {string} oldName
 * @param {string} newName
 * @returns {Promise<Array>} Updated folder list
 */
export async function renameFolder(oldName, newName) {
  return await invoke('rename_folder', { oldName, newName });
}

/**
 * Delete a folder
 * @param {string} name - Folder name
 * @param {boolean} keepNotes - true = move notes to root; false = delete notes too
 * @returns {Promise<Array>} Updated folder list
 */
export async function deleteFolder(name, keepNotes) {
  return await invoke('delete_folder', { name, keepNotes });
}

/**
 * Set or remove folder color
 * @param {string} name - Folder name
 * @param {string|null} color - Hex color or null
 * @returns {Promise<Array>} Updated folder list
 */
export async function setFolderColor(name, color) {
  return await invoke('set_folder_color', { name, color: color ?? null });
}

/**
 * Move notes to a different folder
 * @param {string[]} ids - Note IDs to move
 * @param {string|null} sourceFolder - Current folder (null = root)
 * @param {string|null} targetFolder - Target folder (null = root)
 */
export async function moveNotes(ids, sourceFolder, targetFolder) {
  return await invoke('move_notes', {
    ids,
    sourceFolder: sourceFolder ?? null,
    targetFolder: targetFolder ?? null,
  });
}

/**
 * Get the current operating system platform
 * @returns {Promise<'windows'|'linux'|'macos'|'unknown'>}
 */
export async function getPlatform() {
  return await invoke('get_platform');
}

/**
 * Check whether an in-app update is available (Windows only).
 * Returns the new version string if an update is available, or null if up-to-date.
 * On Linux/macOS always returns null — updates are handled by the package manager.
 * @returns {Promise<string|null>} New version string or null
 */
export async function checkForUpdates() {
  return await invoke('check_for_updates');
}

/**
 * Download and install the available update (Windows only).
 * The app will exit after a successful install; the NSIS installer
 * starts the new version automatically.
 * @returns {Promise<void>}
 */
export async function installUpdate() {
  return await invoke('install_update');
}
