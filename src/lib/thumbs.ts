import { ref } from 'vue';
import { api } from './api';
import type { ClipboardItem } from '../types';

/**
 * 剪贴板图片缩略图惰性加载，QuickPanel 与 ClipboardPane 共用（评审 M9：
 * 此前两处各维护一份逐行相同的实现，且失败后每次渲染都会重发注定失败
 * 的 IPC）。语义：
 * - 成功后缓存，重渲染不再发起请求；
 * - 失败进入 failed 集合不再自动重试——缩略图读的是本地已落盘文件，
 *   失败通常意味着文件丢失，按渲染重试只会形成无退避的失败风暴。
 */
export function useImageThumbs() {
  const thumbs = ref<Record<string, string>>({});
  const pending = new Set<string>();
  const failed = new Set<string>();

  function thumbFor(item: ClipboardItem): string {
    if (item.kind !== 'image' || !item.image) return '';
    const cached = thumbs.value[item.id];
    if (cached) return cached;
    if (pending.has(item.id) || failed.has(item.id)) return '';
    pending.add(item.id);
    api
      .getImageThumb(item.id)
      .then((url) => (thumbs.value[item.id] = url))
      .catch((e) => {
        failed.add(item.id);
        console.error(`[prompt-tool] 缩略图加载失败（${item.id}）:`, e);
      })
      .finally(() => pending.delete(item.id));
    return '';
  }

  return { thumbs, thumbFor };
}
