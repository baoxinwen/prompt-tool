import { getCurrentInstance, onBeforeUnmount, ref } from 'vue';

/** 复制对勾自熄时长（铁律 10：2.5s 后自动淡出恢复） */
export const COPY_FEEDBACK_MS = 2500;

/**
 * 复制反馈状态：flash() 点亮对勾，2.5s 后自动熄灭回原态。
 * 重复触发会重置计时（连复制多条纹时对勾不中途熄灭）
 */
export function useCopiedFlash(ms = COPY_FEEDBACK_MS) {
  const copied = ref(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function flash() {
    copied.value = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied.value = false), ms);
  }

  // 允许在组件外（单测）使用：无实例时跳过卸载钩子注册
  if (getCurrentInstance()) onBeforeUnmount(() => clearTimeout(timer));
  return { copied, flash };
}
