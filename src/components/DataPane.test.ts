import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import DataPane from './DataPane.vue';
import { api } from '../lib/api';
import { managerKey, type ManagerCtx } from '../lib/context';
import type { AppData } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    exportData: vi.fn(),
    importData: vi.fn(),
    importPaths: vi.fn(),
    openDataDir: vi.fn(async () => undefined),
  },
}));

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn(async () => vi.fn()),
  }),
}));

const mockedApi = vi.mocked(api, true);

function makeCtx(data: unknown = { version: 1, seeded: true }) {
  const toasts: Array<{ msg: string; kind?: string }> = [];
  let refreshes = 0;
  const ctx: ManagerCtx & { toasts: typeof toasts; refreshCount: () => number } = {
    data: ref<AppData | null>(data as AppData),
    refresh: async () => {
      refreshes++;
    },
    toast: (msg: string, kind?: 'ok' | 'err') => {
      toasts.push({ msg, kind });
    },
    confirm: async () => true,
    setLeaveGuard: () => {},
    toasts,
    refreshCount: () => refreshes,
  };
  return ctx;
}

async function mountPane(data?: unknown) {
  const ctx = makeCtx(data);
  const wrapper = mount(DataPane, {
    global: { provide: { [managerKey as symbol]: ctx } },
  });
  await flushPromises();
  return { wrapper, ctx };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('DataPane：导入导出', () => {
  it('导出 JSON：默认不含剪贴板，成功后轻提示路径', async () => {
    mockedApi.exportData.mockResolvedValue('/tmp/prompt-tool-backup.json');
    const { wrapper, ctx } = await mountPane();

    const jsonBtn = wrapper.findAll('button').find((b) => b.text().includes('JSON 备份'))!;
    await jsonBtn.trigger('click');
    await flushPromises();

    expect(mockedApi.exportData).toHaveBeenCalledWith('json', false);
    expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toContain('/tmp/prompt-tool-backup.json');
    expect(ctx.toasts[ctx.toasts.length - 1]?.kind, '成功提示不带 kind（默认 ok 样式）').toBeUndefined();
  });

  it('勾选「包含剪贴板历史」后导出参数跟随变化', async () => {
    mockedApi.exportData.mockResolvedValue('x.json');
    const { wrapper } = await mountPane();

    await wrapper.find('input[type="checkbox"]').setValue(true);
    const mdBtn = wrapper.findAll('button').find((b) => b.text().includes('Markdown'))!;
    await mdBtn.trigger('click');
    await flushPromises();

    expect(mockedApi.exportData).toHaveBeenCalledWith('markdown', true);
  });

  it('导出失败时以 err 提示错误内容', async () => {
    mockedApi.exportData.mockRejectedValue('写入失败：磁盘已满');
    const { wrapper, ctx } = await mountPane();

    const jsonBtn = wrapper.findAll('button').find((b) => b.text().includes('JSON 备份'))!;
    await jsonBtn.trigger('click');
    await flushPromises();

    expect(ctx.toasts[ctx.toasts.length - 1]).toMatchObject({ msg: expect.stringContaining('磁盘已满'), kind: 'err' });
  });

  it('点击导入：成功后提示并刷新数据', async () => {
    mockedApi.importData.mockResolvedValue({ added: 3, skipped: 1, message: '新增 3 条，跳过 1 条' });
    const { wrapper, ctx } = await mountPane();

    const importBtn = wrapper.findAll('button').find((b) => b.text().includes('或点击选择文件'))!;
    await importBtn.trigger('click');
    await flushPromises();

    expect(mockedApi.importData).toHaveBeenCalledTimes(1);
    expect(ctx.toasts[ctx.toasts.length - 1]).toMatchObject({ kind: 'ok' });
    expect(ctx.refreshCount()).toBe(1);
  });

  it('用户在文件选择框取消导入：不提示也不刷新', async () => {
    mockedApi.importData.mockResolvedValue({ added: 0, skipped: 0, message: '已取消' });
    const { wrapper, ctx } = await mountPane();

    const importBtn = wrapper.findAll('button').find((b) => b.text().includes('或点击选择文件'))!;
    await importBtn.trigger('click');
    await flushPromises();

    expect(ctx.toasts.length).toBe(0);
    expect(ctx.refreshCount()).toBe(0);
  });

  it('拖拽悬停高亮落区，离开后取消高亮', async () => {
    const { wrapper } = await mountPane();
    const zone = wrapper.find('.dropzone');
    await zone.trigger('dragover');
    expect(zone.classes()).toContain('drag');
    await zone.trigger('dragleave');
    expect(zone.classes()).not.toContain('drag');
  });
});

describe('DataPane：合并语义与数据概览（M7）', () => {
  it('装饰文案：拖拽区顶部为「« 拖入备份 »」，不用 {{ }} 记号', async () => {
    const { wrapper } = await mountPane();
    const mark = wrapper.find('.dz-mark');
    expect(mark.text()).toBe('« 拖入备份 »');
    expect(mark.text()).not.toContain('{{');
  });

  it('合并语义双文案：拖拽区语义行 + 按钮旁再现放心导入提示', async () => {
    const { wrapper } = await mountPane();
    expect(wrapper.find('.merge-line').text()).toBe(
      '与现有条目相同的自动跳过，不会覆盖或删除现有数据，其余全部新增',
    );
    expect(wrapper.text()).toContain('相同的条目会自动跳过，放心导入');
  });

  it('导出降级：JSON 备份/Markdown 为次级按钮，导入保持唯一 primary', async () => {
    const { wrapper } = await mountPane();
    const primaries = wrapper.findAll('button.ab');
    expect(primaries, '导入按钮是页面唯一 primary').toHaveLength(1);
    expect(primaries[0].text()).toContain('或点击选择文件');
    const jsonBtn = wrapper.findAll('button').find((b) => b.text().includes('JSON 备份'))!;
    expect(jsonBtn.classes(), 'JSON 备份不再是主按钮').not.toContain('ab');
    const mdBtn = wrapper.findAll('button').find((b) => b.text().includes('Markdown'))!;
    expect(mdBtn.classes()).not.toContain('ab');
  });

  it('数据概览卡：显示提示词/剪贴板文本条数，点击打开数据目录', async () => {
    const data = {
      version: 1,
      seeded: true,
      prompts: [{ id: 'p1' }, { id: 'p2' }, { id: 'p3' }],
      clipboard: [
        { id: 'c1', kind: 'text' },
        { id: 'c2', kind: 'image' },
        { id: 'c3' }, // 旧数据无 kind 视为文本
      ],
    };
    const { wrapper } = await mountPane(data);

    const ov = wrapper.find('.ov-card');
    expect(ov.text()).toContain('数据概览');
    expect(ov.findAll('.tnum').map((n) => n.text())).toEqual(['3', '2']);

    await ov.findAll('button').find((b) => b.text().includes('打开数据目录'))!.trigger('click');
    expect(mockedApi.openDataDir).toHaveBeenCalledTimes(1);
  });
});
