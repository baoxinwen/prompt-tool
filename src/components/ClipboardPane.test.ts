import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import ClipboardPane from './ClipboardPane.vue';
import { api } from '../lib/api';
import { managerKey, type ManagerCtx } from '../lib/context';
import type { AppData, ClipboardItem } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    clearHistory: vi.fn(async () => undefined),
    deleteHistoryItem: vi.fn(async () => undefined),
    copyText: vi.fn(async () => undefined),
    getImageThumb: vi.fn(async () => ''),
    saveSettings: vi.fn(async () => undefined),
  },
}));

const mockedApi = vi.mocked(api, true);

function clipItem(id: string, content: string): ClipboardItem {
  return { id, content, copiedAt: 1_790_000_000_000, kind: 'text' };
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
    refresh: async () => {},
    toast: vi.fn(),
    confirm: async (opts: { title: string }) => {
      confirmCalls.push(opts.title);
      return true;
    },
  };
  return { ctx: ctx as unknown as ManagerCtx, confirmCalls };
}

function mountPane(data: AppData) {
  const { ctx, confirmCalls } = makeCtx(data);
  const wrapper = mount(ClipboardPane, {
    global: { provide: { [managerKey as symbol]: ctx } },
  });
  return { wrapper, confirmCalls };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('ClipboardPane：清空确认框', () => {
  it('搜索态下确认框显示全量条数，而不是过滤后的匹配数', async () => {
    const { wrapper, confirmCalls } = mountPane(
      makeData([clipItem('c1', '代码审查记录'), clipItem('c2', '会议纪要')]),
    );
    await flushPromises();

    await wrapper.find('.search-box input').setValue('代码');
    await wrapper.find('.ghost-btn.danger').trigger('click');
    await flushPromises();

    expect(confirmCalls).toHaveLength(1);
    expect(confirmCalls[0]).toContain('2 条');
    wrapper.unmount();
  });

  it('清空确认后调用 clearHistory 并刷新', async () => {
    const { wrapper } = mountPane(makeData([clipItem('c1', 'a')]));
    await flushPromises();
    await wrapper.find('.ghost-btn.danger').trigger('click');
    await flushPromises();
    expect(mockedApi.clearHistory).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });
});
