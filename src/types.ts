import type { FillStep } from './domain';
export type Binding = { kind: 'desktop'; path: string; title_contains: string } | { kind: 'website'; origin: string };
export interface Credential { id: string; title: string; username: string; password: string; notes: string; bindings: Binding[]; profile: { steps: FillStep[] } }
export interface Summary { id: string; title: string; usernameHint: string; bindings: Binding[] }
export interface Settings { shortcut: string; mouseEnabled: boolean; doubleClickMs: number | null }
export interface Status { exists: boolean; unlocked: boolean; entries: Summary[]; settings: Settings; path: string }
export interface AppTarget { name: string; path: string; title: string }
export interface BrowserConnection { id: string; extensionId: string; paired: boolean; browser: string; origin: string | null }
export type FillMode = 'auto' | 'username' | 'password' | 'sequence';
export interface PickerState { token: string; targetLabel: string; entries: Summary[]; browser: boolean; expiresAt: number }
