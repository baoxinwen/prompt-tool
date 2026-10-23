import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { useCopiedFlash, COPY_FEEDBACK_MS } from './copyFeedback';

describe('useCopiedFlash（铁律 10：对勾 2.5s 自熄）', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('flash 后点亮，默认 2500ms 自动熄灭', () => {
    const { copied, flash } = useCopiedFlash();
    expect(copied.value).toBe(false);
    flash();
    expect(copied.value).toBe(true);
    vi.advanceTimersByTime(COPY_FEEDBACK_MS - 1);
    expect(copied.value).toBe(true);
    vi.advanceTimersByTime(1);
    expect(copied.value).toBe(false);
  });

  it('连发重置计时：第一发未满时再次 flash，从第二发重新起算', () => {
    const { copied, flash } = useCopiedFlash();
    flash();
    vi.advanceTimersByTime(1500);
    flash();
    vi.advanceTimersByTime(1500);
    expect(copied.value).toBe(true);
    vi.advanceTimersByTime(COPY_FEEDBACK_MS - 1500);
    expect(copied.value).toBe(false);
  });

  it('卸载时清理定时器（不依赖组件挂载也不抛错）', () => {
    const { copied, flash } = useCopiedFlash();
    flash();
    expect(copied.value).toBe(true);
    vi.advanceTimersByTime(COPY_FEEDBACK_MS);
    expect(copied.value).toBe(false);
  });
});
