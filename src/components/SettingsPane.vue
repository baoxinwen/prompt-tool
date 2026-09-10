<script setup lang="ts">
import { inject, onMounted, ref } from 'vue';
import { Monitor, Moon, Sun, FolderOpen, Keyboard, Palette, RefreshCw, SlidersHorizontal } from 'lucide-vue-next';
import { api } from '../lib/api';
import { enqueueSettingsSave } from '../lib/settingsSave';
import { managerKey } from '../lib/context';
import HotkeyInput from './HotkeyInput.vue';
import UpdateSection from './UpdateSection.vue';

const ctx = inject(managerKey)!;

// 由 vite define 注入，与 package.json / tauri.conf.json 同源
const appVersion = __APP_VERSION__;

const hotkey = ref('');
const captureHotkey = ref('');
const restoreClipboard = ref(true);
const pasteAppendEnter = ref(false);
const captureClipboard = ref(true);
const theme = ref('dark');
const autostart = ref(false);
const autoUpdateCheck = ref(true);

async function load() {
  const s = ctx.data.value?.settings;
  if (s) {
    hotkey.value = s.hotkey;
    captureHotkey.value = s.captureHotkey;
    restoreClipboard.value = s.restoreClipboard;
    pasteAppendEnter.value = s.pasteAppendEnter;
    captureClipboard.value = s.captureClipboard;
    theme.value = ['light', 'auto'].includes(s.theme) ? s.theme : 'dark';
    autoUpdateCheck.value = s.autoUpdateCheck !== false;
  }
  try {
    autostart.value = await api.getAutostart();
  } catch {
    /* 忽略 */
  }
}

/** @returns 保存是否成功：乐观更新（先翻转本地开关再保存）的调用方在失败时
 *  必须回滚 UI——catch 只 toast 不回滚会让开关与持久化状态持续分叉，
 *  下次任意设置的全量写回还会把该变更静默丢弃（评审 2026-09-10 I6） */
async function saveSettingsPart(mutate: (s: import('../types').Settings) => void, okMsg: string): Promise<boolean> {
  // 串行化：两次快速变更交叠时，后发者必须等前一次 refresh 完成后再读快照，
  // 否则会用旧快照整体写回，把已持久化的变更静默回滚（评审 I3）
  return enqueueSettingsSave(async (): Promise<boolean> => {
    const s = ctx.data.value?.settings;
    if (!s) return false;
    try {
      const next = JSON.parse(JSON.stringify(s)) as import('../types').Settings;
      mutate(next);
      await api.saveSettings(next);
      await ctx.refresh();
      ctx.toast(okMsg);
      return true;
    } catch (e) {
      ctx.toast(String(e), 'err');
      return false;
    }
  });
}

function onHotkeyChange(v: string) {
  const prev = hotkey.value;
  hotkey.value = v;
  void saveSettingsPart((s) => (s.hotkey = v), '主快捷键已更新').then((ok) => {
    if (!ok) hotkey.value = prev;
  });
}

function onCaptureHotkeyChange(v: string) {
  const prev = captureHotkey.value;
  captureHotkey.value = v;
  void saveSettingsPart((s) => (s.captureHotkey = v), '捕获快捷键已更新').then((ok) => {
    if (!ok) captureHotkey.value = prev;
  });
}

function onThemeChange(v: string) {
  const prev = theme.value;
  theme.value = v;
  void saveSettingsPart((s) => (s.theme = v), '主题已更新').then((ok) => {
    if (!ok) theme.value = prev;
  });
}

function onRestoreClipboardChange() {
  const prev = restoreClipboard.value;
  const next = !prev;
  restoreClipboard.value = next;
  void saveSettingsPart(
    (s) => (s.restoreClipboard = next),
    next ? '已开启粘贴后恢复剪贴板' : '已关闭粘贴后恢复剪贴板',
  ).then((ok) => {
    if (!ok) restoreClipboard.value = prev;
  });
}

function onPasteAppendEnterChange() {
  const prev = pasteAppendEnter.value;
  const next = !prev;
  pasteAppendEnter.value = next;
  void saveSettingsPart(
    (s) => (s.pasteAppendEnter = next),
    next ? '已开启粘贴后自动回车' : '已关闭粘贴后自动回车',
  ).then((ok) => {
    if (!ok) pasteAppendEnter.value = prev;
  });
}

function onCaptureClipboardChange() {
  const prev = captureClipboard.value;
  const next = !prev;
  captureClipboard.value = next;
  void saveSettingsPart((s) => (s.captureClipboard = next), next ? '已开启剪贴板记录' : '已关闭剪贴板记录').then((ok) => {
    if (!ok) captureClipboard.value = prev;
  });
}

function onAutoUpdateCheckChange() {
  const prev = autoUpdateCheck.value;
  const next = !prev;
  autoUpdateCheck.value = next;
  void saveSettingsPart((s) => (s.autoUpdateCheck = next), next ? '已开启自动检查更新' : '已关闭自动检查更新').then((ok) => {
    if (!ok) autoUpdateCheck.value = prev;
  });
}

async function toggleAutostart() {
  try {
    await api.setAutostart(!autostart.value);
    autostart.value = !autostart.value;
    ctx.toast(autostart.value ? '已开启开机自启' : '已关闭开机自启');
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

const themes = [
  { id: 'dark', label: '暗色', icon: Moon, cls: 'th-dark' },
  { id: 'light', label: '亮色', icon: Sun, cls: 'th-light' },
  { id: 'auto', label: '跟随系统', icon: Monitor, cls: 'th-auto' },
] as const;

onMounted(() => {
  load();
});
</script>

<template>
  <div class="st">
    <header class="st-head">
      <h2 class="st-title">设置</h2>
      <span class="grow" />
    </header>

    <div class="st-body">
      <div class="st-card">
        <div class="sec-head">
          <span class="sec-ico"><Keyboard :size="14" /></span>
          <h3>快捷键</h3>
        </div>
        <label class="opt">
          <span>
            呼出快捷面板
            <small class="faint">在任何应用中按下即可呼出面板，Esc 或再按一次隐藏</small>
          </span>
          <HotkeyInput :model-value="hotkey" @update:model-value="onHotkeyChange" />
        </label>
        <label class="opt">
          <span>
            快速捕获
            <small class="faint">选中文本后按下，直接把选中内容保存为新提示词</small>
          </span>
          <HotkeyInput :model-value="captureHotkey" @update:model-value="onCaptureHotkeyChange" />
        </label>
      </div>

      <div class="st-card">
        <div class="sec-head">
          <span class="sec-ico"><Palette :size="14" /></span>
          <h3>外观</h3>
        </div>
        <div class="theme-cards">
          <button
            v-for="t in themes"
            :key="t.id"
            class="theme-card"
            :class="[{ on: theme === t.id }, t.cls]"
            @click="onThemeChange(t.id)"
          >
            <div class="th-preview"><span class="th-bar" /><span class="th-line" /><span class="th-line short" /></div>
            <span class="th-label"><component :is="t.icon" :size="13" /> {{ t.label }}</span>
          </button>
        </div>
      </div>

      <div class="st-card">
        <div class="sec-head">
          <span class="sec-ico"><SlidersHorizontal :size="14" /></span>
          <h3>通用</h3>
        </div>
        <label class="opt">
          <span>
            粘贴后恢复原剪贴板
            <small class="faint">自动粘贴完成后，把你之前复制的内容悄悄放回剪贴板</small>
          </span>
          <span class="switch">
            <input type="checkbox" :checked="restoreClipboard" @change="onRestoreClipboardChange" />
            <span class="track"><span class="thumb" /></span>
          </span>
        </label>
        <label class="opt">
          <span>
            粘贴后自动回车
            <small class="faint">粘贴到网页 AI 对话框后自动发送；代码编辑器场景建议关闭</small>
          </span>
          <span class="switch">
            <input type="checkbox" :checked="pasteAppendEnter" @change="onPasteAppendEnterChange" />
            <span class="track"><span class="thumb" /></span>
          </span>
        </label>
        <label class="opt">
          <span>
            开机自动启动
            <small class="faint">登录 Windows 后在后台运行（托盘图标）</small>
          </span>
          <span class="switch">
            <input type="checkbox" :checked="autostart" @change="toggleAutostart" />
            <span class="track"><span class="thumb" /></span>
          </span>
        </label>
        <label class="opt">
          <span>
            记录剪贴板历史
            <small class="faint">自动记录系统中新复制的文本与图片</small>
          </span>
          <span class="switch">
            <input type="checkbox" :checked="captureClipboard" @change="onCaptureClipboardChange" />
            <span class="track"><span class="thumb" /></span>
          </span>
        </label>
        <label class="opt">
          <span>
            自动检查更新
            <small class="faint">启动后联网检查新版本（仅匿名请求，不上传任何数据）</small>
          </span>
          <span class="switch">
            <input type="checkbox" :checked="autoUpdateCheck" @change="onAutoUpdateCheckChange" />
            <span class="track"><span class="thumb" /></span>
          </span>
        </label>
      </div>

      <div class="st-card">
        <div class="sec-head">
          <span class="sec-ico"><FolderOpen :size="14" /></span>
          <h3>数据</h3>
        </div>
        <label class="opt">
          <span>数据目录<small class="faint">data.json 保存了全部提示词与配置</small></span>
          <button
            @click="api.openDataDir().catch((e) => ctx.toast(String(e), 'err'))"
          ><FolderOpen :size="13" /> 打开目录</button>
        </label>
      </div>

      <div class="st-card">
        <div class="sec-head">
          <span class="sec-ico"><RefreshCw :size="14" /></span>
          <h3>关于与更新</h3>
        </div>
        <UpdateSection />
      </div>

      <div class="about faint">
        <span class="mono">Prompt Tool</span> · <span class="tnum">v{{ appVersion }}</span> · 本地优先的提示词管理工具
      </div>
    </div>
  </div>
</template>

<style scoped>
.st {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.st-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 18px;
  height: 52px;
  flex: none;
  border-bottom: 1px solid var(--border);
}

.st-title {
  font-size: var(--fs-lg);
  font-weight: 650;
  letter-spacing: -0.01em;
}

.st-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 18px 22px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-width: 680px;
  width: 100%;
  /* 限宽列在宽窗口下水平居中，避免右侧大片留白 */
  margin: 0 auto;
}

.st-card {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 6px 18px 12px;
}

.sec-head {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 12px 0 10px;
}

.sec-ico {
  width: 26px;
  height: 26px;
  border-radius: 7px;
  display: grid;
  place-items: center;
  background: var(--brand-soft);
  color: var(--brand);
  flex: none;
}

h3 {
  font-size: 13.5px;
  font-weight: 650;
}

.opt {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 11px 0;
  border-top: 1px solid var(--border);
  cursor: pointer;
}

.st-card .opt:first-of-type {
  border-top: none;
}

.opt span:first-child {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 13px;
}

.opt small {
  font-size: 11.5px;
  font-weight: 400;
}

.theme-cards {
  display: flex;
  gap: 10px;
  padding: 4px 0 10px;
}

.theme-card {
  flex: 1;
  flex-direction: column;
  align-items: stretch;
  gap: 0;
  padding: 0;
  overflow: hidden;
  border: 1.5px solid var(--border);
  border-radius: var(--r-md);
}

.theme-card:hover {
  border-color: var(--border-strong);
  background: var(--panel);
}

.theme-card.on {
  border-color: var(--brand);
  box-shadow: 0 0 0 3px var(--brand-soft);
}

.th-preview {
  height: 64px;
  padding: 11px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.th-dark .th-preview {
  background: #161511;
}

.th-light .th-preview {
  background: #f8f7f4;
}

.th-auto .th-preview {
  background: linear-gradient(105deg, #161511 50%, #f8f7f4 50%);
}

/* 主题预览条：写死目标主题的强调色，不随页面主题变化（M8.4） */
.th-bar {
  width: 55%;
  height: 8px;
  border-radius: 4px;
  background: #8b877a;
}

.th-dark .th-bar {
  background: #d8d3c4;
}

.th-light .th-bar {
  background: #201e19;
}

.th-line {
  width: 85%;
  height: 6px;
  border-radius: 3px;
  background: rgba(128, 128, 148, 0.35);
}

.th-line.short {
  width: 55%;
}

.th-label {
  display: flex;
  align-items: center;
  gap: 6px;
  justify-content: center;
  padding: 7px 0;
  font-size: 12px;
  color: var(--text-2);
  border-top: 1px solid var(--border);
  background: var(--panel-2);
}

.theme-card.on .th-label {
  color: var(--brand);
  font-weight: 600;
}

.about {
  font-size: 11.5px;
  padding: 2px 0 12px;
}
</style>
