/** 分类专属色彩：按分类名从纸感墨色板稳定分配（chips/徽章/列表点/详情头带全局一致） */

export interface CategoryColor {
  main: string;
  soft: string;
}

const PALETTE: { main: string; soft: string }[] = [
  { main: '#bd6a58', soft: 'rgba(189, 106, 88, 0.13)' },
  { main: '#6e7f4f', soft: 'rgba(110, 127, 79, 0.13)' },
  { main: '#5e9480', soft: 'rgba(94, 148, 128, 0.13)' },
  { main: '#b08d3f', soft: 'rgba(176, 141, 63, 0.13)' },
  { main: '#8d7966', soft: 'rgba(141, 121, 102, 0.13)' },
  { main: '#4f7d7b', soft: 'rgba(79, 125, 123, 0.13)' },
  { main: '#b06f84', soft: 'rgba(176, 111, 132, 0.13)' },
  { main: '#5c6e52', soft: 'rgba(92, 110, 82, 0.13)' },
  { main: '#a07e46', soft: 'rgba(160, 126, 70, 0.13)' },
  { main: '#667a8c', soft: 'rgba(102, 122, 140, 0.13)' },
];

const NEUTRAL: CategoryColor = { main: '#918d80', soft: 'rgba(145, 141, 128, 0.13)' };

const cache = new Map<string, CategoryColor>();

export function categoryColor(name: string): CategoryColor {
  if (!name) return NEUTRAL;
  const hit = cache.get(name);
  if (hit) return hit;
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  const c = PALETTE[h % PALETTE.length];
  cache.set(name, c);
  return c;
}
