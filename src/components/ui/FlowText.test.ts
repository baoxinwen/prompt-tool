import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import FlowText from './FlowText.vue';

describe('FlowText（铁律 4：流式打字）', () => {
  it('按字拆分渲染并保留整句 aria-label', () => {
    const w = mount(FlowText, { props: { text: '提示词' } });
    expect(w.findAll('.ft-ch')).toHaveLength(3);
    expect(w.find('.flow-text').attributes('aria-label')).toBe('提示词');
    expect(w.text()).toContain('提');
  });

  it('错开延迟按 30ms 步进且不超过 maxSteps 封顶', () => {
    const w = mount(FlowText, { props: { text: 'abcdefghij', maxSteps: 3 } });
    const delays = w.findAll('.ft-ch').map((c) => c.attributes('style') ?? '');
    expect(delays[0]).toContain('0ms');
    expect(delays[2]).toContain('60ms'); // 未到封顶：2 步 × 30ms
    expect(delays[3]).toContain('90ms'); // 触及第 3 步封顶
    expect(delays[9]).toContain('90ms'); // 之后不再增长
  });
});
