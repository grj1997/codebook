import { describe, it, expect } from 'vitest';
import { normalizeOrigin, parseFillSequence, nextSelection } from '../src/domain';

describe('账户规则', () => {
  it('按精确 HTTPS 来源规范化，拒绝嵌入凭据和非 HTTPS', () => {
    expect(normalizeOrigin('https://EXAMPLE.com:443/login')).toBe('https://example.com');
    expect(() => normalizeOrigin('http://example.com')).toThrow();
    expect(() => normalizeOrigin('https://user:pass@example.com')).toThrow();
  });
  it('只接受受限输入规则，密码中的特殊字符不作为指令', () => {
    expect(parseFillSequence('账号, Tab, 等待:250, Shift+Tab, 密码')).toEqual([
      { kind: 'username' }, { kind: 'tab' }, { kind: 'delay', value: 250 }, { kind: 'shiftTab' }, { kind: 'password' },
    ]);
    expect(() => parseFillSequence('账号, cmd.exe')).toThrow();
    expect(() => parseFillSequence('等待:60000')).toThrow();
  });
  it('候选列表按方向键循环，无候选时不选中任何条目', () => {
    expect(nextSelection(0, -1, 3)).toBe(2);
    expect(nextSelection(2, 1, 3)).toBe(0);
    expect(nextSelection(0, 1, 0)).toBe(-1);
  });
});
