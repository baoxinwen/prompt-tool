/**
 * 启动自动检查的更新结果缓存：Manager 的 autoCheckUpdate 发现新版后写入，
 * 设置页 UpdateSection 挂载时消费（直接进入 available 态），用户点通知条
 * 「去更新」后无需再手动检查一次。
 *
 * 跳过版本（F8）的过滤在 Manager 写入前完成，这里不做 skip 判断——
 * 设置页常驻展示不受 skip 影响是既有语义。消费为一次性：读取后即清空，
 * 避免之后重新挂载时复活已被手动检查推翻的旧结果。
 */
import { ref } from 'vue';
import type { UpdateProgress, UpdateStatus } from '../types';

export const lastAutoUpdate = ref<UpdateStatus | null>(null);

export type UpdatePhase =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'uptodate'
  | 'error';

/** 更新区状态机（评审 2026-09-10 I7）：放在模块级而非组件内——下载中切换标签
 *  会卸载 UpdateSection，组件态随之销毁，重挂载后误以为空闲再次触发
 *  download_and_install，与后端仍在进行的下载产生并发竞态。模块级 ref
 *  与窗口同生命周期，跨卸载保持 phase/进度，天然防重入 */
export const updatePhase = ref<UpdatePhase>('idle');
export const updateInfo = ref<Extract<UpdateStatus, { kind: 'available' }> | null>(null);
export const updateErrMsg = ref('');
export const updateErrorKind = ref('');
export const updateProgress = ref<UpdateProgress | null>(null);

/** 仅供测试复位模块态 */
export function resetUpdateState() {
  updatePhase.value = 'idle';
  updateInfo.value = null;
  updateErrMsg.value = '';
  updateErrorKind.value = '';
  updateProgress.value = null;
}
