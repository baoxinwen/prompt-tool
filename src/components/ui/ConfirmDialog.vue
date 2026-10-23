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
  // 键盘自动重复不得驱动确认/取消：按住 Enter 时第一次 keydown 打开对话框后，
  // 焦点已被 watch 移入确认键，auto-repeat 的 Enter 会在用户看清对话框前
  // 直接「确认」危险操作（评审 2026-09-10 I8）。Tab 除外：长按循环焦点合法
  if (e.repeat && e.key !== 'Tab') return;
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
  font-weight: var(--fw-650);
  font-size: var(--fs-lg);
  flex: 1;
}

/* 关闭键：ghost 物理——无阴影不上浮，悬停只换面 */
.cd-x {
  width: 26px;
  height: 26px;
  padding: 0;
  background: transparent;
  color: var(--muted);
  box-shadow: none;
  transform: none;
}

.cd-x:hover {
  background: var(--panel-2);
  color: var(--text);
  transform: none;
  box-shadow: none;
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

/* 确认键：黑键（与全局 button.primary 同物理——浮起/悬停加亮/按压内陷） */
.cd-confirm {
  background: linear-gradient(180deg, var(--brand-btn) 0%, var(--brand-strong) 100%);
  color: var(--on-brand);
  font-weight: var(--fw-600);
  box-shadow: var(--shadow-brand);
}

.cd-confirm:hover {
  background: linear-gradient(180deg, var(--brand-btn-hover) 0%, var(--brand-strong) 100%);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.16), 0 1.5px 0 var(--edge-strong),
    0 4px 10px rgba(17, 17, 17, 0.24);
}

.cd-confirm:active {
  transform: translateY(1px);
  box-shadow: inset 0 2px 5px rgba(0, 0, 0, 0.35), 0 0 0 var(--edge-strong);
}

/* 危险确认：走全局 danger 键语义——红字 + 悬停红染（实体键物理保留） */
.cd-confirm.danger {
  background: linear-gradient(180deg, var(--key-1) 0%, var(--key-2) 100%);
  color: var(--danger);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.55), 0 1.5px 0 var(--edge),
    0 2px 4px rgba(17, 17, 17, 0.06);
}

.cd-confirm.danger:hover {
  background: var(--danger-soft);
  color: var(--danger);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.55), 0 1.5px 0 var(--edge-strong),
    0 4px 10px rgba(17, 17, 17, 0.1);
}

.cd-confirm.danger:active {
  transform: translateY(1px);
  box-shadow: inset 0 2px 4px rgba(17, 17, 17, 0.14), 0 0 0 var(--edge);
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
