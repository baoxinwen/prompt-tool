<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { Zap, X } from 'lucide-vue-next';
import { api } from '../lib/api';
import { emptyPrompt } from '../types';
import type { AppData } from '../types';
import AccentButton from './ui/AccentButton.vue';

const data = ref<AppData | null>(null);
const title = ref('');
const content = ref('');
// 空串 = 未定：拿到分类列表后回退第一个分类（避免永远落在「未分类」）
const category = ref('');
const saving = ref(false);
const saved = ref(false);
const saveErr = ref('');
const titleInput = ref<HTMLInputElement | null>(null);

async function load() {
  try {
    data.value = await api.getData();
    // 默认分类 = 第一个分类；用户手选后（truthy）不被覆盖，跨次捕获保留
    if (!category.value) {
      category.value = data.value?.categories[0] ?? '未分类';
    }
  } catch {
    /* 忽略 */
  }
}

function reset(text: string) {
  content.value = text;
  const firstLine = text.split('\n')[0]?.trim() ?? '';
  title.value = firstLine.slice(0, 20);
  if (!category.value) category.value = data.value?.categories[0] ?? '未分类';
}

async function save() {
  if (saving.value || saved.value) return;
  if (!content.value.trim()) {
    closeWindow();
    return;
  }
  saving.value = true;
  saveErr.value = '';
  try {
    const cat = category.value || data.value?.categories[0] || '未分类';
    await api.savePrompt({
      ...emptyPrompt(cat),
      title: title.value.trim() || content.value.trim().slice(0, 20),
      content: content.value,
    });
    // 反馈停留约 0.6s 再关窗：本地保存极快，「保存中」一闪而过会被读成「没显示全」
    saved.value = true;
    setTimeout(closeWindow, 600);
  } catch (e) {
    // 窗口保持打开，把失败显式告诉用户，避免误以为已保存
    saveErr.value = String(e);
  } finally {
    saving.value = false;
  }
}

function closeWindow() {
  api.closeCapture();
}

function onKeydown(e: KeyboardEvent) {
  // 输入法组合态的 Esc 是取消候选词，不是关闭窗口（评审 I2）
  if (e.isComposing || e.keyCode === 229) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    closeWindow();
  } else if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'Enter')) {
    e.preventDefault();
    save();
  }
}

let unlistenText: (() => void) | undefined;
let unlistenData: (() => void) | undefined;

onMounted(async () => {
  await load();
  titleInput.value?.focus();
  // 键盘语义挂 window 而非组件根元素：点击非可聚焦区后焦点落 body，
  // keydown 不经过组件子树，Esc/Ctrl+S 会静默失效（评审 I5）
  window.addEventListener('keydown', onKeydown);
  unlistenText = await listen<string>('capture-text', (e) => {
    reset(e.payload ?? '');
    titleInput.value?.focus();
  });
  unlistenData = await listen('data-changed', load);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
  unlistenText?.();
  unlistenData?.();
});
</script>

<template>
  <div class="cv">
    <div class="cv-head">
      <span class="cv-logo"><Zap :size="12" :stroke-width="2.4" /></span>
      <span class="cv-title">快速捕获</span>
      <span class="grow" />
      <button class="icon-x" title="关闭" aria-label="关闭快速捕获" @click="closeWindow()">
        <X :size="14" />
      </button>
    </div>
    <input
      ref="titleInput"
      v-model="title"
      class="cv-title-input"
      type="text"
      placeholder="标题（默认取首行）"
      spellcheck="false"
    />
    <textarea
      v-model="content"
      class="cv-content mono"
      placeholder="选中文本会自动填到这里，也可手动输入"
      spellcheck="false"
    />
    <div v-if="saveErr" class="cv-err-row" role="alert">{{ saveErr }}</div>
    <div class="cv-foot">
      <select v-model="category">
        <option v-for="c in data?.categories ?? []" :key="c" :value="c">{{ c }}</option>
        <option value="未分类">未分类</option>
      </select>
      <span class="grow" />
      <button class="ghost-btn" @click="closeWindow()">取消</button>
      <AccentButton
        :disabled="saving || saved"
        title="快捷键：Ctrl+S 或 Ctrl+Enter"
        @click="save"
      >
        {{ saving ? '保存中…' : saved ? '✓ 已保存' : '保存' }}
        <span v-if="!saving && !saved" class="cv-kbds"><kbd>Ctrl S</kbd></span>
      </AccentButton>
    </div>
  </div>
</template>

<style scoped>
.cv {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--panel);
  border: 1px solid var(--border-strong);
  /* 圆角对齐 Win11 对无框窗口的 DWM 圆角（8px），画布同色见 base.css
     body[data-window]——与快捷面板 .qp 同款，避免角落双曲线与异色楔形 */
  border-radius: var(--r-sm);
  overflow: hidden;
  gap: 9px;
}

.cv-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 11px 12px 0;
}

.cv-logo {
  width: 22px;
  height: 22px;
  border-radius: 7px;
  display: grid;
  place-items: center;
  background: var(--brand-btn);
  color: var(--on-brand);
  flex: none;
}

.cv-title {
  font-weight: 650;
  font-size: 13px;
}

.grow {
  flex: 1;
}

.icon-x {
  width: 26px;
  height: 26px;
  padding: 0;
  border-color: transparent;
  background: transparent;
  color: var(--muted);
}

.cv-title-input {
  margin: 0 12px;
  font-size: 13.5px;
  padding: 7px 10px;
}

.cv-content {
  flex: 1;
  min-height: 0;
  resize: none;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  margin: 0 12px;
  background: var(--bg-soft);
}

.cv-foot {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  row-gap: 8px;
  gap: 8px;
  padding: 0 12px 12px;
}

.cv-foot > * {
  flex: none;
}

/* 弹性占位例外：负责把按钮组推到右侧（修复上面通配的误伤） */
.cv-foot > .grow {
  flex: 1 1 auto;
}

.cv-foot .ab kbd {
  margin-left: 7px;
  /* 随主按钮反色自适应：亮=墨底白键帽，暗=骨白底炭键帽 */
  background: color-mix(in srgb, var(--on-brand) 14%, transparent);
  border-color: color-mix(in srgb, var(--on-brand) 28%, transparent);
  box-shadow: none;
  color: var(--on-brand);
  font-size: 9.5px;
  padding: 2px 5px;
}

select {
  font-size: 12px;
  padding: 5px 8px;
}

.cv-foot .ab {
  flex: none;
}

.cv-foot .ab:disabled {
  color: var(--ok);
  background: var(--ok-soft);
  border-color: transparent;
}

.ghost-btn {
  background: transparent;
  font-size: 12px;
  color: var(--muted);
}

.cv-err-row {
  margin: 0 12px;
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--danger);
  word-break: break-all;
  user-select: text;
}

.ghost-btn:hover {
  color: var(--text);
}
</style>
