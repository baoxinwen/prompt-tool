import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** 评审 I8 守卫：语义中性色（--faint/--muted/--text-2）在两套主题下
 *  对 --bg 与 --panel（页面底与卡片底，评审点 SyncPane .note 与 .field 标签
 *  的实际底色）都必须达到 WCAG AA 正文对比度 4.5:1。改色值前先看这里。 */

const css = readFileSync(join(__dirname, '../styles/tokens.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');

function collectProps(blockRe: RegExp): Map<string, string> {
  const map = new Map<string, string>();
  for (const m of css.matchAll(blockRe)) {
    for (const p of m[1].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
      map.set(p[1], p[2].trim());
    }
  }
  return map;
}

/** :root 基础（暗色）与 :root[data-theme='light'] 覆盖块 */
const base = collectProps(/:root\s*\{([^}]*)\}/g);
const lightOverlay = collectProps(/:root\[data-theme='light'\]\s*\{([^}]*)\}/g);

function resolve(props: Map<string, string>, name: string, seen = new Set<string>()): string {
  let v = props.get(name);
  expect(v, `令牌 ${name} 未定义`).toBeDefined();
  while (v!.startsWith('var(')) {
    const inner = v!.slice(4, -1).split(',')[0].trim();
    expect(seen.has(inner), `令牌 ${name} 存在循环引用: ${inner}`).toBe(false);
    seen.add(inner);
    v = props.get(inner);
    expect(v, `令牌 ${name} 的引用 ${inner} 未定义`).toBeDefined();
  }
  return v!;
}

function lum(hex: string): number {
  const c = hex.replace('#', '');
  const [r, g, b] = [0, 2, 4]
    .map((i) => parseInt(c.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(fg: string, bg: string): number {
  const [l1, l2] = [lum(fg), lum(bg)].sort((a, b) => b - a);
  return (l1 + 0.05) / (l2 + 0.05);
}

describe.each([
  ['暗色', base],
  ['亮色', new Map([...base, ...lightOverlay])],
])('tokens.css 中性色对比度（评审 I8）— %s', (_name, props) => {
  const resolveIn = (n: string) => resolve(props, n);

  it.each(['--bg', '--panel'])('faint/muted/text-2 对 %s 达到 AA 4.5:1', (surface) => {
    const bg = resolveIn(surface);
    for (const token of ['--faint', '--muted', '--text-2']) {
      const ratio = contrast(resolveIn(token), bg);
      expect(ratio, `${token}(${resolveIn(token)}) 对 ${surface}(${bg}) = ${ratio.toFixed(2)}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('视觉层级单调：text-2 比 muted 清晰，muted 比 faint 清晰（对比 panel）', () => {
    const panel = resolveIn('--panel');
    const t2 = contrast(resolveIn('--text-2'), panel);
    const m = contrast(resolveIn('--muted'), panel);
    const f = contrast(resolveIn('--faint'), panel);
    expect(t2).toBeGreaterThan(m);
    expect(m).toBeGreaterThan(f);
  });
});
