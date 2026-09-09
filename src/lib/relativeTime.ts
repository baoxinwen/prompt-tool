const MIN = 60_000;
const HOUR = 3_600_000;

/** 相对时间标签：1 小时内人读相对值（刚刚/N 分钟前/N 小时前），
 *  超过一天相对值失去直觉，落回绝对时间 M/D HH:MM */
export function relativeTime(ts: number, now: Date = new Date()): string {
  const diff = now.getTime() - ts;
  if (diff < MIN) return '刚刚';
  if (diff < HOUR) return `${Math.floor(diff / MIN)} 分钟前`;
  if (diff < 24 * HOUR) return `${Math.floor(diff / HOUR)} 小时前`;
  const d = new Date(ts);
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}
