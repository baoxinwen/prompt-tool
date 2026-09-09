import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import ClipboardPane from './ClipboardPane.vue';
import { api } from '../lib/api';
import { managerKey, type ManagerCtx } from '../lib/context';
import type { AppData, ClipboardItem } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    clearHistory: vi.fn(async () => 0),
    restoreHistory: vi.fn(async () => 0),
    deleteHistoryItem: vi.fn(async () => undefined),
    copyText: vi.fn(async () => undefined),
    copyImage: vi.fn(async () => undefined),
    getImageThumb: vi.fn(async () => ''),
    saveSettings: vi.fn(async () => undefined),
  },
}));

const mockedApi = vi.mocked(api, true);

// 固定「现在」，分组断言不依赖真实时钟：2026-09-08 12:00 周二
const NOW = new Date(2026, 8, 8, 12, 0);

function clipItem(
  id: string,
  content: string,
  copiedAt = NOW.getTime() - 60_000,
  kind: 'text' | 'image' = 'text',
  image?: ClipboardItem['image'],
): ClipboardItem {
  return { id, content, copiedAt, kind, ...(image ? { image } : {}) };
}

function makeData(clipboard: ClipboardItem[]): AppData {
  return {
    version: 1,
    seeded: true,
    settings: {
      hotkey: 'alt+q',
      captureHotkey: 'alt+s',
      captureClipboard: true,
      restoreClipboard: true,
      pasteAppendEnter: false,
      theme: 'light',
      webdav: {
        enabled: false,
        autoSync: false,
        url: '',
        username: '',
        password: '',
      },
      gist: { enabled: false, autoSync: false, token: '', gistId: '' },
      syncProvider: 'webdav',
      syncClipboard: false,
    },
    categories: [],
    prompts: [],
    clipboard,
    tombstones: [],
  };
}

function makeCtx(data: AppData) {
  const confirmCalls: string[] = [];
  const ctx = {
    data: ref<AppData | null>(data),
    refresh: vi.fn(async () => {}),
    toast: vi.fn(),
    confirm: async (opts: { title: string }) => {
      confirmCalls.push(opts.title);
      return true;
    },
  };
  return { ctx: ctx as unknown as ManagerCtx, raw: ctx, confirmCalls };
}

function mountPane(data: AppData) {
  const { ctx, raw, confirmCalls } = makeCtx(data);
  const wrapper = mount(ClipboardPane, {
    global: { provide: { [managerKey as symbol]: ctx } },
  });
  return { wrapper, raw, confirmCalls };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('ClipboardPane：清空确认框', () => {
  it('搜索态下确认框显示全量条数，而不是过滤后的匹配数', async () => {
    const { wrapper, confirmCalls } = mountPane(
      makeData([clipItem('c1', '代码审查记录'), clipItem('c2', '会议纪要')]),
    );
    await flushPromises();

    await wrapper.find('.search-box input').setValue('代码');
    await wrapper.find('.clear-btn').trigger('click');
    await flushPromises();

    expect(confirmCalls).toHaveLength(1);
    expect(confirmCalls[0]).toContain('2 条');
    wrapper.unmount();
  });

  it('清空确认后调用 clearHistory 并刷新', async () => {
    const { wrapper, raw } = mountPane(makeData([clipItem('c1', 'a')]));
    await flushPromises();
    await wrapper.find('.clear-btn').trigger('click');
    await flushPromises();
    expect(mockedApi.clearHistory).toHaveBeenCalledTimes(1);
    expect(raw.refresh).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });
});

describe('ClipboardPane：时间分组与搜索反馈', () => {
  it('按 今天/更早 分组渲染，组头只出现有内容的分组', async () => {
    vi.useFakeTimers({ now: NOW });
    const today = new Date(2026, 8, 8, 9, 0).getTime();
    const earlier = new Date(2025, 5, 1, 9, 0).getTime();
    const { wrapper } = mountPane(
      makeData([clipItem('c1', '很久以前', earlier), clipItem('c2', '今早复制', today)]),
    );
    await flushPromises();

    const grps = wrapper.findAll('.grp').map((n) => n.text());
    // 组头按展示顺序且只出现非空分组
    expect(grps).toEqual(['今天', '更早']);
    // 各组行归属：第一组是今天的行，第二组是更早的行
    const rows = wrapper.findAll('.row-item');
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain('今早复制');
    expect(rows[1].text()).toContain('很久以前');
    wrapper.unmount();
  });

  it('搜索时顶部显示「找到 N 条」hitline', async () => {
    vi.useFakeTimers({ now: NOW });
    const { wrapper } = mountPane(
      makeData([clipItem('c1', '代码审查记录'), clipItem('c2', '会议纪要')]),
    );
    await flushPromises();

    await wrapper.find('.search-box input').setValue('代码');
    await flushPromises();
    expect(wrapper.find('.hitline').text()).toContain('找到 1 条');
    wrapper.unmount();
  });

  it('搜索无结果显示无结果空态，不误显示「暂无记录」', async () => {
    vi.useFakeTimers({ now: NOW });
    const { wrapper } = mountPane(makeData([clipItem('c1', '代码审查记录')]));
    await flushPromises();

    await wrapper.find('.search-box input').setValue('不存在的词');
    await flushPromises();
    const empty = wrapper.find('.empty');
    expect(empty.exists()).toBe(true);
    expect(empty.text()).toContain('没有匹配');
    expect(empty.text()).not.toContain('暂无记录');
    wrapper.unmount();
  });
});

describe('ClipboardPane：图片行复制', () => {
  it('图片行复制调用 copyImage 而不是 copyText', async () => {
    vi.useFakeTimers({ now: NOW });
    const { wrapper } = mountPane(
      makeData([
        clipItem('img1', '', NOW.getTime(), 'image', {
          file: 'img1.png',
          width: 10,
          height: 10,
        }),
        clipItem('c1', '普通文本'),
      ]),
    );
    await flushPromises();

    const imgRow = wrapper.find('.row-item.img');
    expect(imgRow.exists()).toBe(true);
    await imgRow.find('.copy-btn').trigger('click');
    await flushPromises();

    expect(mockedApi.copyImage).toHaveBeenCalledWith('img1');
    expect(mockedApi.copyText).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});

describe('ClipboardPane：清空撤销', () => {
  it('清空后 toast 带撤销动作，点击动作调用 restoreHistory 并刷新', async () => {
    vi.useFakeTimers({ now: NOW });
    mockedApi.clearHistory.mockResolvedValue(2);
    const { wrapper, raw } = mountPane(
      makeData([clipItem('c1', 'a'), clipItem('c2', 'b')]),
    );
    await flushPromises();

    await wrapper.find('.clear-btn').trigger('click');
    await flushPromises();

    expect(mockedApi.clearHistory).toHaveBeenCalledTimes(1);
    expect(raw.toast).toHaveBeenCalledTimes(1);
    const [msg, , action] = raw.toast.mock.calls[0];
    expect(String(msg)).toContain('已清空 2 条');
    expect(action?.label).toBe('撤销');

    await action!.handler();
    await flushPromises();
    expect(mockedApi.restoreHistory).toHaveBeenCalledTimes(1);
    expect(raw.refresh).toHaveBeenCalled();
    wrapper.unmount();
  });
});

describe('ClipboardPane：全文浮层', () => {
  it('点击行打开全文浮层，Esc 关闭', async () => {
    vi.useFakeTimers({ now: NOW });
    const { wrapper } = mountPane(makeData([clipItem('c1', '第一行\n第二行')]));
    await flushPromises();

    await wrapper.find('.row-item').trigger('click');
    await flushPromises();
    expect(wrapper.find('.detail').exists()).toBe(true);
    expect(wrapper.find('.detail-body').text()).toContain('第二行');

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flushPromises();
    expect(wrapper.find('.detail').exists()).toBe(false);
    wrapper.unmount();
  });
});
