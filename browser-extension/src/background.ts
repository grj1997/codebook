import { httpsOrigin, sameDocument, type DocumentIdentity } from './policy';

interface BrowserTarget extends DocumentIdentity {
  windowId: number; contentNonce: string; editable: boolean; active: boolean;
  focusedAt: number; browser: string;
}
interface NativeReply {
  type: string; message?: string; paired?: boolean; target?: BrowserTarget;
  token?: string; username?: string; password?: string; mode?: string;
}
let port: chrome.runtime.Port | null = null;
let target: BrowserTarget | null = null;
let lastFocus = Date.now();
const browserName = navigator.userAgent.includes('Edg/') ? 'Microsoft Edge' : 'Google Chrome';

function badge(text: string, color: string, title: string) {
  void chrome.action.setBadgeText({ text }); void chrome.action.setBadgeBackgroundColor({ color }); void chrome.action.setTitle({ title });
}
function connect() {
  if (port) return;
  try {
    const connection = chrome.runtime.connectNative('local.codebook.host'); port = connection;
    connection.onMessage.addListener((message: NativeReply) => { void receive(message); });
    connection.onDisconnect.addListener(() => { const reason = chrome.runtime.lastError?.message; port = null; target = null; badge('!', '#97764a', reason ? `Codebook 未连接：${reason}` : 'Codebook 未连接'); });
    connection.postMessage({ type: 'hello', browser: browserName });
  } catch { port = null; badge('!', '#97764a', '请先安装并运行 Codebook'); }
}
async function currentIdentity(tabId: number): Promise<DocumentIdentity> {
  const tab = await chrome.tabs.get(tabId);
  const frame = await chrome.webNavigation.getFrame({ tabId, frameId: 0 });
  if (!tab.url || !frame?.documentId || !frame.url) throw new Error('页面尚未就绪');
  const origin = httpsOrigin(tab.url);
  if (origin !== httpsOrigin(frame.url)) throw new Error('页面来源变化');
  return { tabId, documentId: frame.documentId, origin };
}
async function refreshContext(requestFill = false) {
  const tabs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const tab = tabs[0];
  if (!tab?.id || tab.windowId === undefined || !tab.url) throw new Error('没有可用的活动页面');
  const identity = await currentIdentity(tab.id);
  const window = await chrome.windows.get(tab.windowId);
  const probe = await chrome.tabs.sendMessage(tab.id, { type: 'codebook-probe' }, { documentId: identity.documentId });
  if (typeof probe?.contentNonce !== 'string') throw new Error('页面扩展尚未就绪');
  target = { ...identity, windowId: tab.windowId, contentNonce: probe.contentNonce, editable: probe.editable === true, active: window.focused === true, focusedAt: lastFocus, browser: browserName };
  connect(); port?.postMessage({ type: requestFill ? 'requestFill' : 'context', target });
}
async function receive(message: NativeReply) {
  if (message.type === 'status') {
    badge(message.paired ? '' : '?', '#23795f', message.paired ? '从 Codebook 选择账户' : '请在 Codebook 设置中允许此浏览器连接');
    return;
  }
  if (message.type === 'error') { badge('!', '#a95c4c', message.message ?? 'Codebook 操作未完成'); return; }
  if (message.type === 'queryContext') { try { await refreshContext(); } catch { port?.postMessage({ type: 'inactive' }); } return; }
  if (message.type !== 'fill' || !message.target || !message.token) return;
  try {
    const expected = message.target;
    if (!target || target.contentNonce !== expected.contentNonce || !sameDocument(expected, await currentIdentity(expected.tabId))) throw new Error('页面已经变化，请重新调用');
    const tab = await chrome.tabs.get(expected.tabId);
    const win = await chrome.windows.get(expected.windowId);
    if (!tab.active || tab.windowId !== expected.windowId || !win.focused) throw new Error('目标标签页已不在前台');
    const reply = await chrome.tabs.sendMessage(expected.tabId, {
      type: 'codebook-fill', origin: expected.origin, contentNonce: expected.contentNonce,
      username: message.username, password: message.password, mode: message.mode,
    }, { documentId: expected.documentId });
    if (!reply?.ok) throw new Error(reply?.error ?? '无法识别登录框');
    port?.postMessage({ type: 'fillResult', token: message.token, ok: true });
    badge('', '#23795f', '账户已填入，请自行点击登录');
  } catch (error) {
    const reason = error instanceof Error ? error.message : '填充失败';
    port?.postMessage({ type: 'fillResult', token: message.token, ok: false, message: reason });
    badge('!', '#a95c4c', reason);
  } finally {
    message.username = ''; message.password = '';
  }
}

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (message?.type !== 'codebook-focus' || sender.id !== chrome.runtime.id || sender.frameId !== 0 || !sender.tab?.id) return;
  void (async () => {
    try {
      const actual = await currentIdentity(sender.tab!.id!);
      if (sender.documentId !== actual.documentId || !sender.url || httpsOrigin(sender.url) !== actual.origin) return;
      lastFocus = Date.now(); await refreshContext(); respond({ ok: true });
    } catch { respond({ ok: false }); }
  })();
  return true;
});
chrome.action.onClicked.addListener(() => { lastFocus = Date.now(); connect(); void refreshContext(true).catch(error => badge('!', '#a95c4c', error instanceof Error ? error.message : '无法调用')); });
chrome.tabs.onActivated.addListener(() => { lastFocus = Date.now(); void refreshContext().catch(() => { target = null; port?.postMessage({ type: 'inactive' }); }); });
chrome.tabs.onUpdated.addListener((_id, change) => { if (change.status === 'complete') void refreshContext().catch(() => {}); else if (change.status === 'loading') { target = null; port?.postMessage({ type: 'inactive' }); } });
chrome.windows.onFocusChanged.addListener(id => {
  if (id === chrome.windows.WINDOW_ID_NONE) { if (target) { target.active = false; port?.postMessage({ type: 'context', target }); } }
  else { lastFocus = Date.now(); void refreshContext().catch(() => {}); }
});
chrome.alarms.create('codebook-heartbeat', { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(alarm => { if (alarm.name === 'codebook-heartbeat') { connect(); void refreshContext().catch(() => {}); } });
connect(); void refreshContext().catch(() => {});
