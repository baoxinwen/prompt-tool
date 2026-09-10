import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import QuickPanel from './QuickPanel.vue';
import VarDialog from '../components/VarDialog.vue';
import { api } from '../lib/api';
import type { AppData, Prompt } from '../types';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock('../lib/api', () => ({
  api: {
    getData: vi.fn(),
    invokePaste: vi.fn(async () => undefined),
    copyText: vi.fn(async () => undefined),
    recordUse: vi.fn(async () => undefined),
    hideQuick: vi.fn(async () => undefined),
    getClipboardText: vi.fn(async () => ''),
    setPanelHeight: vi.fn(async () => undefined),
    openManager: vi.fn(async () => undefined),
    getImageThumb: vi.fn(async () => ''),
    pasteImage: vi.fn(async () => undefined),
    copyImage: vi.fn(async () => undefined),
    getVarMemory: vi.fn(async () => ({})),
    saveVarMemory: vi.fn(async () => undefined),
  },
}));

const mockedApi = vi.mocked(api, true);

function makePrompt(overrides: Partial<Prompt> & { id: string; title: string }): Prompt {
  return {
    content: `内容-${overrides.title}`,
    category: '开发',
    tags: [],
    pinned: false,
    hotkey: '',
    useCount: 0,
    lastUsedAt: 0,
    createdAt: 1_000,
    updatedAt: 1_000,
    ...overrides,
  };
}

const fixturePrompts: Prompt[] = [
  makePrompt({ id: 'p1', title: '代码审查', content: '请审查这段代码' }),
  makePrompt({ id: 'p2', title: '周报生成', content: '本周完成：{{本周工作|工作内容}}' }),
  makePrompt({ id: 'p3', title: '翻译助手', category: '写作' }),
];

const fixture: AppData = {
  version: 1,
  seeded: true,
  settings: {} as AppData['settings'],
  categories: ['开发', '写作'],
  prompts: fixturePrompts,
  clipboard: [
    { id: 'c1', content: '复制过的文本', copiedAt: 1_790_000_000_000, kind: 'text' },
  ],
  tombstones: [],
};

async function mountPanel(data: AppData = fixture) {
  mockedApi.getData.mockResolvedValue(data);
  const wrapper = mount(QuickPanel, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

beforeAll(() => {
  if (!('ResizeObserver' in globalThis)) {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
  }
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('QuickPanel：快捷面板', () => {
  it('加载后渲染提示词列表，首项默认选中', async () => {
    const wrapper = await mountPanel();
    expect(mockedApi.getData).toHaveBeenCalledTimes(1);
    const items = wrapper.findAll('.qp-list .item');
    expect(items.length).toBe(3);
    expect(items[0].classes()).toContain('active');
    expect(items[0].text()).toContain('代码审查');
    wrapper.unmount();
  });

  it('输入关键词联动过滤（含拼音首字母命中）', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('input.qp-search').setValue('dm');
    await flushPromises();
    let titles = wrapper.findAll('.qp-list .item .item-title').map((t) => t.text());
    expect(titles.some((t) => t.includes('代码审查'))).toBe(true);

    await wrapper.find('input.qp-search').setValue('翻译');
    await flushPromises();
    titles = wrapper.findAll('.qp-list .item .item-title').map((t) => t.text());
    expect(titles.length).toBe(1);
    expect(titles[0]).toContain('翻译助手');
    wrapper.unmount();
  });

  it('无匹配时显示空状态', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('input.qp-search').setValue('完全无关的查询词');
    await flushPromises();
    expect(wrapper.text()).toContain('没有匹配的提示词');
    wrapper.unmount();
  });

  it('分类 chip 过滤列表', async () => {
    const wrapper = await mountPanel();
    const chip = wrapper.findAll('.qp-chips .chip').find((c) => c.text().includes('写作'))!;
    await chip.trigger('click');
    await flushPromises();
    const titles = wrapper.findAll('.qp-list .item .item-title').map((t) => t.text());
    expect(titles.length).toBe(1);
    expect(titles[0]).toContain('翻译助手');
    wrapper.unmount();
  });

  it('方向键循环移动选中项', async () => {
    const wrapper = await mountPanel();
    const root = wrapper.find('.qp');
    await root.trigger('keydown', { key: 'ArrowDown' });
    expect(wrapper.findAll('.qp-list .item')[1].classes()).toContain('active');
    await root.trigger('keydown', { key: 'ArrowUp', ...{} });
    expect(wrapper.findAll('.qp-list .item')[0].classes()).toContain('active');
    // 从第 0 项上移应环绕到最后一项
    await root.trigger('keydown', { key: 'ArrowUp' });
    const items = wrapper.findAll('.qp-list .item');
    expect(items[items.length - 1].classes()).toContain('active');
    wrapper.unmount();
  });

  it('Enter 粘贴无变量提示词：invokePaste 收到原文与 id', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.invokePaste).toHaveBeenCalledWith('请审查这段代码', 'p1');
    wrapper.unmount();
  });

  it('Enter 粘贴带提示的 {{clipboard|提示}}：占位符被剪贴板内容整体替换', async () => {
    mockedApi.getClipboardText.mockResolvedValue('剪贴内容');
    const data: AppData = {
      ...fixture,
      prompts: [
        makePrompt({ id: 'p9', title: '带提示自动变量', content: '参考：{{clipboard|将用剪贴板内容替换}}' }),
      ],
    };
    const wrapper = await mountPanel(data);
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.invokePaste).toHaveBeenCalledWith('参考：剪贴内容', 'p9');
    wrapper.unmount();
  });

  it('Enter 在有变量提示词上弹出变量表单，填写确认后粘贴替换结果', async () => {
    const wrapper = await mountPanel();
    // 下移一次选中 p2（带 {{本周工作}} 变量）
    await wrapper.find('.qp').trigger('keydown', { key: 'ArrowDown' });
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();

    const dialog = wrapper.findComponent(VarDialog);
    expect(dialog.exists()).toBe(true);
    expect(mockedApi.invokePaste).not.toHaveBeenCalled();

    await dialog.findAll('textarea')[0].setValue('写完了三个模块');
    await dialog.find('.vd-foot button').trigger('click');
    await flushPromises();

    expect(mockedApi.invokePaste).toHaveBeenCalledWith('本周完成：写完了三个模块', 'p2');
    expect(wrapper.findComponent(VarDialog).exists()).toBe(false);
    // 确认路径同样恢复焦点（回归：卸载后焦点落 body，键盘导航全失效）
    expect(document.activeElement).toBe(wrapper.find('input.qp-search').element);
    wrapper.unmount();
  });

  it('Shift+Enter 复制：recordUse 后延时隐藏面板', async () => {
    vi.useFakeTimers();
    const wrapper = await mountPanel();
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter', shiftKey: true });
    await flushPromises();

    expect(mockedApi.copyText).toHaveBeenCalledWith('请审查这段代码');
    expect(mockedApi.recordUse).toHaveBeenCalledWith('p1');
    expect(mockedApi.hideQuick).not.toHaveBeenCalled();
    vi.advanceTimersByTime(400);
    expect(mockedApi.hideQuick).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('剪贴板图片项：Enter 粘贴、Shift+Enter 复制而非静默无操作（M12）', async () => {
    const data: AppData = {
      ...fixture,
      clipboard: [
        {
          id: 'img1',
          content: '',
          copiedAt: 1_790_000_000_000,
          kind: 'image',
          image: { file: 'a.png', width: 10, height: 10 },
        },
      ],
    };
    const wrapper = await mountPanel(data);
    await wrapper.find('.qp').trigger('keydown', { key: 'Tab' }); // 切到剪贴板模式
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter', shiftKey: true });
    await flushPromises();
    expect(mockedApi.copyImage).toHaveBeenCalledWith('img1');
    expect(mockedApi.pasteImage).not.toHaveBeenCalled();

    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.pasteImage).toHaveBeenCalledWith('img1');
    wrapper.unmount();
  });

  it('{{clipboard}} 读取失败与为空分别提示，不混为一谈', async () => {
    const data: AppData = {
      ...fixture,
      prompts: [makePrompt({ id: 'pc', title: '自动变量', content: 'x{{clipboard}}' })],
    };
    // 读取失败：必须提示「读取失败」而非误导性的「剪贴板为空」
    mockedApi.getClipboardText.mockRejectedValue('IPC 断开');
    const wrapper = await mountPanel(data);
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.invokePaste).toHaveBeenCalledWith('x', 'pc');
    expect(wrapper.find('.toast').text()).toContain('读取剪贴板失败');
    wrapper.unmount();

    // 确实为空：提示「剪贴板为空」
    mockedApi.getClipboardText.mockResolvedValue('');
    const w2 = await mountPanel(data);
    await w2.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(w2.find('.toast').text()).toContain('剪贴板为空');
    w2.unmount();
  });

  it('Esc 隐藏面板', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('.qp').trigger('keydown', { key: 'Escape' });
    await flushPromises();
    expect(mockedApi.hideQuick).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('输入法组合态的 Enter/Esc 不触发面板快捷键（评审 C1）', async () => {
    const wrapper = await mountPanel();
    // 组合态 Enter：拼音上屏候选词，不得误判为「粘贴」
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter', isComposing: true });
    await flushPromises();
    expect(mockedApi.invokePaste).not.toHaveBeenCalled();
    expect(mockedApi.copyText).not.toHaveBeenCalled();

    // 组合态 Esc：取消候选词，不得隐藏面板
    await wrapper.find('.qp').trigger('keydown', { key: 'Escape', isComposing: true });
    await flushPromises();
    expect(mockedApi.hideQuick).not.toHaveBeenCalled();

    // 非组合态行为不受影响（对照）
    await wrapper.find('.qp').trigger('keydown', { key: 'Escape' });
    await flushPromises();
    expect(mockedApi.hideQuick).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('{{clipboard}} 剪贴板含 $&/$$/派生替换序列时按字面填充（评审 C2）', async () => {
    mockedApi.getClipboardText.mockResolvedValue("awk '{print $$}'");
    const data: AppData = {
      ...fixture,
      prompts: [makePrompt({ id: 'pc2', title: 'sed 片段', content: 'run {{clipboard}}' })],
    };
    const wrapper = await mountPanel(data);
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.invokePaste).toHaveBeenCalledWith("run awk '{print $$}'", 'pc2');
    wrapper.unmount();
  });

  it('Tab 切换到剪贴板模式并渲染历史', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('.qp').trigger('keydown', { key: 'Tab' });
    await flushPromises();
    expect(wrapper.findAll('.qp-list .item').length).toBe(1);
    expect(wrapper.findAll('.qp-list .item')[0].text()).toContain('复制过的文本');

    // 剪贴板模式下 Enter 直接粘贴
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.invokePaste).toHaveBeenCalledWith('复制过的文本', undefined);
    wrapper.unmount();
  });

  it('预览 chip 化：{{变量}} 渲染为 .var-chip（变量名为原子片段不截断）', async () => {
    const wrapper = await mountPanel();
    const rows = wrapper.findAll('.qp-list .item');
    // p2 内容「本周完成：{{本周工作|工作内容}}」→ chip 只含变量名
    const chips = rows[1].findAll('.item-preview .var-chip');
    expect(chips.length).toBe(1);
    expect(chips[0].text()).toBe('本周工作');
    // chip 之外的纯文本照常渲染，不被吞掉
    expect(rows[1].find('.item-preview').text()).toContain('本周完成：');
    // 无变量的 p1 预览不产生 chip
    expect(rows[0].findAll('.item-preview .var-chip').length).toBe(0);
    wrapper.unmount();
  });

  it('页脚 Enter 提示动态化：手动变量数 > 0 → 「填写 N 个变量」，否则「粘贴」', async () => {
    const data: AppData = {
      ...fixture,
      prompts: [
        makePrompt({ id: 'pn1', title: '无变量', content: '请审查这段代码' }),
        makePrompt({ id: 'pn2', title: '多变量', content: '{{a}} 和 {{b}}，重复的 {{a}} 不计' }),
      ],
    };
    const wrapper = await mountPanel(data);
    const footText = () => wrapper.find('.qp-foot').text();
    // 无手动变量 → 粘贴
    expect(footText()).toContain('粘贴');
    // N = 去重后的手动变量数：{{a}}/{{b}} 计 2，重复的 {{a}} 不计
    await wrapper.find('.qp').trigger('keydown', { key: 'ArrowDown' });
    expect(footText()).toContain('填写 2 个变量');
    wrapper.unmount();
  });

  it('自动变量 {{clipboard}} 不计入填写提示（Enter 直接粘贴不弹表单）', async () => {
    const data: AppData = {
      ...fixture,
      prompts: [makePrompt({ id: 'pa', title: '自动变量', content: 'x{{clipboard}}' })],
    };
    const wrapper = await mountPanel(data);
    const foot = wrapper.find('.qp-foot').text();
    expect(foot).toContain('粘贴');
    expect(foot).not.toContain('填写');
    wrapper.unmount();
  });

  it('剪贴板 tab 分组延伸：时间分组头 + 类型图标，与主窗口同族', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('.qp').trigger('keydown', { key: 'Tab' });
    await flushPromises();
    expect(wrapper.findAll('.qp-list .grp').length).toBeGreaterThan(0);
    expect(wrapper.find('.qp-list .item .kind-ico').exists()).toBe(true);
    // 分组后的行仍是 .item，Enter 粘贴行为不变
    await wrapper.find('.qp').trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.invokePaste).toHaveBeenCalledWith('复制过的文本', undefined);
    wrapper.unmount();
  });

  it('pm-panel-shown 事件重置搜索会话', async () => {
    const wrapper = await mountPanel();
    await wrapper.find('input.qp-search').setValue('代码');
    window.dispatchEvent(new CustomEvent('pm-panel-shown'));
    await flushPromises();
    expect((wrapper.find('input.qp-search').element as HTMLInputElement).value).toBe('');
    wrapper.unmount();
  });

  it('回归：变量窗取消后焦点回到搜索框，↑↓ 导航恢复', async () => {
    const wrapper = await mountPanel();
    // 通过 open-prompt 事件打开变量窗（p2 含 {{本周工作}}）
    const { listen } = await import('@tauri-apps/api/event');
    const openCall = vi
      .mocked(listen)
      .mock.calls.find(([name]) => name === 'open-prompt')!;
    (openCall[1] as (e: { payload: string }) => void)({ payload: 'p2' });
    await flushPromises();

    const dialog = wrapper.findComponent(VarDialog);
    expect(dialog.exists()).toBe(true);
    // 变量窗挂载后焦点在其输入框上
    expect(document.activeElement?.tagName).toBe('TEXTAREA');

    // Esc 取消 → 变量窗卸载
    await dialog.findAll('textarea')[0].trigger('keydown', { key: 'Escape' });
    await flushPromises();
    expect(wrapper.findComponent(VarDialog).exists()).toBe(false);

    // 修复目标：焦点必须回到搜索框——否则键盘事件落在 body 上，
    // 不再经过面板根元素，↑↓/Enter/Esc 全部失效（修前红：activeElement 是 body）
    expect(document.activeElement).toBe(wrapper.find('input.qp-search').element);

    // 用户可见行为：方向键导航恢复
    await wrapper.find('.qp').trigger('keydown', { key: 'ArrowDown' });
    expect(wrapper.findAll('.qp-list .item')[1].classes()).toContain('active');
    wrapper.unmount();
  });

  it('回归：列表内容包在 .qp-list-inner 中，供高度测量取自然内容高', async () => {
    // 高度自适应以 .qp-list-inner 的自然高度为测量源；窗口被最小高度
    // 托底时由面板填满窗口（height:100vh），不再露出空白底条
    const wrapper = await mountPanel();
    const inner = wrapper.find('.qp-list .qp-list-inner');
    expect(inner.exists()).toBe(true);
    expect(wrapper.findAll('.qp-list .qp-list-inner .item').length).toBe(3);
    wrapper.unmount();
  });

  // 评审 2026-09-10 I1：浮层 .detail-body 特意设 user-select:text 支持选中，
  // 无条件 preventDefault 会吞掉 Ctrl+C/Ctrl+A 的 keydown 默认行为，
  // Chromium 中 copy 事件随之不派发——浮层内无法键盘复制
  it('全文浮层放行 Ctrl/Meta 组合键：Ctrl+C 不被吞，其余按键仍拦截', async () => {
    const wrapper = await mountPanel();
    (wrapper.vm as unknown as { detailOpen: boolean }).detailOpen = true;

    const ctrlC = new KeyboardEvent('keydown', { key: 'c', ctrlKey: true, cancelable: true });
    wrapper.find('.qp').element.dispatchEvent(ctrlC);
    expect(ctrlC.defaultPrevented).toBe(false);

    const ctrlA = new KeyboardEvent('keydown', { key: 'a', ctrlKey: true, cancelable: true });
    wrapper.find('.qp').element.dispatchEvent(ctrlA);
    expect(ctrlA.defaultPrevented).toBe(false);

    // 无修饰键仍拦截：防止浮层后的面板动作（Enter 粘贴等）误触发
    const enter = new KeyboardEvent('keydown', { key: 'Enter', cancelable: true });
    wrapper.find('.qp').element.dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(true);

    // Esc/← 仍关闭浮层
    const esc = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
    wrapper.find('.qp').element.dispatchEvent(esc);
    expect(esc.defaultPrevented).toBe(true);
    expect((wrapper.vm as unknown as { detailOpen: boolean }).detailOpen).toBe(false);
    wrapper.unmount();
  });
});
