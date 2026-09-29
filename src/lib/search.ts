import { pinyin } from 'pinyin-pro';
import type { ClipboardItem, Prompt } from '../types';

/** 拼音索引缓存：原文 → { 全拼串, 首字母串 }（均小写、仅字母数字） */
const pyCache = new Map<string, { full: string; first: string }>();

function pinyinIndex(text: string): { full: string; first: string } {
  const cached = pyCache.get(text);
  if (cached) return cached;
  let entry = { full: '', first: '' };
  try {
    entry = {
      full: pinyin(text, { toneType: 'none', type: 'array' })
        .join('')
        .toLowerCase()
        .replace(/[^a-z0-9]/g, ''),
      first: pinyin(text, { pattern: 'first', toneType: 'none', type: 'array' })
        .join('')
        .toLowerCase()
        .replace(/[^a-z0-9]/g, ''),
    };
  } catch {
    /* 非中文字符串直接走空索引 */
  }
  pyCache.set(text, entry);
  return entry;
}

function isAsciiQuery(q: string): boolean {
  return /^[a-zA-Z0-9]+$/.test(q);
}

/** 子串或拼音（首字母/全拼）匹配。q 需已 toLowerCase */
export function matchText(text: string, q: string): boolean {
  if (text.toLowerCase().includes(q)) return true;
  if (isAsciiQuery(q)) {
    const idx = pinyinIndex(text);
    return idx.first.includes(q) || idx.full.includes(q);
  }
  return false;
}

function hitScore(q: string, p: Prompt): number {
  let score = 0;
  const title = p.title.toLowerCase();
  if (title.includes(q)) score += 3;
  else if (isAsciiQuery(q)) {
    const idx = pinyinIndex(p.title);
    if (idx.first.includes(q)) score += 2;
    else if (idx.full.includes(q)) score += 1;
  }
  if (p.tags.some((t) => matchText(t, q))) score += 2;
  if (p.category.toLowerCase().includes(q)) score += 1;
  if (p.content.toLowerCase().includes(q)) score += 1;
  return score;
}

export function filterPrompts(prompts: Prompt[], query: string, category: string): Prompt[] {
  let list = prompts;
  if (category) list = list.filter((p) => p.category === category);
  if (query.trim()) {
    const q = query.trim().toLowerCase();
    list = list
      .map((p) => ({ p, s: hitScore(q, p) }))
      .filter(({ s }) => s > 0)
      .sort((a, b) => {
        if (a.p.pinned !== b.p.pinned) return a.p.pinned ? -1 : 1;
        if (b.s !== a.s) return b.s - a.s;
        if (b.p.useCount !== a.p.useCount) return b.p.useCount - a.p.useCount;
        return (b.p.lastUsedAt || 0) - (a.p.lastUsedAt || 0);
      })
      .map(({ p }) => p);
  } else {
    list = [...list].sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      if ((b.lastUsedAt || 0) !== (a.lastUsedAt || 0)) return (b.lastUsedAt || 0) - (a.lastUsedAt || 0);
      if (b.useCount !== a.useCount) return b.useCount - a.useCount;
      return b.updatedAt - a.updatedAt;
    });
  }
  return list;
}

export function filterClipboard(items: ClipboardItem[], query: string): ClipboardItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((i) => i.content.toLowerCase().includes(q));
}

export function formatTime(ts: number): string {
  if (!ts) return '';
  const d = new Date(ts);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  if (sameDay) return hm;
  return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}

/** 按 UTF-16 码元截断但不切碎代理对（评审 M21）：切点落在高位代理上时
 *  回退一位，避免行尾渲染出 U+FFFD 乱码（全 emoji 内容可触发）。
 *  end 语义与 slice(0, end) 一致（码元数，非字符数） */
export function sliceUtf16Safe(s: string, end: number): string {
  if (end >= s.length) return s;
  if (end <= 0) return '';
  const prev = s.charCodeAt(end - 1);
  // 前一码元是高位代理且切点处恰是配对的低位代理：整对不可分，回退一位
  if (prev >= 0xd800 && prev <= 0xdbff) {
    const next = s.charCodeAt(end);
    if (next >= 0xdc00 && next <= 0xdfff) return s.slice(0, end - 1);
  }
  return s.slice(0, end);
}

export function preview(text: string, max = 120): string {
  const one = text.replace(/\s+/g, ' ').trim();
  return one.length > max ? sliceUtf16Safe(one, max) + '…' : one;
}

/** 关键词命中分段（用于标题高亮）：拼音命中的部分无法定位，不做高亮 */
export function highlightSegs(text: string, q: string): { t: string; hit: boolean }[] {
  const needle = q.trim().toLowerCase();
  if (!needle) return [{ t: text, hit: false }];
  const lower = text.toLowerCase();
  const segs: { t: string; hit: boolean }[] = [];
  let i = 0;
  // 最多分 32 段，防病态输入
  for (let guard = 0; guard < 32; guard++) {
    const idx = lower.indexOf(needle, i);
    if (idx === -1) {
      segs.push({ t: text.slice(i), hit: false });
      return segs;
    }
    if (idx > i) segs.push({ t: text.slice(i, idx), hit: false });
    segs.push({ t: text.slice(idx, idx + needle.length), hit: true });
    i = idx + needle.length;
    if (i >= text.length) return segs;
  }
  segs.push({ t: text.slice(i), hit: false });
  return segs;
}
