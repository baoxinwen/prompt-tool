<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { Eraser } from 'lucide-vue-next';
import KeyCap from './ui/KeyCap.vue';
import { hotkeyHint } from '../lib/platform';

defineProps<{ modelValue: string; placeholder?: string }>();
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>();

const capturing = ref(false);

/** Shift 按下时标点键的 e.key 是变体符号（? : ~ < > 等），
 *  Rust 侧 global-hotkey 解析器只认基础符号（/ . , ; 等，见其 parse_key 支持表）；
 *  按 e.code 归一成基础符号，既可注册又保持 KeyCap 显示直观（评审 I4） */
const PUNCT_BY_CODE: Record<string, string> = {
  Slash: '/',
  Period: '.',
  Comma: ',',
  Semicolon: ';',
  Quote: "'",
  Backslash: '\\',
  BracketLeft: '[',
  BracketRight: ']',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
};

function onCaptureKeydown(e: KeyboardEvent) {
  if (!capturing.value) return;
  e.preventDefault();
  e.stopPropagation();
  // 输入法组合态的按键（keyCode 229 / key 为 Process）不是快捷键意图（评审 I2）
  if (e.isComposing || e.keyCode === 229) return;
  if (e.key === 'Escape') {
    capturing.value = false;
    return;
  }
  if (['Alt', 'Control', 'Shift', 'Meta'].includes(e.key)) return;
  if (!e.altKey && !e.ctrlKey && !e.metaKey) {
    return; // 必须带修饰键，忽略纯字母/数字按键
  }
  // 系统保留组合键不允许绑定（否则 Alt+F4 会被全局快捷键抢走，关不掉窗口）
  if (e.altKey && e.key === 'F4') return;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('ctrl');
  if (e.altKey) parts.push('alt');
  if (e.metaKey) parts.push('super');
  if (e.shiftKey) parts.push('shift');
  let key = e.key;
  // 数字键用 e.code 归一：Shift+数字时 e.key 是符号（如 !），accelerator 无法注册
  if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  else if (PUNCT_BY_CODE[e.code]) key = PUNCT_BY_CODE[e.code];
  else if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^[a-z]$/i.test(key)) key = key.toUpperCase();
  else if (/^F\d{1,2}$/.test(key)) key = key.toUpperCase();
  else if (key === ' ') key = 'space';
  else if (key.startsWith('Arrow')) key = key.toLowerCase();
  parts.push(key);
  capturing.value = false;
  emit('update:modelValue', parts.join('+'));
}

function clear() {
  emit('update:modelValue', '');
}

onMounted(() => window.addEventListener('keydown', onCaptureKeydown, true));
onBeforeUnmount(() => window.removeEventListener('keydown', onCaptureKeydown, true));
</script>

<template>
  <span class="hi">
    <span
      class="key-slot"
      :class="{ capturing, empty: !modelValue }"
      role="button"
      tabindex="0"
      aria-label="点击录制快捷键"
      @click="capturing = true"
      @keydown.enter="capturing = true"
      @keydown.space.prevent="capturing = true"
    >
      <KeyCap v-if="modelValue" :combo="modelValue" />
      <span v-else class="key-empty">{{ capturing ? '按下新组合，Esc 取消' : '未设置 · 点击录制' }}</span>
    </span>
    <button class="sm" type="button" @click="capturing = true">修改</button>
    <button
      v-if="modelValue"
      class="sm ghost eraser"
      type="button"
      title="清除快捷键"
      aria-label="清除快捷键"
      @click="clear"
    >
      <Eraser :size="14" :stroke-width="1.8" />
    </button>
  </span>

  <Teleport to="body">
    <!-- 遮罩用 click 而非 mousedown 关闭（评审 2026-09-10 I9）：本浮层无离场过渡，
         mousedown 瞬间移除会让 mouseup 的 click 落到底层 key-slot（浮层关了又弹开）
         或清除按钮（误清快捷键）；存活到完整 click 时鼠标事件始终落在遮罩上 -->
    <div v-if="capturing" class="cap-mask" @click.self="capturing = false">
      <div class="cap-card fade-up">
        <div class="cap-title">请按下新的快捷键</div>
        <div class="cap-hint mono">{{ placeholder || hotkeyHint }}</div>
        <div class="cap-keys"><kbd>Esc</kbd> 取消</div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.hi {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.key-slot {
  display: inline-flex;
  align-items: center;
  min-width: 150px;
  min-height: 34px;
  padding: 4px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--r-sm);
  background: var(--input-bg);
  cursor: pointer;
  transition: border-color var(--t-fast), background var(--t-fast);
}

.key-slot:hover {
  border-color: var(--brand);
}

.key-slot.capturing {
  border-color: var(--brand);
  box-shadow: 0 0 0 3px var(--brand-soft);
}

.key-slot.empty {
  color: var(--faint);
}

.key-empty {
  font-size: 12px;
  width: 100%;
  text-align: center;
}

.sm {
  padding: 4px 10px;
  font-size: 12px;
}

/* 橡皮擦清除钮：26px 方形命中区（M8.1） */
.eraser {
  width: 26px;
  height: 26px;
  min-width: 26px;
  padding: 0;
  justify-content: center;
}

.cap-mask {
  position: fixed;
  inset: 0;
  background: var(--mask-bg);
  display: grid;
  place-items: center;
  z-index: var(--z-capture, 60);
}

.cap-card {
  background: var(--panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--r-md);
  padding: 24px 36px;
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 8px;
  box-shadow: var(--shadow-2);
}

.cap-title {
  font-size: var(--fs-lg);
  font-weight: 650;
}

.cap-hint {
  font-size: 11.5px;
  color: var(--muted);
}

.cap-keys {
  margin-top: 6px;
  font-size: 12px;
  color: var(--muted);
}

.cap-keys kbd {
  margin-right: 4px;
}
</style>
