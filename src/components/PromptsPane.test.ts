import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import PromptsPane from './PromptsPane.vue';
import { api } from '../lib/api';
import { managerKey, type ManagerCtx } from '../lib/context';
import type { AppData, Prompt } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    savePrompt: vi.fn(async () => undefined),
    deletePrompt: vi.fn(async () => undefined),
    copyText: vi.fn(async () => undefined),
    addCategory: vi.fn(async () => undefined),
    renameCategory: vi.fn(async () => undefined),
    deleteCategory: vi.fn(async () => undefined),
    getVarMemory: vi.fn(async () => ({})),
    saveVarMemory: vi.fn(async () => undefined),
    invokePaste: vi.fn(async () => undefined),
    getClipboardText: vi.fn(async () => null),
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

const p1 = makePrompt({ id: 'p1', title: '代码审查', createdAt: 2_000 });
const p2 = makePrompt({ id: 'p2', title: '翻译助手', category: '写作', createdAt: 3_000 });

function makeCtx(prompts: Prompt[]) {
  const toasts: Array<{ msg: string; kind?: string; action?: unknown }> = [];
  let refreshes = 0;
  let confirmResult = true;
  const confirmCalls: string[] = [];
  // 离开守卫（评审 I6）：记录注册的守卫，供守卫行为断言使用
  let leaveGuard: (() => boolean) | null = null;
  const data = ref<AppData | null>({
    version: 1,
    seeded: true,
    settings: {} as AppData['settings'],
    categories: ['开发', '写作'],
    prompts,
  } as AppData);
  const ctx = {
    data,
    refresh: async () => {
      refreshes++;
    },
    toast: (msg: string, kind?: 'ok' | 'err', action?: { label: string; handler: () => void | Promise<void> }) => {
      toasts.push({ msg, kind, action });
    },
    confirm: async (opts: { title: string }) => {
      confirmCalls.push(opts.title);
      return confirmResult;
    },
    setLeaveGuard: (g: (() => boolean) | null) => {
      leaveGuard = g;
    },
    toasts,
    confirmResultRef: () => (confirmResult = !confirmResult),
    refreshCount: () => refreshes,
    confirmCalls,
    leaveGuard: () => leaveGuard,
  } as unknown as ManagerCtx & {
    toasts: typeof toasts;
    refreshCount: () => number;
    confirmCalls: string[];
    leaveGuard: () => (() => boolean) | null;
  };
  return ctx;
}

async function mountPane(prompts: Prompt[] = [p1, p2]) {
  const ctx = makeCtx(prompts);
  const wrapper = mount(PromptsPane, {
    global: { provide: { [managerKey as symbol]: ctx } },
    attachTo: document.body,
  });
  await flushPromises();
  return { wrapper, ctx };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedApi.getVarMemory.mockImplementation(async () => ({}));
  mockedApi.getClipboardText.mockImplementation(async () => null);
});

/** 底栏已无保存按钮：手动保存统一走 Ctrl+S */
function pressCtrlS() {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true }));
}

describe('PromptsPane：提示词管理', () => {
  it('初始渲染列表，右侧为空状态（含标题文案）', async () => {
    const { wrapper } = await mountPane();
    expect(wrapper.findAll('.pitem').length).toBe(2);
    expect(wrapper.text()).toContain('从左侧选择提示词');
    expect(wrapper.find('.pp-count').text()).toBe('2');
    wrapper.unmount();
  });

  it('点击列表项载入草稿并高亮', async () => {
    const { wrapper } = await mountPane();
    await wrapper.findAll('.pitem')[0].trigger('click');
    await flushPromises();
    expect(wrapper.find('.pitem.on').exists()).toBe(true);
    const title = wrapper.find('input.d-title').element as HTMLInputElement;
    expect(title.value).toBe('代码审查');
    expect((wrapper.find('textarea.d-content').element as HTMLTextAreaElement).value).toContain(
      '内容-代码审查',
    );
    wrapper.unmount();
  });

  it('编辑标题后出现未保存标记，保存调用 savePrompt 并刷新', async () => {
    const { wrapper, ctx } = await mountPane();
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('input.d-title').setValue('代码审查（改）');
    await flushPromises();
    expect(wrapper.find('.data-save').text()).toContain('未保存');

    pressCtrlS();
    await flushPromises();

    expect(mockedApi.savePrompt).toHaveBeenCalledTimes(1);
    expect(mockedApi.savePrompt.mock.calls[0][0].title).toBe('代码审查（改）');
    expect(mockedApi.savePrompt.mock.calls[0][0].id).toBe('p1');
    expect(ctx.refreshCount()).toBe(1);
    expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toBe('已保存');
    wrapper.unmount();
  });

  it('空标题禁止保存', async () => {
    const { wrapper, ctx } = await mountPane();
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('input.d-title').setValue('   ');
    pressCtrlS();
    await flushPromises();

    expect(mockedApi.savePrompt).not.toHaveBeenCalled();
    expect(ctx.toasts[ctx.toasts.length - 1]).toMatchObject({ msg: '请填写标题', kind: 'err' });
    wrapper.unmount();
  });

  it('置顶切换进入草稿，保存后持久化 pinned', async () => {
    const { wrapper } = await mountPane();
    await wrapper.findAll('.pitem')[1].trigger('click');
    await wrapper.findAll('.mini-btn').find((b) => b.text().includes('置顶'))!.trigger('click');
    pressCtrlS();
    await flushPromises();

    expect(mockedApi.savePrompt.mock.calls[0][0].pinned).toBe(true);
    wrapper.unmount();
  });

  it('删除需确认：确认后调用 deletePrompt 并给出可撤销提示', async () => {
    const { wrapper, ctx } = await mountPane();
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('.ghost-btn.danger').trigger('click');
    await flushPromises();

    expect(ctx.confirmCalls[0]).toContain('代码审查');
    expect(mockedApi.deletePrompt).toHaveBeenCalledWith('p1');
    expect(ctx.refreshCount()).toBe(1);
    expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toContain('已删除');
    expect((ctx.toasts[ctx.toasts.length - 1]?.action as { label?: string })?.label).toBe('撤销');
    expect(wrapper.find('.d-title').exists()).toBe(false);
    wrapper.unmount();
  });

  it('删除取消时不调用后端', async () => {
    const { wrapper, ctx } = await mountPane([p1]);
    (ctx as unknown as { confirm: () => Promise<boolean> }).confirm = async () => false;
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('.ghost-btn.danger').trigger('click');
    await flushPromises();

    expect(mockedApi.deletePrompt).not.toHaveBeenCalled();
    expect(wrapper.find('input.d-title').exists()).toBe(true);
    wrapper.unmount();
  });

  it('搜索框过滤列表（子串），无结果显示空状态', async () => {
    const { wrapper } = await mountPane();
    await wrapper.find('.search-box input').setValue('翻译');
    await flushPromises();
    let titles = wrapper.findAll('.pitem-title').map((t) => t.text());
    expect(titles).toHaveLength(1);
    expect(titles[0]).toContain('翻译助手');

    await wrapper.find('.search-box input').setValue('不存在的词');
    await flushPromises();
    expect(wrapper.text()).toContain('暂无提示词');
    wrapper.unmount();
  });

  it('分类 chip 过滤', async () => {
    const { wrapper } = await mountPane();
    const chip = wrapper.findAll('.cat-row .chip').find((c) => c.text().includes('写作'))!;
    await chip.trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.pitem').length).toBe(1);
    expect(wrapper.findAll('.pitem-title')[0].text()).toContain('翻译助手');
    wrapper.unmount();
  });

  it('分类重命名输入框：输入法组合态 Enter 不提交（评审 I2）', async () => {
    const { wrapper } = await mountPane();
    const chip = wrapper.findAll('.cat-row .chip').find((c) => c.text().includes('开发'))!;
    await chip.find('.chip-ops svg').trigger('click');
    const input = wrapper.find('input.chip-input');
    expect(input.exists()).toBe(true);
    await input.setValue('开发改');
    // 组合态 Enter：拼音上屏候选词，不得提交（此刻 editName 还是旧名，提交会静默丢失编辑）
    await input.trigger('keydown', { key: 'Enter', isComposing: true });
    expect(mockedApi.renameCategory).not.toHaveBeenCalled();
    // 正常 Enter 提交
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.renameCategory).toHaveBeenCalledWith('开发', '开发改');
    wrapper.unmount();
  });

  it('新建分类输入框：输入法组合态 Enter 不提交（评审 I2）', async () => {
    const { wrapper } = await mountPane();
    await wrapper.find('.chip-add').trigger('click');
    const input = wrapper.find('input.chip-input');
    expect(input.exists()).toBe(true);
    await input.setValue('新分类');
    await input.trigger('keydown', { key: 'Enter', isComposing: true });
    expect(mockedApi.addCategory).not.toHaveBeenCalled();
    await input.trigger('keydown', { key: 'Enter' });
    await flushPromises();
    expect(mockedApi.addCategory).toHaveBeenCalledWith('新分类');
    wrapper.unmount();
  });

  it('新建提示词：空白草稿，保存时无 id', async () => {
    const { wrapper } = await mountPane();
    await wrapper.findAll('button').find((b) => b.text().includes('新建'))!.trigger('click');
    await flushPromises();

    const title = wrapper.find('input.d-title').element as HTMLInputElement;
    expect(title.value).toBe('');
    // 新建时删除按钮禁用（还没有 id）
    expect((wrapper.find('.ghost-btn.danger').element as HTMLButtonElement).disabled).toBe(true);

    await wrapper.find('input.d-title').setValue('全新提示词');
    await wrapper.find('textarea.d-content').setValue('正文');
    pressCtrlS();
    await flushPromises();

    expect(mockedApi.savePrompt.mock.calls[0][0].id).toBe('');
    expect(mockedApi.savePrompt.mock.calls[0][0].title).toBe('全新提示词');
    wrapper.unmount();
  });

  it('Ctrl+S 快捷键触发保存', async () => {
    const { wrapper } = await mountPane();
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('input.d-title').setValue('快捷键保存');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true }));
    await flushPromises();

    expect(mockedApi.savePrompt).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  // ---------- 离开守卫（评审 I6 回归） ----------

  it('挂载注册离开守卫，卸载注销', async () => {
    const { wrapper, ctx } = await mountPane();
    const guard = (ctx as unknown as { leaveGuard: () => (() => boolean) | null }).leaveGuard();
    expect(guard).toBeTypeOf('function');

    wrapper.unmount();
    const after = (ctx as unknown as { leaveGuard: () => (() => boolean) | null }).leaveGuard();
    expect(after).toBeNull();
  });

  it('守卫反映草稿脏状态：干净可离开，编辑后不可', async () => {
    const { wrapper, ctx } = await mountPane();
    const guard = (ctx as unknown as { leaveGuard: () => (() => boolean) | null }).leaveGuard()!;

    expect(guard()).toBe(true);

    // 载入草稿并改动 → 守卫必须拦下
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('textarea.d-content').setValue('改动后的正文');
    expect(guard()).toBe(false);

    wrapper.unmount();
  });

  // ---------- 列表行重构（M4） ----------

  it('分类徽章并入标题行（badge 模式），meta 行不再渲染分类圆点', async () => {
    const { wrapper } = await mountPane();
    const first = wrapper.findAll('.pitem')[0];
    const badgeInTitle = first.find('.pitem-title .badge');
    expect(badgeInTitle.exists()).toBe(true);
    expect(badgeInTitle.text()).toBe('开发');
    // 圆点形态退出列表行
    expect(first.find('.pitem-row .dot').exists()).toBe(false);
    expect(first.find('.pitem-row .badge').exists()).toBe(false);
    wrapper.unmount();
  });

  it('无快捷键/次数/变量的行不渲染 meta 行', async () => {
    // 翻译助手：hotkey 空、useCount 0、内容无变量
    const { wrapper } = await mountPane([p2]);
    expect(wrapper.findAll('.pitem-row').length).toBe(0);
    wrapper.unmount();
  });

  it('meta 行渲染「N 个变量」，{{clipboard}} 自动变量不计入', async () => {
    const pVar = makePrompt({
      id: 'pv',
      title: '带变量',
      content: '把 {{文本|要翻译的内容}} 译成 {{语言}}，参考剪贴板：{{clipboard}}',
    });
    const { wrapper } = await mountPane([pVar]);
    const row = wrapper.find('.pitem-row');
    expect(row.exists()).toBe(true);
    expect(row.text()).toContain('2 个变量');

    // 只有 {{clipboard}}：无需手填 → 不渲染 meta 行
    const pAuto = makePrompt({ id: 'pa', title: '仅剪贴板', content: '粘贴：{{clipboard}}' });
    const { wrapper: w2 } = await mountPane([pAuto]);
    expect(w2.findAll('.pitem-row').length).toBe(0);
    w2.unmount();
    wrapper.unmount();
  });

  it('快捷键在 meta 行以大写规范化键帽显示', async () => {
    const pHot = makePrompt({ id: 'ph', title: '带快捷键', hotkey: 'ctrl+alt+1' });
    const { wrapper } = await mountPane([pHot]);
    const kc = wrapper.find('.pitem-row .kc');
    expect(kc.exists()).toBe(true);
    expect(kc.text()).toBe('CtrlAlt1');
    wrapper.unmount();
  });

  it('预览接 previewSegments：变量 chip 完整渲染，放不下整 chip 丢弃以 … 收尾', async () => {
    // chip 放得下：完整渲染变量名
    const pFit = makePrompt({
      id: 'pf',
      title: '放得下',
      content: '前缀文字 {{目标语言}}',
      updatedAt: 5_000,
    });
    // 40 字前缀 + 变量：预算 44 内放不下整 chip → 整 chip 丢弃，绝不出现半截变量
    const pCut = makePrompt({
      id: 'pc',
      title: '放不下',
      content: '前'.repeat(40) + '{{目标语言英文}}尾部文字',
      updatedAt: 4_000,
    });
    const { wrapper } = await mountPane([pFit, pCut]);

    const fit = wrapper.findAll('.pitem')[0].find('.pitem-preview');
    const fitChip = fit.find('.var-mark');
    expect(fitChip.exists()).toBe(true);
    expect(fitChip.text()).toBe('目标语言');

    const cut = wrapper.findAll('.pitem')[1].find('.pitem-preview');
    expect(cut.findAll('.var-mark').length).toBe(0);
    expect(cut.text()).toContain('…');
    expect(cut.text()).not.toContain('目标');
    wrapper.unmount();
  });

  // ---------- M5 变量卡·镜像高亮·自动保存·主操作 ----------

  const pVars = makePrompt({
    id: 'pv',
    title: '带变量',
    content: '把 {{文本|要翻译的内容}} 译成 {{语言}}，剪贴板：{{clipboard}}',
  });

  it('变量卡：打开草稿按变量记忆预填，placeholder 用变量提示，{{clipboard}} 不进卡', async () => {
    mockedApi.getVarMemory.mockImplementation(async () => ({ 语言: '英文' }));
    const { wrapper } = await mountPane([pVars]);
    await wrapper.findAll('.pitem')[0].trigger('click');
    await flushPromises();

    expect(mockedApi.getVarMemory).toHaveBeenCalledWith('pv');
    const card = wrapper.find('.d-card-vars');
    expect(card.exists()).toBe(true);
    const inputs = card.findAll('input');
    expect(inputs.length).toBe(2);
    expect((inputs[0].element as HTMLInputElement).value).toBe('');
    expect(inputs[0].attributes('placeholder')).toBe('要翻译的内容');
    expect((inputs[1].element as HTMLInputElement).value).toBe('英文');
    expect(inputs[1].attributes('placeholder')).toBe('填入 语言');
    expect(card.text()).not.toContain('clipboard');
    wrapper.unmount();
  });

  it('镜像高亮：先转义再把 {{…}} 全片段包 var-mark，textarea 透明文字层结构就位', async () => {
    const p = makePrompt({ id: 'pm', title: '高亮', content: 'a <b> & {{名|提示}} c' });
    const { wrapper } = await mountPane([p]);
    await wrapper.findAll('.pitem')[0].trigger('click');
    await flushPromises();

    expect(wrapper.find('.ed-wrap').exists()).toBe(true);
    expect(wrapper.find('pre.ed-mirror').exists()).toBe(true);
    expect(wrapper.find('textarea.ed-input.d-content').exists()).toBe(true);

    const html = wrapper.find('.ed-mirror').element.innerHTML;
    expect(html).toContain('&lt;b&gt; &amp;');
    expect(html).not.toContain('<b>');
    expect(html).toContain('<span class="var-mark">{{名|提示}}</span>');
    wrapper.unmount();
  });

  it('自动保存：编辑防抖 900ms 后静默保存——不 toast、刷新列表、指示「已自动保存」', async () => {
    vi.useFakeTimers();
    try {
      const { wrapper, ctx } = await mountPane();
      await wrapper.findAll('.pitem')[0].trigger('click');
      await wrapper.find('input.d-title').setValue('自动保存标题');
      expect(mockedApi.savePrompt).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(899);
      expect(mockedApi.savePrompt).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(1);
      expect(mockedApi.savePrompt).toHaveBeenCalledTimes(1);
      expect(mockedApi.savePrompt.mock.calls[0][0].title).toBe('自动保存标题');
      expect(mockedApi.savePrompt.mock.calls[0][0].id).toBe('p1');
      expect(ctx.refreshCount()).toBe(1);
      expect(ctx.toasts.length).toBe(0);
      expect(wrapper.find('.data-save').text()).toContain('已自动保存');
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it('自动保存失败：toast 错误且指示保持「未保存」', async () => {
    vi.useFakeTimers();
    try {
      mockedApi.savePrompt.mockRejectedValueOnce(new Error('磁盘写入失败'));
      const { wrapper, ctx } = await mountPane();
      await wrapper.findAll('.pitem')[0].trigger('click');
      await wrapper.find('input.d-title').setValue('失败场景');
      await vi.advanceTimersByTimeAsync(900);

      expect(ctx.toasts[0]?.kind).toBe('err');
      expect(ctx.toasts[0]?.msg).toContain('磁盘写入失败');
      expect(wrapper.find('.data-save').text()).toContain('未保存');
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it('填写变量并粘贴：applyVars 替换 + {{clipboard}} 自动填充 + 记忆回存 + 计数 toast', async () => {
    mockedApi.getClipboardText.mockImplementation(async () => '剪贴板内容');
    const { wrapper, ctx } = await mountPane([pVars]);
    await wrapper.findAll('.pitem')[0].trigger('click');
    await flushPromises();

    const inputs = wrapper.findAll('.d-card-vars input');
    await inputs[0].setValue('你好世界');

    await wrapper.find('.fill-paste-btn').trigger('click');
    await flushPromises();

    expect(mockedApi.invokePaste).toHaveBeenCalledTimes(1);
    expect(mockedApi.invokePaste.mock.calls[0][0]).toBe('把 你好世界 译成 ，剪贴板：剪贴板内容');
    expect(mockedApi.invokePaste.mock.calls[0][1]).toBe('pv');
    expect(mockedApi.saveVarMemory).toHaveBeenCalledWith('pv', { 文本: '你好世界', 语言: '' });
    expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toBe('已填写 1/2 个变量并粘贴');
    wrapper.unmount();
  });

  it('无手动变量时「填写变量并粘贴」退化为直接粘贴', async () => {
    const { wrapper, ctx } = await mountPane([p1]);
    await wrapper.findAll('.pitem')[0].trigger('click');
    await wrapper.find('.fill-paste-btn').trigger('click');
    await flushPromises();

    expect(mockedApi.invokePaste).toHaveBeenCalledWith(p1.content, 'p1');
    expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toBe('已粘贴');
    wrapper.unmount();
  });

  it('复制：原文含占位符复制，按钮短暂显示「已复制 ✓」，toast 说明含占位符', async () => {
    vi.useFakeTimers();
    try {
      const { wrapper, ctx } = await mountPane([pVars]);
      await wrapper.findAll('.pitem')[0].trigger('click');
      await flushPromises();

      await wrapper.find('.copy-btn').trigger('click');
      expect(mockedApi.copyText).toHaveBeenCalledWith(pVars.content);
      expect(wrapper.find('.copy-btn').text()).toContain('已复制 ✓');
      expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toContain('占位符');
      // 按钮层级（R1 评审修正）：复制=实心墨块 primary（AccentButton）最右，
      // 填写变量并粘贴=次级 strong（面板底+描边+粗体）
      expect(wrapper.find('.copy-btn').classes()).toContain('ab');
      expect(wrapper.find('.fill-paste-btn').classes()).not.toContain('ab');
      expect(wrapper.find('.fill-paste-btn').classes()).toContain('strong-btn');

      await vi.advanceTimersByTimeAsync(1_200);
      expect(wrapper.find('.copy-btn').text()).not.toContain('已复制 ✓');
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });
});
