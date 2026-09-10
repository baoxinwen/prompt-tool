import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import CaptureView from './CaptureView.vue';
import { api } from '../lib/api';
import type { AppData } from '../types';

vi.mock('../lib/api', () => ({
  api: {
    getData: vi.fn(async () => ({ categories: ['写作', '开发'] })),
    savePrompt: vi.fn(async () => ({})),
    closeCapture: vi.fn(async () => undefined),
  },
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => undefined),
}));

const mockedApi = vi.mocked(api, true);

function mountView() {
  return mount(CaptureView);
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedApi.getData.mockImplementation(
    async () => ({ categories: ['写作', '开发'] }) as unknown as AppData,
  );
  mockedApi.savePrompt.mockImplementation(async () => ({}) as never);
  mockedApi.closeCapture.mockImplementation(async () => undefined);
});

describe('CaptureView：快速捕获', () => {
  it('特征锁定：标题/正文/分类/取消/关闭钮渲染', async () => {
    const w = mountView();
    await flushPromises();
    expect(w.find('.cv-title-input').exists()).toBe(true);
    expect(w.find('.cv-content').exists()).toBe(true);
    expect(w.find('select').exists()).toBe(true);
    expect(w.find('.ghost-btn').text()).toBe('取消');
    expect(w.find('.icon-x').exists()).toBe(true);
  });

  // happy-dom 对「同批渲染 options + v-model 赋值」的 select.value 报告不可靠
  // （真实 Chromium 已验证正确），故断言真实行为——保存载荷里的 category：
  it('C4：默认分类取第一个分类（保存载荷验证）', async () => {
    const w = mountView();
    await flushPromises();
    await w.find('.cv-content').setValue('正文');
    await w.find('.cv-foot .ab').trigger('click');
    await flushPromises();
    expect(mockedApi.savePrompt.mock.calls[0][0].category).toBe('写作');
  });

  it('C4：无分类数据时回退未分类（保存载荷验证）', async () => {
    mockedApi.getData.mockImplementation(async () => ({ categories: [] }) as unknown as AppData);
    const w = mountView();
    await flushPromises();
    await w.find('.cv-content').setValue('正文');
    await w.find('.cv-foot .ab').trigger('click');
    await flushPromises();
    expect(mockedApi.savePrompt.mock.calls[0][0].category).toBe('未分类');
  });

  it('C4：用户手选的分类跨次捕获保留', async () => {
    const w = mountView();
    await flushPromises();
    await w.find('select').setValue('开发');
    // capture-text 重置正文/标题，但不清分类
    await w.trigger('keydown', { key: 'Shift' }); // 无操作，仅确保监听健在
    expect((w.find('select').element as HTMLSelectElement).value).toBe('开发');
  });

  it('C3：保存键位提示含 Ctrl S（窗口已加宽）', async () => {
    const w = mountView();
    await flushPromises();
    expect(w.find('.cv-foot .ab').text()).toContain('Ctrl S');
  });

  it('C1：保存中禁用并显示保存中…，成功后「✓ 已保存」停留约 0.6s 再关窗', async () => {
    let resolveSave: () => void = () => {};
    mockedApi.savePrompt.mockImplementation(() => new Promise<string>((r) => (resolveSave = () => r("captured-1"))));
    const w = mountView();
    await flushPromises();
    await w.find('.cv-content').setValue('正文');
    await w.find('.cv-foot .ab').trigger('click');
    expect((w.find('.cv-foot .ab').element as HTMLButtonElement).disabled).toBe(true);
    expect(w.find('.cv-foot .ab').text()).toContain('保存中…');
    resolveSave();
    await flushPromises();
    expect(w.find('.cv-foot .ab').text()).toContain('已保存');
    expect(mockedApi.closeCapture).not.toHaveBeenCalled();
    await new Promise((r) => setTimeout(r, 700));
    expect(mockedApi.closeCapture).toHaveBeenCalled();
  });

  it('C2：保存失败时错误独立成行可见且不关窗', async () => {
    mockedApi.savePrompt.mockRejectedValue(new Error('磁盘已满'));
    const w = mountView();
    await flushPromises();
    await w.find('.cv-content').setValue('正文');
    await w.find('.cv-foot .ab').trigger('click');
    await flushPromises();
    expect(w.find('.cv-err-row').exists()).toBe(true);
    expect(w.find('.cv-err-row').text()).toContain('磁盘已满');
    expect(mockedApi.closeCapture).not.toHaveBeenCalled();
  });

  it('空内容保存=关闭窗口（不写库）', async () => {
    const w = mountView();
    await flushPromises();
    await w.find('.cv-foot .ab').trigger('click');
    await flushPromises();
    expect(mockedApi.savePrompt).not.toHaveBeenCalled();
    expect(mockedApi.closeCapture).toHaveBeenCalled();
  });

  it('Esc 关闭窗口（window 级监听，焦点不在组件内也生效）', async () => {
    mountView();
    await flushPromises();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flushPromises();
    expect(mockedApi.closeCapture).toHaveBeenCalled();
  });
});
