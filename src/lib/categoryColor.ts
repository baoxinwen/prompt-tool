/** 分类专属色彩：按分类名从纸感墨色板稳定分配（chips/徽章/列表点/详情头带全局一致） */

export interface CategoryColor {
  main: string;
  soft: string;
}

const PALETTE: { main: string; soft: string }[] = [
  { main: '#a85b4b', soft: 'rgba(168, 91, 75, 0.13)' },
  { main: '#6e7f4f', soft: 'rgba(110, 127, 79, 0.13)' },
  { main: '#4e7a6a', soft: 'rgba(78, 122, 106, 0.13)' },
  { main: '#b08d3f', soft: 'rgba(176, 141, 63, 0.13)' },
  { main: '#7a6656', soft: 'rgba(122, 102, 86, 0.13)' },
  { main: '#4f7d7b', soft: 'rgba(79, 125, 123, 0.13)' },
  { main: '#9c5f72', soft: 'rgba(156, 95, 114, 0.13)' },
  { main: '#5c6e52', soft: 'rgba(92, 110, 82, 0.13)' },
  { main: '#8a6d3b', soft: 'rgba(138, 109, 59, 0.13)' },
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
