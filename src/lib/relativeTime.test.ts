import { describe, expect, it } from 'vitest';
import { relativeTime } from './relativeTime';

// 固定「现在」，避免测试依赖真实时钟：2026-09-08 12:00 周二
const now = new Date(2026, 8, 8, 12, 0);

describe('relativeTime', () => {
  it('<60s→刚刚', () => expect(relativeTime(now.getTime() - 30_000, now)).toBe('刚刚'));
  it('59s 仍为刚刚', () => expect(relativeTime(now.getTime() - 59_000, now)).toBe('刚刚'));

  it('60s→1 分钟前', () => expect(relativeTime(now.getTime() - 60_000, now)).toBe('1 分钟前'));
  it('5min→5 分钟前', () => expect(relativeTime(now.getTime() - 5 * 60_000, now)).toBe('5 分钟前'));
  it('59min→59 分钟前', () => expect(relativeTime(now.getTime() - 59 * 60_000, now)).toBe('59 分钟前'));

  it('60min→1 小时前', () => expect(relativeTime(now.getTime() - 60 * 60_000, now)).toBe('1 小时前'));
  it('3h→3 小时前', () => expect(relativeTime(now.getTime() - 3 * 3_600_000, now)).toBe('3 小时前'));
  it('23h59m→23 小时前', () =>
    expect(relativeTime(now.getTime() - 23 * 3_600_000 - 59 * 60_000, now)).toBe('23 小时前'));

  it('≥24h→M/D HH:MM（不足两位补零）', () =>
    expect(relativeTime(new Date(2026, 8, 7, 9, 5).getTime(), now)).toBe('9/7 09:05'));
  it('整 24h→前一天的 M/D HH:MM', () =>
    expect(relativeTime(now.getTime() - 24 * 3_600_000, now)).toBe('9/7 12:00'));
});
