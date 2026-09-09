import { describe, expect, it } from 'vitest';
import { TIME_GROUPS, groupTimeLabel, timeGroup } from './timeGroup';

// 固定「现在」，避免测试依赖真实时钟：2026-09-08 12:00 周二
const now = new Date(2026, 8, 8, 12, 0);

describe('timeGroup', () => {
  it('当天→今天', () => expect(timeGroup(new Date(2026, 8, 8, 9, 0).getTime(), now)).toBe('今天'));
  it('前一天→昨天', () => expect(timeGroup(new Date(2026, 8, 7, 23, 59).getTime(), now)).toBe('昨天'));
  it('6 天前→本周', () => expect(timeGroup(new Date(2026, 8, 2, 12, 0).getTime(), now)).toBe('本周'));
  it('7 天前→更早', () => expect(timeGroup(new Date(2026, 8, 1, 12, 0).getTime(), now)).toBe('更早'));
  it('0/负时间戳→更早', () => expect(timeGroup(0, now)).toBe('更早'));

  it('TIME_GROUPS 顺序固定，供分组渲染使用', () => {
    expect(TIME_GROUPS).toEqual(['今天', '昨天', '本周', '更早']);
  });
});

describe('groupTimeLabel', () => {
  it('组内标签：今天只有时分', () => expect(groupTimeLabel(new Date(2026, 8, 8, 9, 5).getTime(), now)).toBe('09:05'));
  it('组内标签：本周带星期', () => expect(groupTimeLabel(new Date(2026, 8, 2, 21, 5).getTime(), now)).toBe('周三 21:05'));
  it('组内标签：更早为 M/D', () => expect(groupTimeLabel(new Date(2026, 7, 29, 9, 0).getTime(), now)).toBe('8/29'));
});
