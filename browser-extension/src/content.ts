import { hasAutocomplete, httpsOrigin, selectLoginFields } from './policy';

const contentNonce = crypto.randomUUID();
let lastInput: HTMLInputElement | null = null;
function isEditable(element: Element | null): element is HTMLInputElement {
  return element instanceof HTMLInputElement && ['text', 'email', 'tel', 'password'].includes(element.type) && !element.disabled && !element.readOnly && !hasAutocomplete(element, 'one-time-code') && !hasAutocomplete(element, 'new-password');
}
function visible(input: HTMLInputElement): boolean {
  const bounds = input.getBoundingClientRect(), style = getComputedStyle(input);
  return input.isConnected && !input.closest('[hidden], [inert]') && bounds.width > 0 && bounds.height > 0 && style.visibility === 'visible' && style.display !== 'none';
}
function fillValue(input: HTMLInputElement, value: string) {
  if (!isEditable(input) || !visible(input)) throw new Error('目标输入框已不可编辑');
  input.focus();
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  if (!setter) throw new Error('当前输入框不支持填充');
  setter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}
document.addEventListener('focusin', event => {
  if (isEditable(event.target instanceof Element ? event.target : null)) lastInput = event.target as HTMLInputElement;
  void chrome.runtime.sendMessage({ type: 'codebook-focus' }).catch(() => {});
}, true);
chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (sender.id !== chrome.runtime.id || window !== window.top) return;
  if (message?.type === 'codebook-probe') {
    const input = isEditable(document.activeElement) ? document.activeElement : lastInput;
    respond({ contentNonce, editable: !!input && isEditable(input) && visible(input) }); return;
  }
  if (message?.type !== 'codebook-fill') return;
  try {
    if (message.contentNonce !== contentNonce || message.origin !== httpsOrigin(location.href) || document.visibilityState !== 'visible' || !document.hasFocus()) throw new Error('页面或焦点已改变，请重新调用');
    if (typeof message.username !== 'string' || typeof message.password !== 'string' || message.username.length > 4096 || message.password.length > 4096) throw new Error('账户数据无效');
    if (message.mode === 'username' || message.mode === 'password') {
      const input = document.activeElement instanceof HTMLInputElement ? document.activeElement : lastInput;
      if (!isEditable(input)) throw new Error('请先点击可填充的登录输入框');
      fillValue(input, message.mode === 'username' ? message.username : message.password);
    } else if (message.mode === 'auto') {
      if (lastInput?.isConnected) lastInput.focus();
      const { username, password } = selectLoginFields(document);
      if (username && !visible(username) || password && !visible(password)) throw new Error('登录字段不可见');
      if (username) fillValue(username, message.username);
      if (password) fillValue(password, message.password);
    } else throw new Error('不支持此填充方式');
    respond({ ok: true });
  } catch (error) { respond({ ok: false, error: error instanceof Error ? error.message : '填充失败' }); }
  finally { message.username = ''; message.password = ''; }
});
