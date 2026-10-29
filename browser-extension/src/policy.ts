export interface DocumentIdentity { tabId: number; documentId: string; origin: string }
export function httpsOrigin(value: string): string {
  const url = new URL(value);
  if (url.protocol !== 'https:' || url.username || url.password) throw new Error('仅支持 HTTPS 网站');
  return url.origin;
}
export function sameDocument(expected: DocumentIdentity, actual: DocumentIdentity): boolean {
  try {
    return expected.tabId === actual.tabId && expected.documentId === actual.documentId &&
      !!expected.documentId && expected.origin === actual.origin && httpsOrigin(actual.origin) === actual.origin;
  } catch { return false; }
}
function editable(input: HTMLInputElement): boolean {
  const style = getComputedStyle(input);
  return !input.disabled && !input.readOnly && input.type !== 'hidden' && !input.closest('[hidden], [inert]') && style.display !== 'none' && style.visibility !== 'hidden' && style.visibility !== 'collapse';
}
export function hasAutocomplete(input: HTMLInputElement, token: string): boolean {
  return input.autocomplete.toLowerCase().split(/\s+/).includes(token);
}
export function selectLoginFields(root: Document): { username: HTMLInputElement | null; password: HTMLInputElement | null } {
  const focused = root.activeElement instanceof HTMLInputElement ? root.activeElement : null;
  const scope: ParentNode = focused?.form ?? root;
  const fields = Array.from(scope.querySelectorAll('input')).filter(editable);
  const loginFields = fields.filter(f => ['text', 'email', 'tel', 'password'].includes(f.type));
  if (new Set(loginFields.map(f => f.form)).size > 1) throw new Error('登录字段来自不同表单，请先聚焦目标表单');
  if (fields.some(f => hasAutocomplete(f, 'new-password'))) throw new Error('不自动填充注册或修改密码表单');
  const passwords = fields.filter(f => f.type === 'password');
  if (passwords.length > 1) throw new Error('密码框不明确，请聚焦目标登录表单');
  const texts = fields.filter(f => ['text', 'email', 'tel'].includes(f.type) && !hasAutocomplete(f, 'one-time-code'));
  const explicit = texts.filter(f => hasAutocomplete(f, 'username') || f.type === 'email' || /user|email|account|login/i.test(`${f.name} ${f.id}`));
  const username = explicit.length === 1 ? explicit[0] : texts.length === 1 ? texts[0] : null;
  if (!passwords.length && !username) throw new Error('没有找到可编辑的登录字段');
  if (passwords.length && texts.length > 1 && !username) throw new Error('账号框不明确，请先聚焦目标表单');
  return { username, password: passwords[0] ?? null };
}
