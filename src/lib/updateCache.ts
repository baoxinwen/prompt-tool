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
import type { UpdateStatus } from '../types';

export const lastAutoUpdate = ref<UpdateStatus | null>(null);
