<script setup lang="ts">
/** 主操作按钮：全应用唯一的实心墨块（亮=碳墨 / 暗=骨白），无投影无渐变 */
defineProps<{ disabled?: boolean }>();
</script>

<template>
  <button class="ab" :disabled="disabled">
    <slot />
  </button>
</template>

<style scoped>
.ab {
  background: var(--brand-btn);
  border: 1px solid transparent;
  color: var(--on-brand);
  font-weight: 600;
  padding: 7px 18px;
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: background var(--t-fast), transform var(--t-fast);
}

.ab:hover {
  background: var(--brand-btn-hover);
}

.ab:active {
  transform: scale(0.97);
}

.ab:disabled {
  opacity: 0.45;
  pointer-events: none;
}
</style>
