import { describe, expect, it } from 'vitest';
import { previewSegments } from './preview';

describe('previewSegments', () => {
  it('变量为原子 chip 片段', () => {
    expect(previewSegments('目标用户：{{目标用户|一段话说明}}。', 44))
      .toEqual([{ t: '目标用户：' }, { t: '目标用户', chip: true }, { t: '。' }]);
  });

  it('截断落在变量前：整 chip 丢弃', () => {
    const segs = previewSegments('a'.repeat(30) + '{{目标用户|提示}}' + 'b'.repeat(30), 34);
    expect(segs[segs.length - 1]).toEqual({ t: '…', cut: true });
    expect(segs.some((s) => s.chip)).toBe(false);
  });

  it('纯文本截断', () => expect(previewSegments('x'.repeat(50), 20)).toEqual([{ t: 'x'.repeat(20) }, { t: '…', cut: true }]));
  it('恰好等于预算不截断', () => expect(previewSegments('y'.repeat(20), 20)).toEqual([{ t: 'y'.repeat(20) }]));
  it('连续空白折叠', () => expect(previewSegments('  a   b\n\tc  ', 44)).toEqual([{ t: 'a b c' }]));
});
