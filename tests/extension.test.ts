import { describe, it, expect, beforeEach } from 'vitest';
import { httpsOrigin, sameDocument, selectLoginFields } from '../browser-extension/src/policy';

describe('浏览器目标隔离', () => {
  it('不会把相似域名、子域名或新文档当成原页面', () => {
    const original = { tabId: 3, documentId: 'doc-a', origin: 'https://example.com' };
    expect(sameDocument(original, { ...original })).toBe(true);
    for (const actual of [{ ...original, tabId: 4 }, { ...original, documentId: 'doc-b' }, { ...original, origin: 'https://evil.example.com' }]) {
      expect(sameDocument(original, actual)).toBe(false);
    }
    expect(httpsOrigin('https://EXAMPLE.com/login')).toBe('https://example.com');
    expect(() => httpsOrigin('http://example.com')).toThrow();
  });
});

describe('只识别明确的登录表单', () => {
  beforeEach(() => { document.body.innerHTML = ''; });
  it('识别常规账号密码表单', () => {
    document.body.innerHTML = '<form><input autocomplete="username"><input type="password" autocomplete="current-password"></form>';
    const result = selectLoginFields(document);
    expect(result.username).toBe(document.querySelector('input'));
    expect(result.password).toBe(document.querySelector('[type=password]'));
  });
  it('拒绝修改密码及有歧义的多密码表单', () => {
    document.body.innerHTML = '<form><input autocomplete="username"><input type="password" autocomplete="new-password"></form>';
    expect(() => selectLoginFields(document)).toThrow();
    document.body.innerHTML = '<form><input type="password"><input type="password"></form>';
    expect(() => selectLoginFields(document)).toThrow();
  });
  it('不选择隐藏及不可编辑字段', () => {
    document.body.innerHTML = '<form><input autocomplete="username" disabled><input type="password" hidden></form>';
    expect(() => selectLoginFields(document)).toThrow();
  });
  it('解析 autocomplete token，不向注册字段填充，排除验证码', () => {
    document.body.innerHTML = '<form><input autocomplete="section-signup username"><input type="password" autocomplete="section-signup new-password"></form>';
    expect(() => selectLoginFields(document)).toThrow();
    document.body.innerHTML = '<form><input autocomplete="section-login username"><input autocomplete="section-login one-time-code"><input type="password"></form>';
    expect(selectLoginFields(document).username).toBe(document.querySelector('input'));
  });
  it('无输入框焦点时，不组合两个独立表单的字段', () => {
    document.body.innerHTML = '<form><input autocomplete="username"></form><form><input type="password"></form>';
    expect(() => selectLoginFields(document)).toThrow();
  });
});
