import { describe, it, expect } from 'vitest';
import { categoryColor } from './categoryColor';

const PALETTE_MAINS = [
  '#bd6a58',
  '#6e7f4f',
  '#5e9480',
  '#b08d3f',
  '#8d7966',
  '#4f7d7b',
  '#b06f84',
  '#5c6e52',
  '#a07e46',
  '#667a8c',
];

/** 奇数位（1/3/5/7/9，即索引 0/2/4/6/8）提亮后的色板 A 新值，哈希取色位置不变 */
const ODD_POSITION_NEWS = ['#bd6a58', '#5e9480', '#8d7966', '#b06f84', '#a07e46'];

describe('categoryColor', () => {
  it('空分类名返回中性色', () => {
    expect(categoryColor('')).toEqual({ main: '#918d80', soft: 'rgba(145, 141, 128, 0.13)' });
  });

  it('同名分类颜色稳定（缓存一致性）', () => {
    const first = categoryColor('开发');
    const second = categoryColor('开发');
    expect(second).toEqual(first);
  });

  it('奇数位 5 色提亮为色板 A 新值（soft 同步 13%）', () => {
    for (const main of ODD_POSITION_NEWS) {
      expect(PALETTE_MAINS.filter((m) => m === main), `${main} 应在色板中出现一次`).toHaveLength(1);
    }
  });

  it('哈希取色位置与现版一致：奇数位新值、偶数位保持原值', () => {
    // 复刻取位规则：单字符哈希即 charCode，h % 10 定位（'d'=100→0，'f'=102→2 …）
    expect(categoryColor('d')).toEqual({ main: '#bd6a58', soft: 'rgba(189, 106, 88, 0.13)' });
    expect(categoryColor('f')).toEqual({ main: '#5e9480', soft: 'rgba(94, 148, 128, 0.13)' });
    expect(categoryColor('h')).toEqual({ main: '#8d7966', soft: 'rgba(141, 121, 102, 0.13)' });
    expect(categoryColor('j')).toEqual({ main: '#b06f84', soft: 'rgba(176, 111, 132, 0.13)' });
    expect(categoryColor('b')).toEqual({ main: '#a07e46', soft: 'rgba(160, 126, 70, 0.13)' });
    // 偶数位（2/4/6/8/10，即索引 1/3/5/7/9）不变
    expect(categoryColor('e')).toEqual({ main: '#6e7f4f', soft: 'rgba(110, 127, 79, 0.13)' });
    expect(categoryColor('g')).toEqual({ main: '#b08d3f', soft: 'rgba(176, 141, 63, 0.13)' });
    expect(categoryColor('i')).toEqual({ main: '#4f7d7b', soft: 'rgba(79, 125, 123, 0.13)' });
    expect(categoryColor('a')).toEqual({ main: '#5c6e52', soft: 'rgba(92, 110, 82, 0.13)' });
    expect(categoryColor('c')).toEqual({ main: '#667a8c', soft: 'rgba(102, 122, 140, 0.13)' });
  });

  it('返回值必须属于 10 色板之一', () => {
    for (const name of ['开发', '写作', '设计', 'project-a', '分类九']) {
      const { main } = categoryColor(name);
      expect(PALETTE_MAINS, `「${name}」的主色 ${main} 应在色板内`).toContain(main);
    }
  });

  it('不同分类名映射到不同颜色（哈希分散性抽查）', () => {
    expect(categoryColor('a').main).not.toBe(categoryColor('b').main);
    expect(categoryColor('开发').main).not.toBe(categoryColor('写作').main);
  });

  it('中英文与特殊字符分类名均可用', () => {
    for (const name of ['中文分类', 'english', 'with space', '🎉emoji', 'a'.repeat(100)]) {
      const c = categoryColor(name);
      expect(c.main).toMatch(/^#[0-9a-f]{6}$/);
      expect(c.soft).toContain('0.13');
    }
  });
});
