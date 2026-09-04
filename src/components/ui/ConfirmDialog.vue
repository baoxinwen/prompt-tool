<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { AlertTriangle, X } from 'lucide-vue-next';

const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    message?: string;
    confirmText?: string;
    cancelText?: string;
    danger?: boolean;
  }>(),
  { confirmText: '确认', cancelText: '取消', danger: false, message: '' },
);

const emit = defineEmits<{ (e: 'confirm'): void; (e: 'cancel'): void }>();

const maskRef = ref<HTMLElement | null>(null);
const confirmBtn = ref<HTMLButtonElement | null>(null);

function onKeydown(e: KeyboardEvent) {
  if (!props.open) return;
  // 模态打开期间在捕获阶段拦截一切按键并向下游传播：
  // 背景快捷键（Ctrl+K 聚焦搜索、Ctrl+S 保存等）全部失效，
  // 否则焦点被切到遮罩后的输入框时，用户按 Enter 想执行搜索
  // 会被这里的捕获监听解释成「确认」（评审 I5）
  e.stopPropagation();
  if (e.key === 'Escape') {
    e.preventDefault();
    emit('cancel');
  } else if (e.key === 'Tab') {
    // 焦点陷阱：Tab/Shift+Tab 仅在对话框内的按钮间循环，
    // 让 aria-modal="true" 名副其实
    e.preventDefault();
    const buttons = Array.from(
      maskRef.value?.querySelectorAll<HTMLElement>('button') ?? [],
    );
    if (!buttons.length) return;
    const idx = buttons.indexOf(document.activeElement as HTMLElement);
    const next = e.shiftKey
      ? buttons[(idx - 1 + buttons.length) % buttons.length]
      : buttons[(idx + 1) % buttons.length];
    next.focus();
  } else if (e.key === 'Enter') {
    // 确定性处理，不依赖浏览器原生按钮激活：焦点在取消/关闭键上 =
    // 取消；其余情况（含焦点不在按钮上）= 确认，与底栏「Enter 确认」
    // 提示一致
    e.preventDefault();
    const active = document.activeElement as HTMLElement | null;
    const onOtherButton =
      active !== null &&
      active.tagName === 'BUTTON' &&
      !!maskRef.value?.contains(active) &&
      !active.classList.contains('cd-confirm');
    if (onOtherButton) {
      emit('cancel');
    } else {
      emit('confirm');
    }
  }
}

// 打开时把焦点移入对话框（默认落在确认键上，Enter 即确认）
watch(
  () => props.open,
  (open) => {
    if (open) nextTick(() => confirmBtn.value?.focus());
  },
  { immediate: true },
);

onMounted(() => window.addEventListener('keydown', onKeydown, true));
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown, true));
</script>

<template>
  <Teleport to="body">
    <Transition name="cd">
      <div v-if="open" ref="maskRef" class="cd-mask" @mousedown.self="emit('cancel')">
        <div class="cd-card" role="alertdialog" aria-modal="true">
          <div class="cd-head">
            <span v-if="danger" class="cd-warn-ico"><AlertTriangle :size="15" /></span>
            <span class="cd-title">{{ title }}</span>
            <button class="cd-x" aria-label="关闭" @click="emit('cancel')">
              <X :size="14" />
            </button>
          </div>
          <div v-if="message" class="cd-msg">{{ message }}</div>
          <div class="cd-foot">
            <button class="cd-btn" @click="emit('cancel')">{{ cancelText }} <kbd>Esc</kbd></button>
            <button
              ref="confirmBtn"
              class="cd-btn cd-confirm"
              :class="{ danger }"
              @click="emit('confirm')"
            >
              {{ confirmText }} <kbd>Enter</kbd>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.cd-mask {
  position: fixed;
  inset: 0;
  background: var(--mask-bg);
  display: grid;
  place-items: center;
  z-index: var(--z-overlay, 40);
}

.cd-card {
  width: min(400px, calc(100vw - 48px));
  background: var(--panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow-2);
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.cd-head {
  display: flex;
  align-items: center;
  gap: 9px;
}

.cd-warn-ico {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--danger-soft);
  color: var(--danger);
  flex: none;
}

.cd-title {
  font-weight: 650;
  font-size: var(--fs-lg);
  flex: 1;
}

.cd-x {
  width: 26px;
  height: 26px;
  padding: 0;
  border-color: transparent;
  background: transparent;
  color: var(--muted);
}

.cd-msg {
  color: var(--text-2);
  font-size: var(--fs-base);
  line-height: 1.6;
}

.cd-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}

.cd-btn {
  padding: 7px 14px;
  font-size: var(--fs-base);
}

.cd-btn kbd {
  margin-left: 6px;
  opacity: 0.75;
}

.cd-confirm {
  background: var(--panel-2);
  font-weight: 600;
}

.cd-confirm.danger {
  background: var(--danger-btn);
  border-color: transparent;
  color: #fff;
}

.cd-confirm.danger:hover {
  background: var(--danger);
}

/* 过渡：200ms ease-out，scale 0.96 + opacity */
.cd-enter-active,
.cd-leave-active {
  transition: opacity 200ms var(--ease);
}

.cd-enter-active .cd-card,
.cd-leave-active .cd-card {
  transition: transform 200ms var(--ease);
}

.cd-enter-from,
.cd-leave-to {
  opacity: 0;
}

.cd-enter-from .cd-card,
.cd-leave-to .cd-card {
  transform: scale(0.96);
}
</style>
