<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api } from './api';
import { normalizeOrigin, parseFillSequence } from './domain';
import type { AppTarget, Binding, BrowserConnection, Credential, Settings, Status } from './types';

const status = ref<Status | null>(null);
const error = ref(''); const notice = ref(''); const busy = ref(false);
const password = ref(''); const confirmation = ref(''); const showMaster = ref(false);
const section = ref('all'); const query = ref('');
const editing = ref<Credential | null>(null); const showSecret = ref(false);
const apps = ref<AppTarget[]>([]); const selectedApp = ref(''); const newWebsite = ref('');
const sequence = ref('账号, Tab, 密码');
const settings = ref<Settings>({ shortcut: 'Ctrl+Alt+K', mouseEnabled: true, doubleClickMs: null });
const currentMaster = ref(''); const nextMaster = ref(''); const confirmMaster = ref('');
const connections = ref<BrowserConnection[]>([]);
const restoreForm = ref(false); const restorePassword = ref('');
let unsubscribe: (() => void) | undefined; let browserTimer: ReturnType<typeof setInterval> | undefined;
const desktopCount = computed(() => new Set(status.value?.entries.flatMap(e => e.bindings.filter(b => b.kind === 'desktop').map(b => b.path.toLowerCase()))).size);
const websiteCount = computed(() => new Set(status.value?.entries.flatMap(e => e.bindings.filter(b => b.kind === 'website').map(b => b.origin))).size);
const entries = computed(() => (status.value?.entries ?? []).filter(e => {
  const kind = section.value === 'desktop' ? 'desktop' : section.value === 'website' ? 'website' : null;
  return (!kind || e.bindings.some(b => b.kind === kind)) && `${e.title} ${e.usernameHint} ${e.bindings.map(bindingLabel).join(' ')}`.toLowerCase().includes(query.value.toLowerCase());
}));
const title = computed(() => ({ all: '全部账户', desktop: '桌面应用', website: '网站账户', settings: '设置' })[section.value] ?? '全部账户');
const native = '__TAURI_INTERNALS__' in window;
function bindingLabel(b: Binding) { return b.kind === 'website' ? b.origin.replace('https://', '') : b.path.split(/[\\/]/).pop() ?? b.path; }
function message(e: unknown) { return e instanceof Error ? e.message : String(e); }
async function run(task: () => Promise<void>, success = '') {
  if (busy.value) return;
  busy.value = true; error.value = ''; notice.value = '';
  try { await task(); if (success) notice.value = success; }
  catch (e) { error.value = message(e); }
  finally { busy.value = false; }
}
async function refresh() {
  status.value = await api.status(); settings.value = { ...status.value.settings };
  if (!status.value.unlocked) { clearEditor(); currentMaster.value = nextMaster.value = confirmMaster.value = ''; restorePassword.value = ''; restoreForm.value = false; password.value = confirmation.value = ''; }
}
async function unlock() {
  await run(async () => {
    try {
      if (!status.value?.exists) {
        if (password.value !== confirmation.value) throw new Error('两次主密码不一致');
        await api.create(password.value);
      } else await api.unlock(password.value);
      await refresh();
    } finally { password.value = confirmation.value = ''; }
  });
}
function clearEditor() { if (editing.value) { editing.value.password = ''; editing.value.username = ''; editing.value.notes = ''; } editing.value = null; showSecret.value = false; }
async function edit(id?: string) {
  await run(async () => {
    clearEditor();
    editing.value = id ? await api.get(id) : { id: '', title: '', username: '', password: '', notes: '', bindings: [], profile: { steps: parseFillSequence('账号, Tab, 密码') } };
    sequence.value = editing.value.profile.steps.map(s => s.kind === 'delay' ? `等待:${s.value}` : ({ username: '账号', password: '密码', tab: 'Tab', shiftTab: 'Shift+Tab' })[s.kind]).join(', ');
    selectedApp.value = ''; newWebsite.value = '';
    apps.value = await api.apps();
  });
}
function addApp() {
  if (!editing.value) return;
  const app = apps.value.find(a => a.path === selectedApp.value); if (!app) return;
  if (!editing.value.bindings.some(b => b.kind === 'desktop' && b.path.toLowerCase() === app.path.toLowerCase())) editing.value.bindings.push({ kind: 'desktop', path: app.path, title_contains: '' });
}
async function chooseExe() {
  await run(async () => {
    const path = await open({ multiple: false, filters: [{ name: 'Windows 应用', extensions: ['exe'] }] });
    if (typeof path !== 'string' || !editing.value) return;
    const app = await api.describe(path); apps.value.push(app); selectedApp.value = app.path; addApp();
  });
}
function addWebsite() {
  error.value = '';
  try {
    const origin = normalizeOrigin(newWebsite.value);
    if (!editing.value?.bindings.some(b => b.kind === 'website' && b.origin === origin)) editing.value?.bindings.push({ kind: 'website', origin });
    newWebsite.value = '';
  } catch (e) { error.value = message(e); }
}
async function saveEntry() {
  await run(async () => {
    if (!editing.value) return;
    editing.value.profile.steps = parseFillSequence(sequence.value);
    await api.save(editing.value); clearEditor(); await refresh();
  }, '账户已加密保存');
}
async function remove(id: string) {
  if (!window.confirm('删除这一条账户？此操作会更新本地密码库。')) return;
  await run(async () => { await api.remove(id); await refresh(); }, '账户已删除');
}
async function exportBackup() {
  await run(async () => {
    const path = await save({ defaultPath: `Codebook-${new Date().toISOString().slice(0, 10)}.cbvault`, filters: [{ name: '加密密码库', extensions: ['cbvault'] }] });
    if (path) { await api.backup(path); notice.value = '加密备份已保存'; }
  });
}
async function restoreBackup() {
  await run(async () => {
    try {
      const path = await open({ multiple: false, filters: [{ name: '加密密码库', extensions: ['cbvault'] }] });
      if (typeof path !== 'string') return;
      const count = await api.verifyBackup(path, restorePassword.value);
      if (!window.confirm(`备份验证通过，包含 ${count} 条账户。替换当前密码库？原库会保留为恢复前备份。`)) return;
      await api.restore(path, restorePassword.value); restoreForm.value = false; await refresh(); notice.value = '备份已恢复';
    } finally { restorePassword.value = ''; }
  });
}
async function changeMaster() {
  await run(async () => {
    try {
      if (nextMaster.value !== confirmMaster.value) throw new Error('新主密码两次输入不一致');
      await api.changePassword(currentMaster.value, nextMaster.value);
      notice.value = '主密码已修改。旧备份仍使用旧主密码';
    } finally { currentMaster.value = nextMaster.value = confirmMaster.value = ''; }
  });
}
async function lockVault() { await run(async () => { await api.lock(); await refresh(); }); }
async function copyPassword(id: string) { await run(() => api.copy(id, "password"), "密码已复制，30 秒后清理"); }
async function savePrefs() { await run(async () => { await api.settings({ ...settings.value, doubleClickMs: settings.value.doubleClickMs ? Number(settings.value.doubleClickMs) : null }); await refresh(); }, "快捷设置已保存"); }
async function pairConnection(id: string) { await run(async () => { await api.pair(id); await refreshBrowsers(); }, "浏览器已配对"); }
async function revokeConnection(id: string) { await run(async () => { await api.revoke(id); await refreshBrowsers(); }, "已撤销授权"); }
async function refreshBrowsers() { if (status.value?.unlocked && section.value === 'settings') { try { connections.value = await api.browsers(); } catch { /* disconnect is surfaced by status refresh */ } } }
onMounted(async () => {
  if (!native) { error.value = '当前是界面预览。请使用 npm run tauri dev 启动桌面程序，密码库只在桌面端可用。'; return; }
  try {
    await refresh();
    const changed = await listen('vault-changed', () => { void refresh().catch(e => { error.value = message(e); }); });
    const failed = await listen<string>('operation-error', event => { error.value = event.payload; });
    const hidden = await listen('window-hidden', () => { clearEditor(); password.value = confirmation.value = currentMaster.value = nextMaster.value = confirmMaster.value = restorePassword.value = ''; restoreForm.value = false; });
    unsubscribe = () => { changed(); failed(); hidden(); };
    browserTimer = setInterval(refreshBrowsers, 2000);
  }
  catch (e) { error.value = message(e); }
});
onUnmounted(() => { unsubscribe?.(); if (browserTimer) clearInterval(browserTimer); clearEditor(); });
</script>

<template>
  <div class="shell" :class="{ 'is-locked': !status?.unlocked }">
    <aside class="sidebar">
      <div class="brand"><span class="brand-icon">c<span>·</span></span><div>codebook<small>私有密码库</small></div></div>
      <div class="nav-label">工作空间</div>
      <nav>
        <button v-for="item in [{ key: 'all', icon: '▦', label: '全部账户' }, { key: 'desktop', icon: '▣', label: '桌面应用' }, { key: 'website', icon: '◎', label: '网站账户' }]" :key="item.key" :class="{ active: section === item.key }" :disabled="!status?.unlocked" @click="section = item.key"><span>{{ item.icon }}</span>{{ item.label }}<b v-if="item.key === 'all'">{{ status?.entries.length ?? 0 }}</b></button>
      </nav>
      <div class="sidebar-note"><span class="outline-lock">⌑</span><p>你的账户，留在本机。<small>本地存储 · 加密保管</small></p></div>
      <div class="sidebar-bottom"><button :class="{ active: section === 'settings' }" :disabled="!status?.unlocked" @click="section = 'settings'; refreshBrowsers()"><span>⚙</span>设置</button><button :disabled="!status?.unlocked" @click="lockVault"><span>♧</span>锁定密码库</button><div class="local-status"><i></i>仅在此设备运行 <span>v0.1</span></div></div>
    </aside>

    <main class="main">
      <header class="topbar"><span>个人密码库 <span class="breadcrumb">/ {{ status?.unlocked ? title : '解锁' }}</span></span><span class="status-pill"><i :class="{ locked: !status?.unlocked }"></i>{{ status?.unlocked ? '已解锁' : '已锁定' }}</span></header>
      <div v-if="error" class="banner error" role="alert">{{ error }}<button @click="error = ''" aria-label="关闭提示">×</button></div>
      <div v-if="notice" class="banner success" role="status">{{ notice }}<button @click="notice = ''" aria-label="关闭提示">×</button></div>

      <section v-if="!status?.unlocked" class="unlock-page">
        <div class="unlock-decoration"><span>⌑</span><i></i><i></i></div>
        <div class="eyebrow">YOUR PRIVATE SPACE</div>
        <h1>{{ status?.exists ? '欢迎回来' : '建立你的私有密码库' }}</h1>
        <p>{{ status?.exists ? '输入主密码，打开本机保存的账户。' : '一个主密码，保管你的应用与网站账户。' }}</p>
        <form class="unlock-form" @submit.prevent="unlock">
          <label>主密码<div class="password-input"><input v-model="password" :type="showMaster ? 'text' : 'password'" :autocomplete="status?.exists ? 'current-password' : 'new-password'" placeholder="请输入主密码" :minlength="status?.exists ? undefined : 12" required autofocus><button type="button" @click="showMaster = !showMaster">{{ showMaster ? '隐藏' : '显示' }}</button></div></label>
          <label v-if="!status?.exists">确认主密码<input v-model="confirmation" type="password" autocomplete="new-password" placeholder="至少 12 个字符，再输入一次" minlength="12" required></label>
          <button class="primary wide" :disabled="busy || !native">{{ busy ? '正在处理…' : status?.exists ? '解锁密码库 →' : '创建密码库 →' }}</button>
        </form>
        <p class="unlock-footnote">{{ status?.exists ? '本次后台运行期间保持解锁，退出后需要重新验证。' : '请记住主密码，密码库没有找回入口。' }}</p>
        <button class="text-button" @click="restoreForm = !restoreForm">从加密备份恢复</button>
      </section>

      <section v-else-if="section !== 'settings'" class="workspace">
        <div class="page-heading"><div><div class="eyebrow">PERSONAL VAULT</div><h1>{{ title }}<span class="count-chip">{{ entries.length }}</span></h1><p>集中保管，随时调用。</p></div><button class="primary" @click="edit()" :disabled="busy"><span>＋</span> 新增账户</button></div>
        <div class="overview"><div><span class="metric-icon green">▦</span><p>已保存账户<strong>{{ status.entries.length }}<small>个</small></strong></p></div><div><span class="metric-icon blue">▣</span><p>已关联软件<strong>{{ desktopCount }}<small>款</small></strong></p></div><div><span class="metric-icon amber">◎</span><p>已关联网站<strong>{{ websiteCount }}<small>个</small></strong></p></div></div>
        <div class="list-toolbar"><div class="search-input"><span>⌕</span><input v-model="query" placeholder="搜索账户、应用或网站…" aria-label="搜索账户"><kbd>搜索</kbd></div><span class="list-count">{{ entries.length }} 条账户</span></div>
        <div class="entry-table"><div class="table-head"><span>账户</span><span>关联目标</span><span>操作</span></div>
          <article v-for="entry in entries" :key="entry.id" class="entry-row"><div class="entry-identity"><span class="avatar" :class="entry.bindings.some(b => b.kind === 'website') ? 'web' : 'app'">{{ entry.title.slice(0, 1).toUpperCase() }}</span><div><h3>{{ entry.title }}</h3><p>{{ entry.usernameHint }}</p></div></div><div class="bindings"><span v-for="(binding, i) in entry.bindings.slice(0, 2)" :key="i" :title="binding.kind === 'website' ? binding.origin : binding.path">{{ binding.kind === 'website' ? '◎' : '▣' }} {{ bindingLabel(binding) }}</span><span v-if="entry.bindings.length > 2">+{{ entry.bindings.length - 2 }}</span><span v-if="!entry.bindings.length" class="muted">尚未关联</span></div><div class="row-actions"><button @click="copyPassword(entry.id)" title="复制密码">复制</button><button @click="edit(entry.id)">编辑</button><button class="danger subtle" @click="remove(entry.id)" title="删除账户">×</button></div></article>
          <div v-if="!entries.length" class="empty-state"><span>⌑</span><h2>{{ query ? '没有找到匹配账户' : '从第一个账户开始' }}</h2><p>{{ query ? '试试其他名称、应用或网站。' : '添加账号密码，关联它所属的软件或网站。' }}</p><button v-if="!query" class="secondary" @click="edit()">＋ 添加账户</button></div>
        </div>
        <div class="shortcut-help"><span>✧</span><p>打开登录界面，按 <kbd>{{ settings.shortcut }}</kbd> 或在输入框连续两次右键，选择账户即可填充。</p></div>
      </section>

      <section v-else class="workspace settings-page">
        <div class="page-heading"><div><div class="eyebrow">PREFERENCES</div><h1>设置</h1><p>让密码库按你的习惯工作。</p></div></div>
        <div class="settings-card"><h2>快捷调用</h2><label>键盘快捷键<input v-model="settings.shortcut" placeholder="Ctrl+Alt+K"></label><label class="check-row"><input type="checkbox" v-model="settings.mouseEnabled">输入框内连续两次右键调出账户</label><p class="field-help">保留原软件右键菜单，不延迟单次右键。</p><label>双击间隔（毫秒）<input type="number" min="150" max="1000" v-model="settings.doubleClickMs" placeholder="留空则跟随 Windows"></label><button class="primary" :disabled="busy" @click="savePrefs()">保存设置</button></div>
        <div class="settings-card"><h2>浏览器连接</h2><p class="field-help">加载配套扩展后，点击扩展连接。本机确认后才允许访问对应网站的账户。</p><div v-for="connection in connections" :key="connection.id" class="connection-row"><div><strong>{{ connection.browser || '浏览器' }}</strong><small>{{ connection.extensionId }}</small><small>{{ connection.origin ?? '等待打开 HTTPS 网站' }}</small></div><button v-if="!connection.paired" class="primary" @click="pairConnection(connection.id)">允许连接</button><button v-else class="secondary" @click="revokeConnection(connection.extensionId)">撤销授权</button></div><p v-if="!connections.length" class="muted">暂无连接。请按 README 安装 Chrome／Edge 扩展。</p></div>
        <div class="settings-card"><h2>备份与恢复</h2><p class="field-help">备份始终加密。主密码修改前的备份需要使用旧主密码。</p><div class="button-row"><button class="secondary" @click="exportBackup" :disabled="busy">导出加密备份</button><button class="secondary" @click="restoreForm = true">恢复备份</button></div><p class="vault-path">{{ status.path }}</p></div>
        <form class="settings-card" @submit.prevent="changeMaster"><h2>修改主密码</h2><label>当前主密码<input v-model="currentMaster" type="password" autocomplete="current-password" required></label><div class="two-columns"><label>新主密码<input v-model="nextMaster" type="password" autocomplete="new-password" minlength="12" required></label><label>再次确认<input v-model="confirmMaster" type="password" autocomplete="new-password" minlength="12" required></label></div><button class="primary" :disabled="busy">更新主密码</button></form>
        <div class="settings-card"><h2>后台与锁定</h2><p class="field-help">关闭主窗口后留在托盘。锁屏、休眠或闲置时保持解锁；退出、崩溃或重启后重新输入主密码。</p><button class="secondary danger" @click="api.quit()">彻底退出 Codebook</button></div>
      </section>
    </main>

    <div v-if="editing" class="modal-backdrop" @click.self="clearEditor"><form class="editor modal" @submit.prevent="saveEntry"><div class="modal-heading"><div><div class="eyebrow">ACCOUNT DETAILS</div><h2>{{ editing.id ? '编辑账户' : '新增账户' }}</h2></div><button type="button" class="icon-button" @click="clearEditor" aria-label="关闭编辑">×</button></div><div class="editor-body"><label>账户名称<input v-model="editing.title" maxlength="160" placeholder="例如：工作邮箱" required></label><label>账号<input v-model="editing.username" autocomplete="off" maxlength="4096" placeholder="用户名、邮箱或手机号"></label><label>密码<div class="password-input"><input v-model="editing.password" :type="showSecret ? 'text' : 'password'" autocomplete="new-password" maxlength="4096" required><button type="button" @click="showSecret = !showSecret">{{ showSecret ? '隐藏' : '显示' }}</button></div></label><div class="editor-section"><h3>关联桌面软件</h3><div class="input-with-button"><select v-model="selectedApp"><option value="">选择正在运行的软件</option><option v-for="app in apps" :key="app.path" :value="app.path">{{ app.name }} — {{ app.title.slice(0, 36) }}</option></select><button type="button" class="secondary" @click="addApp">关联</button><button type="button" class="secondary" @click="chooseExe">选择 EXE</button></div></div><div class="editor-section"><h3>关联网站</h3><div class="input-with-button"><input v-model="newWebsite" placeholder="https://example.com" @keydown.enter.prevent="addWebsite"><button type="button" class="secondary" @click="addWebsite">关联</button></div><p class="field-help">只匹配明确关联的网站来源，其他子域名请单独添加。</p></div><div v-for="(binding, index) in editing.bindings" :key="index" class="binding-editor"><div><span>{{ binding.kind === 'website' ? '◎' : '▣' }}</span><small>{{ binding.kind === 'website' ? binding.origin : binding.path }}</small><button type="button" @click="editing.bindings.splice(index, 1)" aria-label="移除关联">×</button></div><input v-if="binding.kind === 'desktop'" v-model="binding.title_contains" placeholder="窗口标题需包含（可留空）"></div><label>备用输入顺序<input v-model="sequence" placeholder="账号, Tab, 密码"><small class="field-help">支持：账号、密码、Tab、Shift+Tab、等待:250。先点击规则对应的起始输入框。</small></label><label>备注<textarea v-model="editing.notes" rows="3" maxlength="16384" placeholder="仅保存在加密密码库中"></textarea></label><div v-if="error" class="inline-error" role="alert">{{ error }}</div></div><footer class="modal-footer"><span>⌑ 加密保存于本机</span><button type="button" class="secondary" @click="clearEditor">取消</button><button class="primary" :disabled="busy">{{ busy ? '正在保存…' : '保存账户' }}</button></footer></form></div>

    <div v-if="restoreForm" class="modal-backdrop" @click.self="restoreForm = false; restorePassword = ''"><form class="modal small-modal" @submit.prevent="restoreBackup"><h2>恢复加密备份</h2><p class="field-help">输入备份当时的主密码。验证成功后才会替换，当前库将保留为恢复前备份。</p><label>备份主密码<input v-model="restorePassword" type="password" autocomplete="off" required></label><div v-if="error" class="inline-error">{{ error }}</div><div class="button-row"><button type="button" class="secondary" @click="restoreForm = false; restorePassword = ''">取消</button><button class="primary" :disabled="busy">选择备份并恢复</button></div></form></div>
  </div>
</template>
