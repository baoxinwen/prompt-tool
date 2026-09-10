import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import Manager from './Manager.vue';
import { api } from '../lib/api';
import type { AppData, Settings } from '../types';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock('@tauri-apps/api/window', () => {
  const win = {
    isMaximized: vi.fn(async () => false),
    unmaximize: vi.fn(async () => undefined),
    startDragging: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
    minimize: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
  };
  return { getCurrentWindow: () => win };
});

import { getCurrentWindow } from '@tauri-apps/api/window';
const winMock = getCurrentWindow() as unknown as {
  isMaximized: ReturnType<typeof vi.fn>;
  unmaximize: ReturnType<typeof vi.fn>;
  startDragging: ReturnType<typeof vi.fn>;
  toggleMaximize: ReturnType<typeof vi.fn>;
};

vi.mock('../lib/api', () => ({
  api: {
    getData: vi.fn(),
    getRecoveryNotice: vi.fn(async () => null),
    checkUpdate: vi.fn(async () => ({ kind: 'up-to-date' })),
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
    setPanelHeight: vi.fn(async () => undefined),
  },
}));

const mockedApi = vi.mocked(api, true);

function makeData(settings: Partial<Settings>): AppData {
  return {
    version: 1,
    seeded: true,
    settings: {
      autoUpdateCheck: false,
      webdav: { enabled: false, url: '', username: '', password: '', lastSyncAt: 0 },
      gist: { enabled: false, token: '', gist_id: '', lastSyncAt: 0 },
      ...settings,
    } as Settings,
    categories: ['开发'],
    prompts: [],
    clipboard: [],
    tombstones: [],
  } as AppData;
}

async function mountManager(data = makeData({})) {
  mockedApi.getData.mockResolvedValue(data);
  const wrapper = mount(Manager, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

function setGuardFalse(wrapper: ReturnType<typeof mount>) {
  // 直接置守卫为「有未保存修改」：等价于 SyncPane/PromptsPane dirty 时注册的守卫
  (wrapper.vm as unknown as { leaveGuard: () => boolean }).leaveGuard = () => false;
}

beforeEach(() => {
  vi.clearAllMocks();
  winMock.isMaximized.mockResolvedValue(false);
});

describe('Manager：标签页切换必须收敛到 switchTab（评审 2026-09-10 I2）', () => {
  it('Ctrl+K 在有未保存修改时先弹确认，而不是静默丢弃并切换', async () => {
    const wrapper = await mountManager();
    // 切到云同步页（无守卫时正常切换）
    await wrapper.findAll('.nav-btn')[2].trigger('click');
    await flushPromises();
    setGuardFalse(wrapper);

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, cancelable: true }),
    );
    await flushPromises();

    // ConfirmDialog Teleport 到 body，不在 wrapper DOM 内
    expect(document.body.textContent).toContain('有未保存的修改');
    // 仍停留在云同步页：第一个导航按钮不得变为选中
    expect(wrapper.findAll('.nav-btn')[0].classes()).not.toContain('on');
    wrapper.unmount();
  });

  it('Ctrl+K 确认「丢弃并离开」后切换到提示词页并聚焦搜索', async () => {
    const wrapper = await mountManager();
    await wrapper.findAll('.nav-btn')[2].trigger('click');
    await flushPromises();
    setGuardFalse(wrapper);

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, cancelable: true }),
    );
    await flushPromises();

    // 确认对话框的危险确认键（Teleport 到 body）
    const confirmBtn = document.body.querySelector('.cd-confirm') as HTMLButtonElement | null;
    expect(confirmBtn).not.toBeNull();
    confirmBtn!.click();
    await flushPromises();

    expect(wrapper.findAll('.nav-btn')[0].classes()).toContain('on');
    wrapper.unmount();
  });

  it('「去更新」toast 在有未保存修改时同样先过守卫确认', async () => {
    vi.useFakeTimers();
    try {
      mockedApi.checkUpdate.mockResolvedValue({
        kind: 'available',
        version: '9.9.9',
      } as Awaited<ReturnType<typeof api.checkUpdate>>);
      const wrapper = await mountManager(makeData({ autoUpdateCheck: true }));
      setGuardFalse(wrapper);

      await vi.advanceTimersByTimeAsync(3_000);
      await flushPromises();

      const action = wrapper.find('.toast-action');
      expect(action.exists()).toBe(true);
      await action.trigger('click');
      await flushPromises();
      vi.useRealTimers();
      await flushPromises();

      // ConfirmDialog Teleport 到 body
      expect(document.body.textContent).toContain('有未保存的修改');
      // 设置页导航按钮不得选中（仍是提示词页）
      expect(wrapper.findAll('.nav-btn')[4].classes()).not.toContain('on');
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('Manager：标题栏双击在 mousedown 内收敛（评审 2026-09-10 I3）', () => {
  it('普通窗口快速两次按下 = 双击 → 最大化，只进入一次拖拽', async () => {
    const wrapper = await mountManager();
    const tb = wrapper.find('.tb');
    await tb.trigger('mousedown', { button: 0 });
    await flushPromises();
    await tb.trigger('mousedown', { button: 0 });
    await flushPromises();

    expect(winMock.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(winMock.unmaximize).not.toHaveBeenCalled();
    expect(winMock.startDragging).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('最大化窗口按下先还原并拖拽；紧随的第二次按下不得重新最大化（原生双击还原语义）', async () => {
    // 只有第一次按下时窗口处于最大化：unmaximize 后 isMaximized 应返回 false
    winMock.isMaximized.mockResolvedValueOnce(true).mockResolvedValue(false);
    const wrapper = await mountManager();
    const tb = wrapper.find('.tb');
    await tb.trigger('mousedown', { button: 0 });
    await flushPromises();
    await tb.trigger('mousedown', { button: 0 });
    await flushPromises();

    expect(winMock.unmaximize).toHaveBeenCalledTimes(1);
    expect(winMock.toggleMaximize).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
