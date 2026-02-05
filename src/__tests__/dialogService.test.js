import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('DialogService', () => {
  let dialogService;

  beforeEach(async () => {
    // Clean DOM
    document.body.innerHTML = '';
    
    // Fresh import each time
    vi.resetModules();
    const mod = await import('../services/DialogService.js');
    dialogService = mod.dialogService;
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  describe('initialization', () => {
    it('should create the dialog element in DOM', () => {
      const dialog = document.getElementById('universal-dialog');
      expect(dialog).toBeTruthy();
    });

    it('should have all required elements', () => {
      expect(document.getElementById('dialog-title')).toBeTruthy();
      expect(document.getElementById('dialog-message')).toBeTruthy();
      expect(document.getElementById('dialog-confirm-btn')).toBeTruthy();
      expect(document.getElementById('dialog-cancel-btn')).toBeTruthy();
      expect(document.getElementById('dialog-icon')).toBeTruthy();
    });

    it('should start hidden', () => {
      const dialog = document.getElementById('universal-dialog');
      expect(dialog.classList.contains('hidden')).toBe(true);
    });
  });

  describe('confirm()', () => {
    it('should show dialog with correct title and message', async () => {
      const promise = dialogService.confirm({
        title: 'Delete Note',
        message: 'Are you sure?',
        confirmText: 'Delete',
        cancelText: 'Cancel',
        type: 'danger',
      });

      expect(document.getElementById('dialog-title').textContent).toBe('Delete Note');
      expect(document.getElementById('dialog-message').textContent).toBe('Are you sure?');
      expect(document.getElementById('dialog-confirm-btn').textContent).toBe('Delete');
      expect(document.getElementById('dialog-cancel-btn').textContent).toBe('Cancel');

      // Dialog should be visible
      const dialog = document.getElementById('universal-dialog');
      expect(dialog.classList.contains('hidden')).toBe(false);

      // Click confirm
      document.getElementById('dialog-confirm-btn').click();
      const result = await promise;
      expect(result).toBe(true);
    });

    it('should return false when cancelled', async () => {
      const promise = dialogService.confirm({
        title: 'Test',
        message: 'Test message',
      });

      document.getElementById('dialog-cancel-btn').click();
      const result = await promise;
      expect(result).toBe(false);
    });

    it('should hide dialog after confirm', async () => {
      const promise = dialogService.confirm({
        title: 'Test',
        message: 'Test',
      });

      document.getElementById('dialog-confirm-btn').click();
      await promise;

      const dialog = document.getElementById('universal-dialog');
      expect(dialog.classList.contains('hidden')).toBe(true);
    });

    it('should show cancel button for confirm dialog', () => {
      dialogService.confirm({
        title: 'Test',
        message: 'Test',
      });

      const cancelBtn = document.getElementById('dialog-cancel-btn');
      expect(cancelBtn.style.display).not.toBe('none');
    });

    it('should apply danger button class for danger type', () => {
      dialogService.confirm({
        title: 'Delete',
        message: 'Sure?',
        type: 'danger',
      });

      const confirmBtn = document.getElementById('dialog-confirm-btn');
      expect(confirmBtn.className).toBe('btn-danger');
    });
  });

  describe('alert()', () => {
    it('should hide cancel button', () => {
      dialogService.alert({
        title: 'Info',
        message: 'Something happened',
      });

      const cancelBtn = document.getElementById('dialog-cancel-btn');
      expect(cancelBtn.style.display).toBe('none');
    });

    it('should return true when confirmed', async () => {
      const promise = dialogService.alert({
        title: 'Info',
        message: 'OK',
      });

      document.getElementById('dialog-confirm-btn').click();
      const result = await promise;
      expect(result).toBe(true);
    });

    it('should use custom button text', () => {
      dialogService.alert({
        title: 'Test',
        message: 'Test',
        buttonText: 'Got it',
      });

      expect(document.getElementById('dialog-confirm-btn').textContent).toBe('Got it');
    });
  });

  describe('shorthand methods', () => {
    it('error() should set title to Error', () => {
      dialogService.error({ message: 'Oops' });
      expect(document.getElementById('dialog-title').textContent).toBe('Error');
    });

    it('success() should set title to Success', () => {
      dialogService.success({ message: 'Done' });
      expect(document.getElementById('dialog-title').textContent).toBe('Success');
    });

    it('warning() should set title to Warning', () => {
      dialogService.warning({ message: 'Watch out' });
      expect(document.getElementById('dialog-title').textContent).toBe('Warning');
    });

    it('info() should set title to Information', () => {
      dialogService.info({ message: 'FYI' });
      expect(document.getElementById('dialog-title').textContent).toBe('Information');
    });

    it('error() should apply btn-danger class', () => {
      dialogService.error({ message: 'Oops' });
      expect(document.getElementById('dialog-confirm-btn').className).toBe('btn-danger');
    });

    it('success() should apply btn-success class', () => {
      dialogService.success({ message: 'Done' });
      expect(document.getElementById('dialog-confirm-btn').className).toBe('btn-success');
    });

    it('warning() should apply btn-warning class', () => {
      dialogService.warning({ message: 'Watch out' });
      expect(document.getElementById('dialog-confirm-btn').className).toBe('btn-warning');
    });
  });

  describe('prompt()', () => {
    it('should show input field', () => {
      dialogService.prompt({
        title: 'New Note',
        message: 'Enter name:',
        placeholder: 'Note title...',
      });

      const input = document.getElementById('dialog-input');
      expect(input).toBeTruthy();
      expect(input.placeholder).toBe('Note title...');
    });

    it('should return input value on confirm', async () => {
      const promise = dialogService.prompt({
        title: 'New Note',
        message: 'Enter name:',
      });

      const input = document.getElementById('dialog-input');
      // Simulate user typing
      input.value = 'My New Note';

      document.getElementById('dialog-confirm-btn').click();
      const result = await promise;
      expect(result).toBe('My New Note');
    });

    it('should return null on cancel', async () => {
      const promise = dialogService.prompt({
        title: 'New Note',
        message: 'Enter name:',
      });

      document.getElementById('dialog-cancel-btn').click();
      const result = await promise;
      expect(result).toBeNull();
    });

    it('should return null for empty input', async () => {
      const promise = dialogService.prompt({
        title: 'New Note',
        message: 'Enter name:',
      });

      document.getElementById('dialog-input').value = '   ';
      document.getElementById('dialog-confirm-btn').click();
      const result = await promise;
      expect(result).toBeNull();
    });

    it('should pre-fill default value', () => {
      dialogService.prompt({
        title: 'Rename',
        message: 'New name:',
        defaultValue: 'Old Name',
      });

      const input = document.getElementById('dialog-input');
      expect(input.value).toBe('Old Name');
    });
  });

  describe('keyboard handling', () => {
    it('should close confirm dialog on Escape', async () => {
      const promise = dialogService.confirm({
        title: 'Test',
        message: 'Test',
      });

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      const result = await promise;
      expect(result).toBe(false);
    });

    it('should confirm on Enter', async () => {
      const promise = dialogService.confirm({
        title: 'Test',
        message: 'Test',
      });

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
      const result = await promise;
      expect(result).toBe(true);
    });
  });

  describe('icon rendering', () => {
    it('should render info icon', () => {
      dialogService.alert({ title: 'Info', message: 'Test', type: 'info' });
      const iconContainer = document.getElementById('dialog-icon');
      expect(iconContainer.classList.contains('dialog-icon-info')).toBe(true);
      expect(iconContainer.innerHTML).toContain('svg');
    });

    it('should render error icon', () => {
      dialogService.error({ message: 'Test' });
      const iconContainer = document.getElementById('dialog-icon');
      expect(iconContainer.classList.contains('dialog-icon-error')).toBe(true);
    });

    it('should render danger icon', () => {
      dialogService.confirm({ title: 'Del', message: 'Sure?', type: 'danger' });
      const iconContainer = document.getElementById('dialog-icon');
      expect(iconContainer.classList.contains('dialog-icon-danger')).toBe(true);
    });

    it('should render success icon', () => {
      dialogService.success({ message: 'Done' });
      const iconContainer = document.getElementById('dialog-icon');
      expect(iconContainer.classList.contains('dialog-icon-success')).toBe(true);
    });

    it('should render warning icon', () => {
      dialogService.warning({ message: 'Watch out' });
      const iconContainer = document.getElementById('dialog-icon');
      expect(iconContainer.classList.contains('dialog-icon-warning')).toBe(true);
    });
  });
});
