import type { InjectionKey, Ref } from 'vue';
import type { AppData } from '../types';

export interface ToastAction {
  label: string;
  handler: () => void | Promise<void>;
}

export interface ManagerCtx {
  data: Ref<AppData | null>;
  refresh: () => Promise<void>;
  /** 轻提示；action 可选（如「撤销」） */
  toast: (msg: string, kind?: 'ok' | 'err', action?: ToastAction) => void;
  /** 应用内确认框（替换原生 window.confirm），resolve true=确认 */
  confirm: (options: {
    title: string;
    message?: string;
    confirmText?: string;
    danger?: boolean;
  }) => Promise<boolean>;
  /**
   * 注册/注销「离开提示词页守卫」：守卫返回 false 表示当前有未保存状态，
   * Manager 切换标签前会先弹确认框。PromptsPane 与 SyncPane 都会注册
   * （挂载时注册、卸载时注销），守卫随 pane 卸载自然失效（评审 M3#8）
   */
  setLeaveGuard: (guard: (() => boolean) | null) => void;
}

export const managerKey: InjectionKey<ManagerCtx> = Symbol('manager');
