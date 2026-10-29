import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import App from '../src/App.vue';
import Picker from '../src/Picker.vue';

const backend = vi.hoisted(() => ({
  unlocked: true,
  exists: true,
  dialogPath: null as string | null,
  restoreEvents: [] as string[],
  listeners: new Map<string, () => void>(),
  saved: null as unknown,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: async (name: string, cb: () => void) => { backend.listeners.set(name, cb); return () => backend.listeners.delete(name); } }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => backend.dialogPath, save: async () => null }));
vi.mock('../src/api', () => ({ api: {
  status: async () => ({ exists: backend.exists, unlocked: backend.unlocked, path: 'test/vault.cbvault', settings: { shortcut: 'Ctrl+Alt+K', mouseEnabled: true, doubleClickMs: null }, entries: backend.unlocked ? [{ id: 'a', title: '测试工作账户', usernameHint: 'us••••', bindings: [] }] : [] }),
  get: async () => ({ id: 'a', title: '测试工作账户', username: 'user@example.test', password: 'disposable-test-secret', notes: '', bindings: [], profile: { steps: [{ kind: 'username' }, { kind: 'tab' }, { kind: 'password' }] } }),
  apps: async () => [], save: async (entry: unknown) => { backend.saved = structuredClone(entry); },
  lock: async () => { backend.unlocked = false; }, browsers: async () => [],
  verifyBackup: async () => { backend.restoreEvents.push('verified'); return 3; },
  restore: async () => { backend.restoreEvents.push('restored'); },
  picker: async () => ({ token: 't', targetLabel: '测试窗口', entries: [{ id: 'a', title: '测试工作账户', usernameHint: 'us••••', bindings: [] }], browser: false, expiresAt: Date.now() + 30_000 }),
  fill: async () => { throw new Error('原输入框已失效'); }, cancel: async () => {},
} }));
let wrapper: VueWrapper | null = null;
beforeEach(() => { backend.exists = true; backend.unlocked = true; backend.saved = null; backend.dialogPath = null; backend.restoreEvents = []; backend.listeners.clear(); Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} }); });
afterEach(() => { wrapper?.unmount(); wrapper = null; vi.useRealTimers(); vi.restoreAllMocks(); });

describe('密码库界面', () => {
  it('列表仅显示摘要，编辑取消后移除敏感输入框', async () => {
    wrapper = mount(App); await flushPromises();
    expect(wrapper.text()).toContain('测试工作账户');
    expect(wrapper.text()).not.toContain('disposable-test-secret');
    await wrapper.findAll('button').find(b => b.text() === '编辑')!.trigger('click'); await flushPromises();
    expect(wrapper.find('input[type=password]').element).toHaveProperty('value', 'disposable-test-secret');
    await wrapper.find('[aria-label="关闭编辑"]').trigger('click');
    expect(wrapper.find('input[type=password]').exists()).toBe(false);
  });
  it('锁定事件关闭编辑器并清除账户列表', async () => {
    wrapper = mount(App); await flushPromises();
    await wrapper.findAll('button').find(b => b.text() === '编辑')!.trigger('click'); await flushPromises();
    backend.unlocked = false; backend.listeners.get('vault-changed')?.(); await flushPromises();
    expect(wrapper.text()).toContain('欢迎回来');
    expect(wrapper.find('.editor').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('测试工作账户');
  });
  it('拒绝非法备用步骤并保留待编辑账户', async () => {
    wrapper = mount(App); await flushPromises();
    await wrapper.findAll('button').find(b => b.text() === '编辑')!.trigger('click'); await flushPromises();
    await wrapper.find('input[placeholder="账号, Tab, 密码"]').setValue('账号, cmd.exe');
    await wrapper.find('form.editor').trigger('submit'); await flushPromises();
    expect(wrapper.find('.inline-error').text()).toContain('不支持的步骤');
    expect(backend.saved).toBeNull();
    expect(wrapper.find('.editor').exists()).toBe(true);
  });
  it('收进托盘时关闭编辑器，避免隐藏窗口保留密码表单', async () => {
    wrapper = mount(App); await flushPromises();
    await wrapper.findAll('button').find(b => b.text() === '编辑')!.trigger('click'); await flushPromises();
    backend.listeners.get('window-hidden')?.(); await flushPromises();
    expect(wrapper.find('.editor').exists()).toBe(false);
  });
  it('托盘锁定时关闭恢复窗口并清除其中的主密码', async () => {
    wrapper = mount(App); await flushPromises();
    await wrapper.findAll('button').find(b => b.text().includes('设置') && !b.text().includes('保存'))!.trigger('click');
    await wrapper.findAll('button').find(b => b.text() === '恢复备份')!.trigger('click');
    await wrapper.find('.small-modal input').setValue('backup-master-secret');
    backend.unlocked = false; backend.listeners.get('vault-changed')?.(); await flushPromises();
    expect(wrapper.find('.small-modal').exists()).toBe(false);
    await wrapper.findAll('button').find(b => b.text() === '从加密备份恢复')!.trigger('click');
    expect(wrapper.find('.small-modal input').element).toHaveProperty('value', '');
  });
  it('首次安装可直接恢复已有备份', async () => {
    backend.exists = backend.unlocked = false;
    wrapper = mount(App); await flushPromises();
    expect(wrapper.findAll('button').some(b => b.text() === '从加密备份恢复')).toBe(true);
  });
  it('备份先验证，再请求确认，确认后才替换', async () => {
    backend.exists = backend.unlocked = false; backend.dialogPath = 'fixture.cbvault';
    vi.spyOn(window, 'confirm').mockImplementation(() => { backend.restoreEvents.push('confirmed'); return true; });
    wrapper = mount(App); await flushPromises();
    await wrapper.findAll('button').find(b => b.text() === '从加密备份恢复')!.trigger('click');
    await wrapper.find('.small-modal input').setValue('backup-master-secret');
    await wrapper.find('form.small-modal').trigger('submit'); await flushPromises();
    expect(backend.restoreEvents).toEqual(['verified', 'confirmed', 'restored']);
  });
});
describe('账户浮窗', () => {
  it('后台拒绝失效目标后展示错误并允许取消', async () => {
    wrapper = mount(Picker); await flushPromises();
    await wrapper.find('.picker-entry').trigger('click'); await flushPromises();
    expect(wrapper.find('[role="alert"]').text()).toContain('原输入框已失效');
    expect(wrapper.find('[aria-label="关闭"]').exists()).toBe(true);
  });
});
