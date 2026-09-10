import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import UpdateSection from './UpdateSection.vue';
import { api } from '../lib/api';
import { lastAutoUpdate, resetUpdateState } from '../lib/updateCache';
import { managerKey, type ManagerCtx } from '../lib/context';
import type { AppData, Settings, UpdateProgress } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    checkUpdate: vi.fn(),
    downloadAndInstallUpdate: vi.fn(),
    openReleasesPage: vi.fn(),
    onUpdateProgress: vi.fn(async () => () => {}),
    saveSettings: vi.fn(async () => {}),
  },
}));

const mockedApi = vi.mocked(api, true);

const baseSettings: Settings = {
  hotkey: 'ctrl+shift+q',
  captureHotkey: 'alt+s',
  captureClipboard: true,
  restoreClipboard: true,
  pasteAppendEnter: false,
  theme: 'dark',
  webdav: { enabled: false, autoSync: false, url: '', username: '', password: '' },
  gist: { enabled: false, autoSync: false, token: '', gistId: '' },
  syncProvider: 'webdav',
  syncClipboard: false,
  autoUpdateCheck: true,
  skippedUpdateVersion: null,
};

const ctx = {
  data: ref<AppData | null>({ settings: { ...baseSettings } } as unknown as AppData),
  refresh: vi.fn(async () => {}),
  toast: vi.fn(),
  confirm: async () => true,
  setLeaveGuard: () => {},
} satisfies ManagerCtx;

const mountSection = () =>
  mount(UpdateSection, { global: { provide: { [managerKey as symbol]: ctx } } });

beforeEach(() => {
  vi.clearAllMocks();
  lastAutoUpdate.value = null; // 模块级缓存，测试间必须复位
  resetUpdateState(); // 更新状态机也是模块级（I7），同样必须复位
});

describe('UpdateSection', () => {
  it('idle 态显示当前版本与检查按钮', () => {
    const w = mountSection();
    expect(w.text()).toContain(`当前版本 v${__APP_VERSION__}`);
    expect(w.find('button.check').text()).toContain('检查更新');
  });

  it('手动检查 → available：显示版本、日志与三个操作', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '- 修复了 X' });
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    expect(w.text()).toContain('1.1.0');
    expect(w.text()).toContain('- 修复了 X');
    expect(w.find('button.install').exists()).toBe(true);
    expect(w.find('button.skip').exists()).toBe(true);
    expect(w.find('button.releases').exists()).toBe(true);
  });

  it('upToDate 与 error 态', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'upToDate' });
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    expect(w.text()).toContain('已是最新');

    mockedApi.checkUpdate.mockResolvedValue({ kind: 'error', errorKind: 'network', message: 'timeout' });
    await w.find('button.check').trigger('click');
    await flushPromises();
    expect(w.text()).toContain('网络'); // errorKind=network 的文案
    expect(w.text()).toContain('timeout'); // errMsg 原始信息可见
    expect(w.find('button.retry').exists()).toBe(true);
    expect(w.find('button.releases').exists()).toBe(true);
  });

  it('跳过此版本：写入 settings 并刷新', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    await w.find('button.skip').trigger('click');
    await flushPromises();
    expect(mockedApi.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ skippedUpdateVersion: '1.1.0' }),
    );
    expect(ctx.refresh).toHaveBeenCalled();
  });

  it('下载进度：percent 驱动进度条，100% 进入 installing', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    let emit: (p: UpdateProgress) => void = () => {};
    mockedApi.onUpdateProgress.mockImplementation(async (cb) => { emit = cb; return () => {}; });
    mockedApi.downloadAndInstallUpdate.mockReturnValue(new Promise<void>(() => {})); // 安装中永不 resolve
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    await w.find('button.install').trigger('click');
    emit({ downloaded: 50, total: 100, percent: 50 });
    await flushPromises();
    expect(w.find('.progress').exists()).toBe(true);
    expect(w.text()).toContain('50%');
    emit({ downloaded: 100, total: 100, percent: 100 });
    await flushPromises();
    expect(w.text()).toContain('安装中');
    expect(w.find('button.install').attributes('disabled')).toBeDefined();
  });

  it('下载/安装中禁止重新检查', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    let emit: (p: UpdateProgress) => void = () => {};
    mockedApi.onUpdateProgress.mockImplementation(async (cb) => { emit = cb; return () => {}; });
    mockedApi.downloadAndInstallUpdate.mockReturnValue(new Promise<void>(() => {})); // 安装中永不 resolve
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    await w.find('button.install').trigger('click');
    emit({ downloaded: 50, total: 100, percent: 50 });
    await flushPromises();
    await w.find('button.check').trigger('click');
    await flushPromises();
    expect(mockedApi.checkUpdate).toHaveBeenCalledTimes(1);
    expect(w.find('.progress').exists()).toBe(true); // phase 未被拉走，进度区仍在
  });

  it('挂载时消费自动检查缓存：直接展示新版本与三按钮，无需手动检查', async () => {
    lastAutoUpdate.value = { kind: 'available', version: '1.2.0', notes: '- 自动检查发现' };
    const w = mountSection();
    await flushPromises();
    expect(mockedApi.checkUpdate).not.toHaveBeenCalled(); // 免二次手动检查
    expect(w.text()).toContain('1.2.0');
    expect(w.text()).toContain('- 自动检查发现');
    expect(w.find('button.install').exists()).toBe(true);
    expect(w.find('button.skip').exists()).toBe(true);
    expect(w.find('button.releases').exists()).toBe(true);
    expect(lastAutoUpdate.value).toBeNull(); // 一次性消费，避免复挂载时复活旧结果
  });

  it('安装失败：错误文案为「更新失败」而非「检查更新失败」', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    mockedApi.onUpdateProgress.mockImplementation(async () => () => {});
    mockedApi.downloadAndInstallUpdate.mockRejectedValueOnce(new Error('install boom'));
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    await w.find('button.install').trigger('click');
    await flushPromises();
    expect(w.text()).toContain('更新失败');
    expect(w.text()).not.toContain('检查更新失败');
    expect(w.text()).toContain('install boom'); // errMsg 原始信息可见
  });

  it('进度订阅失败：进入错误态而非卡死在下载中', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    mockedApi.onUpdateProgress.mockRejectedValueOnce(new Error('listen boom'));
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    await w.find('button.install').trigger('click');
    await flushPromises();
    expect(w.find('.progress').exists()).toBe(false); // 未卡死在 downloading
    expect(w.text()).toContain('更新失败');
    expect(w.text()).toContain('listen boom');
    expect(w.find('button.check').attributes('disabled')).toBeUndefined(); // 错误态可重新检查
  });

  it('下载进度：total 未知（percent=null）时显示已下载 MB 而非 0%', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    let emit: (p: UpdateProgress) => void = () => {};
    mockedApi.onUpdateProgress.mockImplementation(async (cb) => { emit = cb; return () => {}; });
    mockedApi.downloadAndInstallUpdate.mockReturnValue(new Promise<void>(() => {})); // 安装中永不 resolve
    const w = mountSection();
    await w.find('button.check').trigger('click');
    await flushPromises();
    await w.find('button.install').trigger('click');
    emit({ downloaded: 1572864, total: null, percent: null }); // 1572864 / 1048576 = 1.5 MB
    await flushPromises();
    expect(w.find('.progress').exists()).toBe(true);
    expect(w.text()).toContain('已下载 1.5 MB');
    expect(w.text()).not.toContain('下载中 0%');
  });

  // 评审 2026-09-10 I7：下载中卸载组件（切页）不得丢失状态——
  // 重挂载后 phase 仍在 downloading，不得二次触发 download_and_install
  it('下载中卸载后重挂载：phase 保持下载中，不重复触发下载', async () => {
    mockedApi.checkUpdate.mockResolvedValue({ kind: 'available', version: '1.1.0', notes: '' });
    mockedApi.onUpdateProgress.mockImplementation(async () => () => {});
    mockedApi.downloadAndInstallUpdate.mockReturnValue(new Promise<void>(() => {}));

    const w1 = mountSection();
    await w1.find('button.check').trigger('click');
    await flushPromises();
    await w1.find('button.install').trigger('click');
    await flushPromises();
    w1.unmount(); // 切页卸载：后端下载仍在进行

    const w2 = mountSection(); // 回到设置页重挂载
    await flushPromises();
    expect(w2.find('.progress').exists()).toBe(true); // phase 仍为 downloading，进度区可见

    await w2.find('button.install').trigger('click');
    await flushPromises();
    expect(mockedApi.downloadAndInstallUpdate).toHaveBeenCalledTimes(1); // 不得二次触发
    w2.unmount();
  });
});
