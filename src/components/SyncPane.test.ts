import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import SyncPane from './SyncPane.vue';
import { api } from '../lib/api';
import { managerKey, type ManagerCtx } from '../lib/context';
import type { AppData, Settings } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    saveSettings: vi.fn(async () => undefined),
    webdavTest: vi.fn(),
    gistTest: vi.fn(),
    syncNow: vi.fn(),
  },
}));

const mockedApi = vi.mocked(api, true);

const baseSettings: Settings = {
  hotkey: 'alt+q',
  captureHotkey: 'alt+s',
  captureClipboard: true,
  restoreClipboard: true,
  pasteAppendEnter: false,
  theme: 'light',
  webdav: {
    enabled: false,
    autoSync: false,
    url: 'https://dav.example.com/dav/prompt-tool/',
    username: 'user@example.com',
    password: 'app-password',
  },
  gist: { enabled: false, autoSync: false, token: 'ghp_seed', gistId: 'gist123' },
  syncProvider: 'webdav',
  syncClipboard: false,
};

function makeCtx(settings: Settings, extra: Partial<AppData> = {}) {
  const toasts: Array<{ msg: string; kind?: string }> = [];
  let refreshes = 0;
  let confirmResult = true;
  const confirmCalls: string[] = [];
  // 离开守卫（评审 I9）：记录注册/注销供断言
  let leaveGuard: (() => boolean) | null = null;
  const ctx = {
    data: ref<AppData | null>({ settings, prompts: [], clipboard: [], ...extra } as unknown as AppData),
    refresh: async () => {
      refreshes++;
    },
    toast: (msg: string, kind?: 'ok' | 'err') => {
      toasts.push({ msg, kind });
    },
    confirm: async (opts: { title: string }) => {
      confirmCalls.push(opts.title);
      return confirmResult;
    },
    setLeaveGuard: (g: (() => boolean) | null) => {
      leaveGuard = g;
    },
    toasts,
    refreshCount: () => refreshes,
    confirmCalls,
    leaveGuard: () => leaveGuard,
    setConfirm(v: boolean) {
      confirmResult = v;
    },
  };
  return ctx as typeof ctx & ManagerCtx;
}

async function mountPane(settings: Settings = JSON.parse(JSON.stringify(baseSettings))) {
  const ctx = makeCtx(settings);
  const wrapper = mount(SyncPane, {
    global: { provide: { [managerKey as symbol]: ctx } },
  });
  await flushPromises();
  return { wrapper, ctx };
}

function inputByLabel(wrapper: ReturnType<typeof mount>, label: string) {
  const field = wrapper.findAll('label.field').find((l) => l.text().includes(label));
  if (!field) throw new Error(`找不到输入项: ${label}`);
  return field.find('input');
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('SyncPane：云同步设置', () => {
  it('挂载时回显后端配置（WebDAV 地址/账号/密码、未启用徽章）', async () => {
    const { wrapper } = await mountPane();
    expect((inputByLabel(wrapper, '服务器地址').element as HTMLInputElement).value).toBe(
      'https://dav.example.com/dav/prompt-tool/',
    );
    expect((inputByLabel(wrapper, '账号').element as HTMLInputElement).value).toBe('user@example.com');
    expect((inputByLabel(wrapper, '密码').element as HTMLInputElement).type).toBe('password');
    expect(wrapper.find('.on-badge').text()).toContain('未启用');
    expect(wrapper.text()).not.toContain('GitHub Token');
  });

  it('启用后徽章变为已启用；http 地址显示明文警告', async () => {
    const settings = JSON.parse(JSON.stringify(baseSettings));
    settings.webdav.enabled = true;
    const { wrapper } = await mountPane(settings);
    expect(wrapper.find('.on-badge').text()).toContain('已启用');

    const davSwitch = wrapper
      .findAll('label.row.opt')
      .find((l) => l.text().includes('启用 WebDAV 同步'))!
      .find('input');
    await davSwitch.setValue(true);
    expect(wrapper.find('.on-badge').text()).toContain('已启用');

    await inputByLabel(wrapper, '服务器地址').setValue('http://insecure.example.com/dav/');
    await flushPromises();
    expect(wrapper.find('.warn-hint').exists()).toBe(true);
  });

  it('切换 provider：显示 Gist 表单并隐藏 WebDAV 表单', async () => {
    const { wrapper } = await mountPane();
    await wrapper.findAll('.seg-item').find((b) => b.text().includes('GitHub Gist'))!.trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('GitHub Token');
    expect(wrapper.text()).not.toContain('服务器地址');
    expect(wrapper.find('.on-badge').text()).toContain('未启用');
  });

  it('保存配置：合并当前表单写入 settings（含 trim）并提示刷新', async () => {
    const { wrapper, ctx } = await mountPane();
    await inputByLabel(wrapper, '服务器地址').setValue('  https://trimmed.example.com/dav/  ');
    await wrapper
      .findAll('.btns button')
      .find((b) => b.text().includes('保存配置'))!
      .trigger('click');
    await flushPromises();

    const payload = mockedApi.saveSettings.mock.calls[0][0];
    expect(payload.syncProvider).toBe('webdav');
    expect(payload.webdav.url).toBe('https://trimmed.example.com/dav/');
    expect(payload.webdav.password).toBe('app-password');
    expect(payload.gist.token).toBe('ghp_seed');
    expect(ctx.toasts[ctx.toasts.length - 1]?.msg).toBe('同步配置已保存');
    expect(ctx.refreshCount()).toBe(1);
  });

  it('测试连接（WebDAV）：成功显示 ✓，失败显示 ✗', async () => {
    mockedApi.webdavTest.mockResolvedValueOnce('连接成功，目录可读写');
    const { wrapper } = await mountPane();
    await wrapper.findAll('.btns button').find((b) => b.text().includes('测试连接'))!.trigger('click');
    await flushPromises();

    expect(mockedApi.webdavTest).toHaveBeenCalledWith(
      'https://dav.example.com/dav/prompt-tool/',
      'user@example.com',
      'app-password',
    );
    const status = wrapper.find('.status');
    expect(status.classes()).toContain('ok');
    expect(status.text()).toContain('✓ 连接成功');

    mockedApi.webdavTest.mockRejectedValueOnce('创建网盘目录失败 (HTTP 403)');
    await wrapper.findAll('.btns button').find((b) => b.text().includes('测试连接'))!.trigger('click');
    await flushPromises();
    const errStatus = wrapper.find('.status');
    expect(errStatus.classes()).toContain('err');
    expect(errStatus.text()).toContain('✗ 创建网盘目录失败 (HTTP 403)');
  });

  it('测试连接（Gist）：按当前 provider 调用 gistTest', async () => {
    mockedApi.gistTest.mockResolvedValueOnce('Token 有效（账号 octocat），Gist 可访问');
    const { wrapper } = await mountPane();
    await wrapper.findAll('.seg-item').find((b) => b.text().includes('GitHub Gist'))!.trigger('click');
    await wrapper.findAll('.btns button').find((b) => b.text().includes('测试连接'))!.trigger('click');
    await flushPromises();

    expect(mockedApi.gistTest).toHaveBeenCalledWith('ghp_seed', 'gist123');
    expect(wrapper.find('.status').text()).toContain('✓ Token 有效');
  });

  it('立即同步：先静默保存再调用 syncNow("merge")', async () => {
    mockedApi.syncNow.mockResolvedValueOnce({ added: 1, updated: 2, removed: 0, message: '同步完成' });
    const { wrapper, ctx } = await mountPane();
    await wrapper.findAll('button.ob').find((b) => b.text().includes('立即同步'))!.trigger('click');
    await flushPromises();

    expect(mockedApi.saveSettings).toHaveBeenCalledTimes(1);
    expect(mockedApi.syncNow).toHaveBeenCalledWith('merge');
    expect(wrapper.find('.status').text()).toContain('✓ 同步完成');
    expect(ctx.confirmCalls.length).toBe(0);
  });

  it('仅上传/仅下载是破坏性操作：必须确认，取消则不调用 syncNow', async () => {
    const { wrapper, ctx } = await mountPane();
    ctx.setConfirm(false);

    await wrapper.findAll('button.ob').find((b) => b.text().includes('仅上传'))!.trigger('click');
    await flushPromises();
    expect(ctx.confirmCalls[0]).toContain('仅上传：用本机数据覆盖云端');
    expect(mockedApi.syncNow).not.toHaveBeenCalled();

    await wrapper.findAll('button.ob').find((b) => b.text().includes('仅下载'))!.trigger('click');
    await flushPromises();
    expect(ctx.confirmCalls[1]).toContain('仅下载：用云端数据覆盖本机');
    expect(mockedApi.syncNow).not.toHaveBeenCalled();

    ctx.setConfirm(true);
    mockedApi.syncNow.mockResolvedValue({ added: 0, updated: 0, removed: 0, message: '已上传' });
    await wrapper.findAll('button.ob').find((b) => b.text().includes('仅上传'))!.trigger('click');
    await flushPromises();
    expect(mockedApi.syncNow).toHaveBeenCalledWith('push');
  });

  it('同步失败时状态条显示错误', async () => {
    mockedApi.syncNow.mockRejectedValueOnce('GitHub Token 无效');
    const { wrapper } = await mountPane();
    await wrapper.findAll('button.ob').find((b) => b.text().includes('立即同步'))!.trigger('click');
    await flushPromises();

    expect(wrapper.find('.status').classes()).toContain('err');
    expect(wrapper.find('.status').text()).toContain('✗ GitHub Token 无效');
  });

  it('同步范围开关：syncClipboard 写入保存载荷', async () => {
    const { wrapper } = await mountPane();
    const scope = wrapper
      .findAll('label.row.opt')
      .find((l) => l.text().includes('云同步包含剪贴板历史'))!
      .find('input');
    await scope.setValue(true);

    await wrapper
      .findAll('.btns button')
      .find((b) => b.text().includes('保存配置'))!
      .trigger('click');
    await flushPromises();
    expect(mockedApi.saveSettings.mock.calls[0][0].syncClipboard).toBe(true);
  });

  it('保存失败时以 err 提示且不触发同步', async () => {
    mockedApi.saveSettings.mockRejectedValueOnce('磁盘写入失败');
    const { wrapper, ctx } = await mountPane();
    await wrapper.findAll('button.ob').find((b) => b.text().includes('立即同步'))!.trigger('click');
    await flushPromises();

    expect(ctx.toasts[ctx.toasts.length - 1]).toMatchObject({ kind: 'err' });
    expect(mockedApi.syncNow).not.toHaveBeenCalled();
  });

  // ---------- 评审 I6/I9/I12 ----------

  it('凭据存储说明与实现一致：不声称存于 data.json（评审 I6）', async () => {
    const { wrapper } = await mountPane();
    const note = wrapper.find('.note').text();
    expect(note).not.toContain('data.json');
    expect(note).toContain('凭据管理器');
  });

  it('挂载注册离开守卫：表单脏时不可静默离开，卸载注销（评审 I9）', async () => {
    const { wrapper, ctx } = await mountPane();
    const guard = (ctx as unknown as { leaveGuard: () => (() => boolean) | null }).leaveGuard();
    expect(guard).toBeTypeOf('function');
    // 表单干净：可离开
    expect(guard!()).toBe(true);
    // 改动表单后：守卫拦截
    await inputByLabel(wrapper, '服务器地址').setValue('https://dirty.example.com/dav/');
    expect(guard!()).toBe(false);
    // 卸载后注销，不影响其它页
    wrapper.unmount();
    expect(
      (ctx as unknown as { leaveGuard: () => (() => boolean) | null }).leaveGuard(),
    ).toBeNull();
  });

  it('后端自动创建 Gist 回填 id：表单脏时不被保存回滚成空（评审 I12）', async () => {
    const settings = JSON.parse(JSON.stringify(baseSettings)) as Settings;
    settings.syncProvider = 'gist';
    settings.gist.gistId = '';
    const { wrapper, ctx } = await mountPane(settings);
    // 用户改动了表单（脏），随后自动同步在后端创建了 Gist 并写回 settings
    await inputByLabel(wrapper, 'GitHub Token').setValue('ghp_new');
    ctx.data.value = {
      version: 1,
      seeded: true,
      settings: { ...JSON.parse(JSON.stringify(settings)), gist: { ...settings.gist, gistId: 'gistNEW' } },
      categories: [],
      prompts: [],
      clipboard: [],
      tombstones: [],
    } as AppData;
    await flushPromises();

    // gistId 输入框跟随后端回填
    expect((inputByLabel(wrapper, 'Gist ID').element as HTMLInputElement).value).toBe('gistNEW');
    // 保存不再把后端的 gist_id 覆盖回空串（点 AccentButton 保存，不是第一个按钮「测试连接」）
    await wrapper.find('.btns .ab').trigger('click');
    await flushPromises();
    expect(mockedApi.saveSettings.mock.calls[0][0].gist.gistId).toBe('gistNEW');
  });
});

describe('SyncPane：单卡收敛（v3）', () => {
  it('状态三态徽章：未配置 / 已配置未启用 st-ready / 已启用 st-on', async () => {
    const unset = JSON.parse(JSON.stringify(baseSettings));
    unset.webdav.url = '';
    const { wrapper: wUnset } = await mountPane(unset);
    expect(wUnset.find('.state-badge').text()).toContain('未配置');
    expect(wUnset.find('.on-badge').text()).toContain('未配置');

    const { wrapper: wReady } = await mountPane();
    expect(wReady.find('.state-badge').classes()).toContain('st-ready');

    const on = JSON.parse(JSON.stringify(baseSettings));
    on.webdav.enabled = true;
    const { wrapper: wOn } = await mountPane(on);
    expect(wOn.find('.state-badge').classes()).toContain('st-on');

    // configured 按 provider 取必填项：gist → token
    const gistNoToken = JSON.parse(JSON.stringify(baseSettings));
    gistNoToken.gist.token = '';
    const { wrapper: wGist } = await mountPane(gistNoToken);
    expect(wGist.find('.state-badge').classes()).toContain('st-ready');
    await wGist.findAll('.seg-item').find((b) => b.text().includes('GitHub Gist'))!.trigger('click');
    await flushPromises();
    expect(wGist.find('.state-badge').text()).toContain('未配置');
  });

  it('同步门槛：未配置时三个同步按钮禁用、syncNow 不被调用、出现「去填写」', async () => {
    const settings = JSON.parse(JSON.stringify(baseSettings));
    settings.webdav.url = '';
    const { wrapper } = await mountPane(settings);

    const syncBtns = wrapper.findAll('button.ob').filter((b) => !b.text().includes('去填写'));
    expect(syncBtns.length).toBe(3);
    for (const b of syncBtns) expect(b.attributes('disabled')).toBeDefined();

    await syncBtns.find((b) => b.text().includes('立即同步'))!.trigger('click');
    await flushPromises();
    expect(mockedApi.syncNow).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('去填写');
  });

  it('去填写：WebDAV 聚焦服务器地址输入框，Gist 聚焦 Token 输入框', async () => {
    const settings = JSON.parse(JSON.stringify(baseSettings));
    settings.webdav.url = '';
    settings.gist.token = '';
    // focus 断言要求组件真实挂到 document 上（VTU 默认挂到游离节点，activeElement 不变）
    const ctx = makeCtx(settings);
    const wrapper = mount(SyncPane, {
      attachTo: document.body,
      global: { provide: { [managerKey as symbol]: ctx } },
    });
    await flushPromises();
    try {
      await wrapper.findAll('button').find((b) => b.text().includes('去填写'))!.trigger('click');
      expect(document.activeElement).toBe(inputByLabel(wrapper, '服务器地址').element);

      await wrapper
        .findAll('.seg-item')
        .find((b) => b.text().includes('GitHub Gist'))!
        .trigger('click');
      await flushPromises();
      await wrapper.findAll('button').find((b) => b.text().includes('去填写'))!.trigger('click');
      expect(document.activeElement).toBe(inputByLabel(wrapper, 'GitHub Token').element);
    } finally {
      wrapper.unmount();
    }
  });

  it('上次同步：有值显示 relativeTime，无值显示「尚未同步」', async () => {
    const withTs = JSON.parse(JSON.stringify(baseSettings));
    withTs.lastSyncAt = Date.now() - 5 * 60_000;
    const { wrapper } = await mountPane(withTs);
    expect(wrapper.text()).toContain('上次同步 · 5 分钟前');

    const { wrapper: wNone } = await mountPane();
    expect(wNone.text()).toContain('尚未同步');
    expect(wNone.text()).not.toContain('上次同步');
  });

  it('上次同步随 30s 定时刷新（刚刚 → 1 分钟前）', async () => {
    vi.useFakeTimers();
    try {
      const t0 = Date.now();
      const settings = JSON.parse(JSON.stringify(baseSettings));
      settings.lastSyncAt = t0 - 30_000;
      const ctx = makeCtx(settings);
      const wrapper = mount(SyncPane, {
        global: { provide: { [managerKey as symbol]: ctx } },
      });
      expect(wrapper.text()).toContain('上次同步 · 刚刚');
      await vi.advanceTimersByTimeAsync(31_000);
      expect(wrapper.text()).toContain('上次同步 · 1 分钟前');
    } finally {
      vi.useRealTimers();
    }
  });

  it('动作完成后状态回填相对时间：✓ … · 刚刚', async () => {
    mockedApi.syncNow.mockResolvedValueOnce({ added: 0, updated: 0, removed: 0, message: '同步完成' });
    const { wrapper } = await mountPane();
    await wrapper.findAll('button.ob').find((b) => b.text().includes('立即同步'))!.trigger('click');
    await flushPromises();
    expect(wrapper.find('.status').text()).toContain('✓ 同步完成 · 刚刚');
  });

  it('条目统计：显示提示词与剪贴板数量', async () => {
    const settings = JSON.parse(JSON.stringify(baseSettings));
    const ctx = makeCtx(settings, {
      prompts: [{ id: 'p1' }, { id: 'p2' }],
      clipboard: [{ id: 'c1' }],
    } as Partial<AppData>);
    const wrapper = mount(SyncPane, {
      global: { provide: { [managerKey as symbol]: ctx } },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('2 条提示词');
    expect(wrapper.text()).toContain('1 条剪贴板');
  });

  it('开关语义小字：启用「只影响自动同步」/ 自动同步 / 剪贴板上云提示', async () => {
    const { wrapper } = await mountPane();
    expect(wrapper.text()).toContain('只影响自动同步，仍可手动同步');
    expect(wrapper.text()).toContain('启动时与内容变更后自动合并');
    expect(wrapper.text()).toContain('敏感内容也会上云，多设备需保持一致');
    expect(wrapper.text()).toContain('删除也会同步到其他设备');
  });
});
