import { describe, it, expect } from 'vitest';
import { categoryColor } from './categoryColor';

const PALETTE_MAINS = [
  '#4a4a4a',
  '#6f6f6f',
  '#525252',
  '#777777',
  '#5a5a5a',
  '#7f7f7f',
  '#626262',
  '#878787',
  '#6a6a6a',
  '#8f8f8f',
];

/** 奇数位（1/3/5/7/9，即索引 0/2/4/6/8）提亮后的色板 A 新值，哈希取色位置不变 */
const ODD_POSITION_NEWS = ['#4a4a4a', '#525252', '#5a5a5a', '#626262', '#6a6a6a'];

describe('categoryColor', () => {
  it('空分类名返回中性色', () => {
    expect(categoryColor('')).toEqual({ main: '#9a9a9a', soft: 'rgba(17, 17, 17, 0.08)' });
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
    expect(categoryColor('d')).toEqual({ main: '#4a4a4a', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('f')).toEqual({ main: '#525252', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('h')).toEqual({ main: '#5a5a5a', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('j')).toEqual({ main: '#626262', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('b')).toEqual({ main: '#6a6a6a', soft: 'rgba(17, 17, 17, 0.08)' });
    // 偶数位（2/4/6/8/10，即索引 1/3/5/7/9）不变
    expect(categoryColor('e')).toEqual({ main: '#6f6f6f', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('g')).toEqual({ main: '#777777', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('i')).toEqual({ main: '#7f7f7f', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('a')).toEqual({ main: '#878787', soft: 'rgba(17, 17, 17, 0.08)' });
    expect(categoryColor('c')).toEqual({ main: '#8f8f8f', soft: 'rgba(17, 17, 17, 0.08)' });
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
      expect(c.soft).toContain('0.08');
    }
  });
});
