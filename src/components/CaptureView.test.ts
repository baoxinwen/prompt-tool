import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import CaptureView from './CaptureView.vue';
import { api } from '../lib/api';
import type { AppData } from '../types';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock('../lib/api', () => ({
  api: {
    getData: vi.fn(async () => ({}) as AppData),
    closeCapture: vi.fn(async () => undefined),
    savePrompt: vi.fn(async () => undefined),
  },
}));

const mockedApi = vi.mocked(api, true);

async function mountView() {
  const wrapper = mount(CaptureView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

function winKey(init: Partial<KeyboardEvent> & { key: string }) {
  return new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init } as KeyboardEventInit);
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('CaptureView：快速捕获窗口', () => {
  it('焦点落在 body 时按 Esc 仍能关闭窗口（window 级监听，评审 I5）', async () => {
    const wrapper = await mountView();
    // 模拟点击非可聚焦区域后焦点落 body：事件 target 是 body，不经过组件根 div
    (document.activeElement as HTMLElement | null)?.blur();
    expect(document.activeElement).toBe(document.body);
    window.dispatchEvent(winKey({ key: 'Escape' }));
    await flushPromises();
    expect(mockedApi.closeCapture).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('焦点落在 body 时按 Ctrl+S 仍能保存（评审 I5）', async () => {
    const wrapper = await mountView();
    (document.activeElement as HTMLElement | null)?.blur();
    await wrapper.find('textarea').setValue('捕获的内容');
    window.dispatchEvent(winKey({ key: 's', ctrlKey: true }));
    await flushPromises();
    expect(mockedApi.savePrompt).toHaveBeenCalledTimes(1);
    expect(mockedApi.savePrompt.mock.calls[0][0].content).toBe('捕获的内容');
    expect(mockedApi.closeCapture).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it('输入法组合态 Esc 是取消候选词，不关闭窗口（评审 I2）', async () => {
    const wrapper = await mountView();
    window.dispatchEvent(winKey({ key: 'Escape', isComposing: true }));
    await flushPromises();
    expect(mockedApi.closeCapture).not.toHaveBeenCalled();
    // 非组合态对照：正常关闭
    window.dispatchEvent(winKey({ key: 'Escape' }));
    await flushPromises();
    expect(mockedApi.closeCapture).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });
});
