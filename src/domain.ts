export type FillStep = { kind: 'username' | 'password' | 'tab' | 'shiftTab' } | { kind: 'delay'; value: number };
export function normalizeOrigin(value: string): string {
  const url = new URL(value.trim());
  if (url.protocol !== 'https:' || !url.hostname || url.username || url.password) throw new Error('请输入不含用户名和密码的 HTTPS 网站地址');
  return url.origin;
}
export function parseFillSequence(value: string): FillStep[] {
  const tokens = value.split(/[,，→\n]/).map(s => s.trim()).filter(Boolean);
  if (!tokens.length || tokens.length > 32) throw new Error('输入顺序需要 1～32 个步骤');
  let delay = 0;
  return tokens.map(token => {
    const simple: Record<string, FillStep['kind']> = { '账号': 'username', '用户名': 'username', username: 'username', '密码': 'password', password: 'password', tab: 'tab', 'shift+tab': 'shiftTab' };
    const kind = simple[token.toLowerCase()];
    if (kind) return { kind } as FillStep;
    const wait = /^(?:等待|delay)[:：](\d+)$/i.exec(token);
    const ms = wait ? Number(wait[1]) : NaN;
    delay += ms;
    if (!Number.isInteger(ms) || ms < 0 || ms > 2000 || delay > 5000) throw new Error(`不支持的步骤：${token}。等待每步最多 2000 毫秒，总计最多 5000 毫秒`);
    return { kind: 'delay', value: ms };
  });
}
export function nextSelection(index: number, delta: number, count: number): number {
  return count > 0 ? ((index + delta) % count + count) % count : -1;
}
