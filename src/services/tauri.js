import { invoke } from '@tauri-apps/api/core';

/**
 * WebDAV Connection Service
 */
export async function connect(url, username, password) {
  return await invoke('connect', { url, username, password });
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
 * @returns {Promise<Object>} Note object
 */
export async function getNote(id) {
  return await invoke('get_note', { id });
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
 */
export async function deleteNote(id) {
  return await invoke('delete_note', { id });
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
