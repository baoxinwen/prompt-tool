/** 时间分组名，顺序即展示顺序（今天→更早） */
export type TimeGroupName = '今天' | '昨天' | '本周' | '更早';
export const TIME_GROUPS: TimeGroupName[] = ['今天', '昨天', '本周', '更早'];

const DAY = 86_400_000;
/** 本地时区的当日零点，避免 UTC 偏移把凌晨归错组 */
const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** 按自然日把时间戳归入 今天/昨天/本周（近 6 天）/更早；非法时间戳一律归更早 */
export function timeGroup(ts: number, now: Date = new Date()): TimeGroupName {
  if (!ts) return '更早';
  const t0 = startOfDay(now);
  if (ts >= t0) return '今天';
  if (ts >= t0 - DAY) return '昨天';
  if (ts >= t0 - 6 * DAY) return '本周';
  return '更早';
}

const WEEK = ['日', '一', '二', '三', '四', '五', '六'];

/** 组内时间标签：今天/昨天只显示时分，本周带星期，更早显示 M/D */
export function groupTimeLabel(ts: number, now: Date = new Date()): string {
  const d = new Date(ts);
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  const g = timeGroup(ts, now);
  if (g === '今天' || g === '昨天') return hm;
  if (g === '本周') return `周${WEEK[d.getDay()]} ${hm}`;
  return `${d.getMonth() + 1}/${d.getDate()}`;
}
