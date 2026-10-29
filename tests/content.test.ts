import { beforeEach, afterEach, it, expect, vi } from 'vitest';
type Handler = (message: Record<string, unknown>, sender: { id: string }, respond: (value: Record<string, unknown>) => void) => void;
let handler: Handler;
let nonce = '';
beforeEach(async () => {
  vi.resetModules();
  document.body.innerHTML = '<form><input autocomplete="username"><input type="password" autocomplete="current-password"><button>登录</button></form>';
  vi.spyOn(document, 'hasFocus').mockReturnValue(true);
  vi.spyOn(HTMLInputElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(10, 10, 200, 30));
  vi.stubGlobal('chrome', { runtime: { id: 'fixture-extension', onMessage: { addListener: (callback: Handler) => { handler = callback; } }, sendMessage: async () => ({ ok: true }) } });
  await import('../browser-extension/src/content');
  document.querySelector('input')!.focus();
  handler({ type: 'codebook-probe' }, { id: 'fixture-extension' }, reply => { nonce = reply.contentNonce as string; });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
function fill(overrides: Record<string, unknown> = {}) {
  let response: Record<string, unknown> = {};
  handler({ type: 'codebook-fill', origin: 'https://example.test', contentNonce: nonce, mode: 'auto', username: 'user@example.test', password: 'literal{TAB}密码!', ...overrides }, { id: 'fixture-extension' }, reply => { response = reply; });
  return response;
}
it('填入实际 DOM 字段，保留特殊字符并派发输入事件，不提交表单', () => {
  let inputEvents = 0, submissions = 0;
  document.addEventListener('input', () => inputEvents++, { once: true });
  document.querySelector('form')!.addEventListener('submit', () => submissions++);
  expect(fill().ok).toBe(true);
  expect(document.querySelector<HTMLInputElement>('input')!.value).toBe('user@example.test');
  expect(document.querySelector<HTMLInputElement>('[type=password]')!.value).toBe('literal{TAB}密码!');
  expect(inputEvents).toBe(1); expect(submissions).toBe(0);
});
it('旧文档标识和错误来源不会取得密码', () => {
  expect(fill({ contentNonce: 'stale' }).ok).toBe(false);
  expect(fill({ origin: 'https://evil.example.test' }).ok).toBe(false);
  expect(document.querySelector<HTMLInputElement>('[type=password]')!.value).toBe('');
});
it('填写账号导致表单替换时，拒绝向替换后的密码框泄露密码', () => {
  document.querySelector('input')!.addEventListener('input', () => { document.querySelector('[type=password]')!.outerHTML = '<input type="password" id="replacement">'; });
  expect(fill().ok).toBe(false);
  expect(document.querySelector<HTMLInputElement>('#replacement')!.value).toBe('');
});
it('显式填当前框也拒绝 tokenized new-password 字段', () => {
  const input = document.querySelector<HTMLInputElement>('[type=password]')!;
  input.autocomplete = 'section-signup new-password'; input.focus();
  expect(fill({ mode: 'password' }).ok).toBe(false);
  expect(input.value).toBe('');
});
