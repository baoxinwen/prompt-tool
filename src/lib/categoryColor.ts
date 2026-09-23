/** 分类专属色彩：黑白仪器——按分类名从纯中性灰阶稳定分配（chips/徽章/列表点/详情头带全局一致） */

export interface CategoryColor {
  main: string;
  soft: string;
}

/** 黑白仪器：灰阶分配
 *  10 档 main 全为纯中性灰（R=G=B），从 #4a4a4a 到 #8f8f8f 拉开明度差；
 *  按「深浅交替」排列，相邻档明度差 ≥ 8（实际 29+），保证哈希相邻取色仍可分辨；
 *  soft 统一为墨色低透明度底染 */
const PALETTE: { main: string; soft: string }[] = [
  { main: '#4a4a4a', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#6f6f6f', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#525252', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#777777', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#5a5a5a', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#7f7f7f', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#626262', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#878787', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#6a6a6a', soft: 'rgba(17, 17, 17, 0.08)' },
  { main: '#8f8f8f', soft: 'rgba(17, 17, 17, 0.08)' },
];

const NEUTRAL: CategoryColor = { main: '#9a9a9a', soft: 'rgba(17, 17, 17, 0.08)' };

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
