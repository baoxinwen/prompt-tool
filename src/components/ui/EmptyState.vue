<script setup lang="ts">
/** 空状态：{{ }} 记号 + 图标 + 标题 + 描述 + 可选动作 */
import type { Component } from 'vue';
import { Sparkles } from 'lucide-vue-next';

withDefaults(
  defineProps<{ icon?: Component; /** 空状态标题；也可用 #title 插槽自定义内容 */ title?: string }>(),
  { icon: () => Sparkles },
);
</script>

<template>
  <div class="empty">
    <div class="empty-mark mono" aria-hidden="true">{{ '{' + '{ … }' + '}' }}</div>
    <div class="empty-icon">
      <component :is="icon" :size="24" :stroke-width="1.8" />
    </div>
    <div class="empty-title"><slot name="title">{{ title }}</slot></div>
    <div class="empty-desc"><slot /></div>
    <div v-if="$slots.action" class="empty-action"><slot name="action" /></div>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 40px 20px;
  text-align: center;
}

/* 丝印等宽记号 */
.empty-mark {
  font-size: 14px;
  color: var(--faint);
  letter-spacing: 0.08em;
  opacity: 0.85;
  user-select: none;
}

/* 图标容器：克制的呼吸（keyframes 复用全局 breathe） */
.empty-icon {
  width: 50px;
  height: 50px;
  border-radius: var(--r-lg);
  display: grid;
  place-items: center;
  background: var(--panel-2);
  border: 1px dashed var(--border-strong);
  color: var(--brand);
  margin-bottom: 2px;
  animation: breathe 2.6s var(--ease) infinite;
}

@media (prefers-reduced-motion: reduce) {
  .empty-icon {
    animation: none;
  }
}

.empty-title {
  font-weight: 600;
  font-size: 14px;
  letter-spacing: 0.01em;
}

.empty-desc {
  color: var(--muted);
  font-size: 13px;
  max-width: 320px;
  line-height: 1.7;
}

.empty-action {
  margin-top: 6px;
}
</style>
