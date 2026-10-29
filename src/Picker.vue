<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { nextSelection } from './domain';
import type { FillMode, PickerState } from './types';
const state = ref<PickerState | null>(null); const query = ref(''); const selected = ref(0); const mode = ref<FillMode>('auto'); const error = ref(''); const busy = ref(false); const expired = ref(false);
const entries = computed(() => state.value?.entries.filter(e => `${e.title} ${e.usernameHint}`.toLowerCase().includes(query.value.toLowerCase())) ?? []);
let unlisten: (() => void) | undefined; let timer: ReturnType<typeof setInterval> | undefined;
async function refresh() { state.value = await api.picker(); query.value = ''; selected.value = 0; mode.value = 'auto'; error.value = ''; expired.value = false; }
async function fill(id: string) { if (busy.value || expired.value || !state.value) return; busy.value = true; error.value = ''; try { await api.fill(state.value.token, id, mode.value); } catch (e) { error.value = String(e); } finally { busy.value = false; } }
function key(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); void api.cancel(); }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); selected.value = nextSelection(selected.value, event.key === 'ArrowDown' ? 1 : -1, entries.value.length); }
  if (event.key === 'Enter' && event.target instanceof HTMLInputElement) { event.preventDefault(); const entry = entries.value[selected.value]; if (entry) void fill(entry.id); }
}
onMounted(async () => { await refresh(); unlisten = await listen('picker-ready', () => { void refresh(); }); document.addEventListener('keydown', key); timer = setInterval(() => { expired.value = !!state.value && Date.now() > state.value.expiresAt; }, 300); });
onUnmounted(() => { unlisten?.(); document.removeEventListener('keydown', key); if (timer) clearInterval(timer); });
</script>
<template>
  <main class="picker"><header><span class="brand-icon small">c<span>·</span></span><div><strong>选择账户</strong><small>{{ state?.targetLabel ?? '等待目标' }}</small></div><button class="icon-button" @click="api.cancel()" aria-label="关闭">×</button></header><div class="picker-search"><span>⌕</span><input v-model="query" @input="selected = 0" autofocus placeholder="搜索匹配账户…"></div><div class="picker-entries"><button v-for="(entry, index) in entries" :key="entry.id" class="picker-entry" :class="{ selected: selected === index }" :disabled="busy || expired" @mouseenter="selected = index" @click="fill(entry.id)"><span class="avatar">{{ entry.title.slice(0, 1) }}</span><div><strong>{{ entry.title }}</strong><small>{{ entry.usernameHint }}</small></div><span>↵</span></button><div v-if="!entries.length" class="picker-empty">当前目标尚未关联账户。<br>请在主窗口添加或关联。</div></div><div class="picker-options"><label>填充方式<select v-model="mode"><option value="auto">识别登录表单</option><option value="username">仅填当前框 · 账号</option><option value="password">仅填当前框 · 密码</option><option v-if="!state?.browser" value="sequence">使用输入顺序</option></select></label></div><p v-if="error || expired" class="picker-error" role="alert">{{ expired ? '本次调用已过期，请返回登录框重新触发。' : error }}</p><footer><span>↑ ↓ 选择　↵ 填充　Esc 取消</span><span>本机密码库</span></footer></main>
</template>
