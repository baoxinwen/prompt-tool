<script setup lang="ts">
/** 分段选择器 */
defineProps<{
  options: { id: string; label: string }[];
  modelValue: string;
  /** 整组禁用（如同步进行中禁切 provider，评审 M3#4） */
  disabled?: boolean;
}>();
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>();
</script>

<template>
  <div class="seg" :class="{ disabled }" role="tablist">
    <button
      v-for="o in options"
      :key="o.id"
      type="button"
      class="seg-item"
      :class="{ on: modelValue === o.id }"
      :disabled="disabled"
      :aria-pressed="modelValue === o.id"
      @click="emit('update:modelValue', o.id)"
    >
      {{ o.label }}
    </button>
  </div>
</template>

<style scoped>
/* 轨道：凹槽井（铁律 1：无边框，内阴影下压，与全局输入井同族） */
.seg {
  display: inline-flex;
  gap: 2px;
  padding: 3px;
  background: var(--input-bg);
  border-radius: var(--r-sm);
  box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.05);
}

/* 轨道内不做浮起键：拍平，选中项以「按住的键」表达 */
.seg-item {
  border: none;
  background: transparent;
  padding: 5px 16px;
  border-radius: var(--r-xs);
  color: var(--muted);
  font-size: 13px;
  box-shadow: none;
  transform: none;
  transition: background var(--t-fast), color var(--t-fast), box-shadow var(--t-fast);
}

.seg-item:hover {
  color: var(--text);
  background: transparent;
}

.seg-item.on {
  background: var(--panel-3);
  color: var(--text);
  font-weight: var(--fw-600);
  box-shadow: inset 0 2px 4px rgba(17, 17, 17, 0.08);
}

.seg.disabled {
  opacity: 0.55;
  pointer-events: none;
}
</style>
