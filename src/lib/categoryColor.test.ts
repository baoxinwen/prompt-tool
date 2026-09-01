import { describe, it, expect } from 'vitest';
import { categoryColor } from './categoryColor';

const PALETTE_MAINS = [
  '#a85b4b',
  '#6e7f4f',
  '#4e7a6a',
  '#b08d3f',
  '#7a6656',
  '#4f7d7b',
  '#9c5f72',
  '#5c6e52',
  '#8a6d3b',
  '#667a8c',
];

describe('categoryColor', () => {
  it('空分类名返回中性色', () => {
    expect(categoryColor('')).toEqual({ main: '#918d80', soft: 'rgba(145, 141, 128, 0.13)' });
  });

  it('同名分类颜色稳定（缓存一致性）', () => {
    const first = categoryColor('开发');
    const second = categoryColor('开发');
    expect(second).toEqual(first);
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
