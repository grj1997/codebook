import { beforeEach, afterEach, it, expect, vi } from 'vitest';
import { flushPromises } from '@vue/test-utils';

let receive: (message: Record<string, unknown>) => void;
let runtimeMessage: (message: Record<string, unknown>, sender: Record<string, unknown>, respond: () => void) => unknown;
let messages: Record<string, unknown>[];
let delivered: Record<string, unknown>[];
let documentId: string;
let focused: boolean;
const ignoreEvent = { addListener: () => {} };
beforeEach(async () => {
  vi.resetModules(); messages = []; delivered = []; documentId = 'original-document'; focused = true;
  vi.stubGlobal('chrome', {
    runtime: { id: 'fixture-extension', connectNative: () => ({
      postMessage: (message: Record<string, unknown>) => messages.push(structuredClone(message)),
      onMessage: { addListener: (handler: typeof receive) => { receive = handler; } }, onDisconnect: ignoreEvent,
    }), onMessage: { addListener: (handler: typeof runtimeMessage) => { runtimeMessage = handler; } } },
    tabs: {
      get: async () => ({ id: 3, windowId: 4, active: true, url: 'https://example.test/login' }),
      query: async () => [{ id: 3, windowId: 4, url: 'https://example.test/login' }],
      sendMessage: async (_tab: number, message: Record<string, unknown>) => {
        if (message.type === 'codebook-probe') return { contentNonce: 'fixture-content-nonce', editable: true };
        delivered.push(structuredClone(message)); return { ok: true };
      }, onActivated: ignoreEvent, onUpdated: ignoreEvent,
    },
    windows: { get: async () => ({ focused }), onFocusChanged: ignoreEvent, WINDOW_ID_NONE: -1 },
    webNavigation: { getFrame: async () => ({ documentId, url: 'https://example.test/login' }) },
    action: { setBadgeText: async () => {}, setBadgeBackgroundColor: async () => {}, setTitle: async () => {}, onClicked: ignoreEvent },
    alarms: { create: () => {}, onAlarm: ignoreEvent },
  });
  await import('../browser-extension/src/background'); await flushPromises();
});
afterEach(() => { vi.unstubAllGlobals(); });
async function fill() {
  const target = messages.find(m => m.type === 'context')!.target;
  const message = { type: 'fill', target, token: 'one-shot-token', username: 'user@example.test', password: 'fixture-secret', mode: 'auto' };
  receive(message); await flushPromises(); return message;
}
it('只向经 tabs 和 webNavigation 核验的原文档发送填充，随后清除响应秘密', async () => {
  const message = await fill();
  expect(delivered).toHaveLength(1);
  expect(delivered[0]).toMatchObject({ type: 'codebook-fill', origin: 'https://example.test', password: 'fixture-secret' });
  expect(message.password).toBe('');
});
it('导航后不把旧授权的密码发给新文档', async () => {
  documentId = 'navigated-document'; await fill();
  expect(delivered).toHaveLength(0);
  expect(messages.at(-1)).toMatchObject({ type: 'fillResult', ok: false });
});
it('其他浏览器窗口处于前台时拒绝填充', async () => {
  focused = false; await fill();
  expect(delivered).toHaveLength(0);
  expect(messages.at(-1)).toMatchObject({ type: 'fillResult', ok: false });
});
it('不信任伪造的网页运行时消息', async () => {
  const before = messages.length;
  runtimeMessage({ type: 'codebook-focus' }, { id: 'webpage', frameId: 0, tab: { id: 3 }, url: 'https://example.test', documentId }, () => {});
  await flushPromises(); expect(messages).toHaveLength(before);
});
