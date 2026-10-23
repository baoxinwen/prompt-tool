<script setup lang="ts">
/** 主操作按钮：全应用唯一的实心墨块（亮=碳墨 / 暗=骨白），无投影无渐变 */
defineProps<{ disabled?: boolean }>();
</script>

<template>
  <button class="ab" type="button" :disabled="disabled">
    <slot />
  </button>
</template>

<style scoped>
/* 黑键三态：浮起（渐变墨面 + 实体投影）→ 悬停加亮上浮 → 按下下沉内陷
   （铁律 1：无边框，轮廓与厚度交给投影） */
.ab {
  background: linear-gradient(180deg, var(--brand-btn) 0%, var(--brand-strong) 100%);
  color: var(--on-brand);
  font-weight: var(--fw-600);
  padding: 7px 18px;
  border-radius: var(--r-key);
  cursor: pointer;
  box-shadow: var(--shadow-brand);
  transition: background var(--t-fast), transform var(--t-spring), box-shadow var(--t-spring);
}

.ab:hover {
  background: linear-gradient(180deg, var(--brand-btn-hover) 0%, var(--brand-strong) 100%);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.16), 0 1.5px 0 var(--edge-strong),
    0 4px 10px rgba(17, 17, 17, 0.24);
}

.ab:active {
  transform: translateY(1px);
  box-shadow: inset 0 2px 5px rgba(0, 0, 0, 0.35), 0 0 0 var(--edge-strong);
}

.ab:disabled {
  opacity: 1;
  background: var(--panel-2);
  color: var(--muted);
  box-shadow: none;
  transform: none;
  cursor: default;
  pointer-events: none;
}
</style>
