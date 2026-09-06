<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { FileText, ClipboardList, Cloud, HardDriveDownload, Settings, Zap, Minus, Square, X } from 'lucide-vue-next';
import { api } from '../lib/api';
import { lastAutoUpdate } from '../lib/updateCache';
import { managerKey } from '../lib/context';
import type { AppData } from '../types';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import type { ToastAction } from '../lib/context';
import PromptsPane from '../components/PromptsPane.vue';
import ClipboardPane from '../components/ClipboardPane.vue';
import SyncPane from '../components/SyncPane.vue';
import DataPane from '../components/DataPane.vue';
import SettingsPane from '../components/SettingsPane.vue';

const data = ref<AppData | null>(null);
const tab = ref<'prompts' | 'clipboard' | 'sync' | 'data' | 'settings'>('prompts');
const toastMsg = ref('');
const toastKind = ref<'ok' | 'err'>('ok');
const toastAction = ref<ToastAction | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | undefined;

const confirmOpen = ref(false);
const confirmOpts = ref<{ title: string; message?: string; confirmText?: string; danger?: boolean }>({
  title: '',
});
let confirmResolve: ((v: boolean) => void) | undefined;

function confirm(options: {
  title: string;
  message?: string;
  confirmText?: string;
  danger?: boolean;
}): Promise<boolean> {
  confirmOpts.value = options;
  confirmOpen.value = true;
  return new Promise((resolve) => (confirmResolve = resolve));
}

function settleConfirm(v: boolean) {
  confirmOpen.value = false;
  confirmResolve?.(v);
  confirmResolve = undefined;
}

const syncLabel = computed(() => {
  const s = data.value?.settings;
  if (!s) return '未配置';
  if (s.syncProvider === 'gist') return s.gist.enabled ? 'GitHub 同步已启用' : 'GitHub 未启用';
  return s.webdav.enabled ? 'WebDAV 已启用' : '云同步未启用';
});
const syncOn = computed(() => {
  const s = data.value?.settings;
  if (!s) return false;
  return s.syncProvider === 'gist' ? s.gist.enabled : s.webdav.enabled;
});

async function refresh() {
  try {
    data.value = await api.getData();
  } catch (e) {
    toast(String(e), 'err');
  }
}

/* 无框窗口：自定义标题栏的窗口控制（真实后端生效；浏览器 shim 里静默失败） */
const win = getCurrentWindow();
function winMinimize() {
  win.minimize().catch((e) => console.warn('[prompt-tool] 最小化失败:', e));
}
function winToggleMaximize() {
  win.toggleMaximize().catch((e) => console.warn('[prompt-tool] 最大化/还原失败:', e));
}
function winClose() {
  win.close().catch((e) => console.warn('[prompt-tool] 关闭窗口失败:', e));
}

/* 自管标题栏拖拽：不用 data-tauri-drag-region，因为其 start_dragging 在
   最大化窗口上不会先还原，拖拽会完全无效果（无法变尺寸）。原生惯例是
   最大化时拖动标题栏 = 还原窗口并跟随鼠标。 */
async function onTitlebarMouseDown(e: MouseEvent) {
  if (e.button !== 0) return;
  if ((e.target as HTMLElement).closest('button')) return;
  e.preventDefault();
  if (await win.isMaximized()) await win.unmaximize();
  await win.startDragging();
}
function onTitlebarDoubleClick() {
  winToggleMaximize();
}

function toast(msg: string, kind: 'ok' | 'err' = 'ok', action?: ToastAction, ms?: number) {
  toastMsg.value = msg;
  toastKind.value = kind;
  toastAction.value = action ?? null;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(
    () => {
      toastMsg.value = '';
      toastAction.value = null;
    },
    ms ?? (action ? 5000 : 2200),
  );
}

/** 离开提示词页的守卫（由 PromptsPane 注册）：返回 false = 有未保存草稿 */
const leaveGuard = ref<(() => boolean) | null>(null);
function setLeaveGuard(guard: (() => boolean) | null) {
  leaveGuard.value = guard;
}

type TabId = (typeof tabs)[number]['id'];

/** 切换标签页。从提示词页离开前先过守卫：有未保存草稿时弹确认，
 *  静默卸载会把用户未保存的标题/内容/快捷键修改无提示丢掉（评审 I6） */
async function switchTab(target: TabId) {
  if (target === tab.value) return;
  if (tab.value === 'prompts' && leaveGuard.value && !leaveGuard.value()) {
    const ok = await confirm({
      title: '有未保存的修改',
      message: '离开「提示词」页将丢失未保存的修改，确定离开吗？',
      confirmText: '丢弃并离开',
      danger: true,
    });
    if (!ok) return;
  }
  tab.value = target;
}

provide(managerKey, { data, refresh, toast, confirm, setLeaveGuard });

let unlisten: (() => void) | undefined;
let unlistenSync: (() => void) | undefined;

/** Ctrl+K 聚焦搜索：当前页没有搜索框时先切回提示词页 */
function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    if (tab.value !== 'prompts' && tab.value !== 'clipboard') {
      tab.value = 'prompts';
      nextTick(() => window.dispatchEvent(new CustomEvent('pm-focus-search')));
    } else {
      window.dispatchEvent(new CustomEvent('pm-focus-search'));
    }
  }
}

/** 自动同步失败的去重节流：同一错误 60s 内只提示一次，避免每个轮询周期刷屏 */
let lastAutoSyncErr = '';
let lastAutoSyncErrAt = 0;

/** 启动 3s 后自动检查更新（设置里可关）：有新版且未被用户跳过时弹通知条，可直达设置页 */
let updateTimer: ReturnType<typeof setTimeout> | undefined;

async function autoCheckUpdate() {
  try {
    if (data.value?.settings.autoUpdateCheck === false) return; // 用户关闭了自动检查
    const st = await api.checkUpdate();
    if (st.kind !== 'available') return;
    if (data.value?.settings.skippedUpdateVersion === st.version) return; // F8：跳过只管打扰
    lastAutoUpdate.value = st; // 交给设置页消费（F3）：点「去更新」后更新区直接展示，无需二次检查
    toast(`发现新版本 v${st.version}`, 'ok', {
      label: '去更新',
      handler: () => { tab.value = 'settings'; },
    }, 8000);
  } catch { /* 自动检查失败静默（F2） */ }
}

onMounted(async () => {
  document.addEventListener('keydown', onKeydown);
  await refresh();
  // 数据文件损坏的恢复提示：挂载后主动拉取（后端取后即清）。
  // 不用事件：窗口创建初期事件先于监听注册，必然丢失
  try {
    const notice = await api.getRecoveryNotice();
    if (notice) toast(notice, 'err', undefined, 8000);
  } catch {
    /* 忽略 */
  }
  unlisten = await listen('data-changed', refresh);
  // 云同步结果反馈（自动同步失败 / 托盘手动同步成败）
  unlistenSync = await listen<{ message: string; ok?: boolean; auto?: boolean }>(
    'sync-done',
    (e) => {
      const p = e.payload;
      if (!p?.message) return;
      if (p.ok === false) {
        if (p.auto) {
          const now = Date.now();
          if (p.message === lastAutoSyncErr && now - lastAutoSyncErrAt < 60000) return;
          lastAutoSyncErr = p.message;
          lastAutoSyncErrAt = now;
        }
        toast(`云同步失败：${p.message}`, 'err', undefined, 5000);
      } else {
        lastAutoSyncErr = '';
        toast(p.message);
      }
    },
  );
  // 启动自动检查更新：延后 3s，避免挤占首屏加载
  updateTimer = setTimeout(autoCheckUpdate, 3000);
});
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
  unlisten?.();
  unlistenSync?.();
  clearTimeout(updateTimer);
});

const tabs = [
  { id: 'prompts', label: '提示词', icon: FileText },
  { id: 'clipboard', label: '剪贴板', icon: ClipboardList },
  { id: 'sync', label: '云同步', icon: Cloud },
  { id: 'data', label: '数据', icon: HardDriveDownload },
  { id: 'settings', label: '设置', icon: Settings },
] as const;
</script>

<template>
  <div class="mg">
    <!-- 自定义标题栏：无框窗口的拖拽区 + 窗口控制（替代系统标题栏） -->
    <header
      class="tb"
      title="拖动移动窗口，双击最大化/还原"
      @mousedown="onTitlebarMouseDown"
      @dblclick="onTitlebarDoubleClick"
    >
      <span class="tb-name">Prompt Tool 提示词助手</span>
      <span class="grow" />
      <div class="tb-controls">
        <button class="tb-btn" title="最小化" aria-label="最小化" @click="winMinimize">
          <Minus :size="14" :stroke-width="1.8" />
        </button>
        <button class="tb-btn" title="最大化 / 还原" aria-label="最大化或还原" @click="winToggleMaximize">
          <Square :size="12" :stroke-width="1.8" />
        </button>
        <button class="tb-btn tb-close" title="关闭" aria-label="关闭" @click="winClose">
          <X :size="15" :stroke-width="1.8" />
        </button>
      </div>
    </header>

    <div class="mg-body">
      <!-- 图标栏 -->
      <aside class="mg-side">
        <div class="brand-mark" title="Prompt Tool">
          <Zap :size="17" :stroke-width="2.4" />
        </div>

        <nav class="mg-nav">
          <button
            v-for="t in tabs"
            :key="t.id"
            class="nav-btn"
            :class="{ on: tab === t.id }"
            :title="t.label"
            :aria-label="t.label"
            @click="switchTab(t.id)"
          >
            <component :is="t.icon" :size="18" :stroke-width="1.9" />
          </button>
        </nav>

        <div class="side-foot">
          <span class="sync-dot" :class="{ on: syncOn }" :title="syncLabel" />
        </div>
      </aside>

      <!-- 内容区 -->
      <main class="mg-main">
        <PromptsPane v-if="tab === 'prompts'" />
        <ClipboardPane v-else-if="tab === 'clipboard'" />
        <SyncPane v-else-if="tab === 'sync'" />
        <DataPane v-else-if="tab === 'data'" />
        <SettingsPane v-else />
      </main>
    </div>

    <div v-if="toastMsg" class="toast" :class="toastKind" aria-live="polite">
      {{ toastMsg }}
      <button
        v-if="toastAction"
        class="toast-action"
        @click="
          () => {
            const a = toastAction;
            toastMsg = '';
            toastAction = null;
            a?.handler();
          }
        "
      >
        {{ toastAction.label }}
      </button>
    </div>

    <ConfirmDialog
      :open="confirmOpen"
      :title="confirmOpts.title"
      :message="confirmOpts.message"
      :confirm-text="confirmOpts.confirmText"
      :danger="confirmOpts.danger"
      @confirm="settleConfirm(true)"
      @cancel="settleConfirm(false)"
    />
  </div>
</template>

<style scoped>
.mg {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg);
}

/* 自定义标题栏：拖拽区（双击最大化由 Tauri 拖拽区处理） */
.tb {
  display: flex;
  align-items: center;
  height: 36px;
  flex: none;
  padding: 0 6px 0 14px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-soft);
  user-select: none;
}

.tb-name {
  font-family: var(--font-display);
  font-size: 12px;
  letter-spacing: 0.03em;
  color: var(--muted);
}

.tb-controls {
  display: flex;
  gap: 2px;
}

.tb-btn {
  width: 40px;
  height: 26px;
  min-width: 0;
  padding: 0;
  border: none;
  background: transparent;
  border-radius: var(--r-xs);
  color: var(--muted);
}

.tb-btn:hover {
  background: var(--panel-2);
  color: var(--text);
}

.tb-close:hover {
  background: var(--danger-btn);
  border-color: transparent;
  color: #fff;
}

.mg-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

/* 图标栏：品牌标 + 图标导航 + 同步状态灯 */
.mg-side {
  width: 64px;
  flex: none;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 12px 0 14px;
  border-right: 1px solid var(--border);
  background: var(--bg-soft);
  gap: 6px;
}

.brand-mark {
  width: 34px;
  height: 34px;
  border-radius: var(--r-sm);
  display: grid;
  place-items: center;
  background: var(--brand-btn);
  color: var(--on-brand);
  margin-bottom: 14px;
  flex: none;
}

.mg-nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.nav-btn {
  position: relative;
  width: 40px;
  height: 40px;
  padding: 0;
  border: none;
  background: transparent;
  border-radius: var(--r-sm);
  color: var(--muted);
}

.nav-btn:hover {
  background: var(--panel-2);
  color: var(--text);
}

.nav-btn.on {
  background: var(--brand-soft);
  color: var(--brand);
}

.nav-btn.on::before {
  content: '';
  position: absolute;
  left: -12px;
  top: 9px;
  bottom: 9px;
  width: 3px;
  border-radius: 3px;
  background: var(--brand);
}

.side-foot {
  margin-top: auto;
  padding-top: 8px;
}

.sync-dot {
  display: block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--faint);
  cursor: default;
  transition: background var(--t-med), box-shadow var(--t-med);
}

.sync-dot.on {
  background: var(--ok);
  box-shadow: 0 0 8px var(--ok);
}

.mg-main {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

/* toast 靠右下，避开编辑器底部操作区 */
.mg .toast {
  left: auto;
  right: 18px;
  transform: none;
  bottom: 18px;
}

@starting-style {
  .mg .toast {
    opacity: 0;
    transform: translateY(8px);
  }
}

.toast-action {
  margin-left: 10px;
  padding: 2px 12px;
  font-size: 12px;
  font-weight: 600;
  border-radius: 999px;
  background: var(--brand-soft);
  border-color: transparent;
  color: var(--brand);
}
</style>
