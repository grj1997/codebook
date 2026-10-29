import { invoke } from '@tauri-apps/api/core';
import type { Status, Credential, AppTarget, Settings, PickerState, FillMode, BrowserConnection } from './types';

export const api = {
  status: () => invoke<Status>('vault_status'),
  create: (password: string) => invoke<void>('vault_create', { password }),
  unlock: (password: string) => invoke<void>('vault_unlock', { password }),
  lock: () => invoke<void>('vault_lock'),
  get: (id: string) => invoke<Credential>('credential_get', { id }),
  save: (entry: Credential) => invoke<void>('credential_save', { entry }),
  remove: (id: string) => invoke<void>('credential_delete', { id }),
  apps: () => invoke<AppTarget[]>('list_apps'),
  describe: (path: string) => invoke<AppTarget>('describe_app', { path }),
  copy: (id: string, field: 'username' | 'password') => invoke<void>('credential_copy', { id, field }),
  settings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  changePassword: (current: string, next: string) => invoke<void>('vault_change_password', { current, next }),
  backup: (path: string) => invoke<void>('vault_backup', { path }),
  verifyBackup: (path: string, password: string) => invoke<number>('vault_verify_backup', { path, password }),
  restore: (path: string, password: string) => invoke<void>('vault_restore', { path, password }),
  picker: () => invoke<PickerState | null>('picker_state'),
  fill: (token: string, id: string, mode: FillMode) => invoke<void>('fill_confirm', { token, id, mode }),
  cancel: () => invoke<void>('picker_cancel'),
  browsers: () => invoke<BrowserConnection[]>('browser_connections'),
  pair: (id: string) => invoke<void>('browser_pair', { id }),
  revoke: (extensionId: string) => invoke<void>('browser_revoke', { extensionId }),
  quit: () => invoke<void>('quit_app'),
};
