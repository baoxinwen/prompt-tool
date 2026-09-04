import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import ConfirmDialog from './ConfirmDialog.vue';

/**
 * 确认框模态键盘语义（评审 I5 回归守卫）：
 * 打开期间背景快捷键必须被阻断、焦点必须困在对话框内、
 * Enter/Esc 语义必须与底栏提示一致。
 *
 * 键盘事件用原生 dispatchEvent 构造（test-utils 的 trigger 无法
 * 在已构造的 KeyboardEvent 上补 bubbles 等只读属性）。
 */
function keydown(el: EventTarget, init: KeyboardEventInit) {
  el.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }));
}

async function mountDialog(props: Record<string, unknown> = {}) {
  const wrapper = mount(ConfirmDialog, {
    props: { open: true, title: '删除「x」？', danger: true, ...props },
    global: { stubs: { teleport: true } },
    attachTo: document.body,
  });
  await nextTick(); // 等 watch 把焦点移入确认按钮
  return wrapper;
}

describe('ConfirmDialog：模态键盘语义', () => {
  it('打开时焦点落在确认按钮上', async () => {
    const wrapper = await mountDialog();
    expect(document.activeElement?.classList.contains('cd-confirm')).toBe(true);
    wrapper.unmount();
  });

  it('焦点在确认按钮上按 Enter 触发确认，Esc 触发取消', async () => {
    const wrapper = await mountDialog();
    const confirmBtn = wrapper.find('.cd-confirm').element;
    keydown(confirmBtn, { key: 'Enter' });
    expect(wrapper.emitted('confirm')).toHaveLength(1);

    const mask = wrapper.find('.cd-mask').element;
    keydown(mask, { key: 'Escape' });
    expect(wrapper.emitted('cancel')).toHaveLength(1);
    wrapper.unmount();
  });

  it('焦点不在按钮上时按 Enter 走兜底确认路径', async () => {
    const wrapper = await mountDialog();
    (document.activeElement as HTMLElement | null)?.blur();
    keydown(document.body, { key: 'Enter' });
    expect(wrapper.emitted('confirm')).toHaveLength(1);
    wrapper.unmount();
  });

  it('焦点在取消按钮上按 Enter：取消而非确认', async () => {
    const wrapper = await mountDialog();
    const cancelBtn = wrapper.find('.cd-btn:not(.cd-confirm)').element as HTMLElement;
    cancelBtn.focus();
    keydown(cancelBtn, { key: 'Enter' });
    expect(wrapper.emitted('confirm')).toBeUndefined();
    expect(wrapper.emitted('cancel')).toHaveLength(1);
    wrapper.unmount();
  });

  it('打开期间阻断背景键盘监听（Ctrl+K 类场景）', async () => {
    const wrapper = await mountDialog();
    const background = vi.fn();
    // 模拟 Manager/PromptsPane 的背景快捷键：document 冒泡阶段监听。
    // 组件在 window 捕获阶段 stopPropagation，背景监听必须收不到
    document.addEventListener('keydown', background);
    try {
      keydown(wrapper.find('.cd-confirm').element, { key: 'k', ctrlKey: true });
      expect(background).not.toHaveBeenCalled();
    } finally {
      document.removeEventListener('keydown', background);
    }
    wrapper.unmount();
  });

  it('Tab 在对话框内循环（焦点陷阱），Shift+Tab 反向', async () => {
    const wrapper = await mountDialog();
    const focusedCls = () => (document.activeElement as HTMLElement).className;
    expect(focusedCls()).toContain('cd-confirm'); // 初始：确认按钮

    // 逐键断言循环顺序：确认 → 关闭× → 取消 → 确认
    keydown(document.activeElement!, { key: 'Tab' });
    expect(focusedCls()).toContain('cd-x');
    keydown(document.activeElement!, { key: 'Tab' });
    expect(focusedCls()).toContain('cd-btn');
    keydown(document.activeElement!, { key: 'Tab' });
    expect(focusedCls()).toContain('cd-confirm');

    // Shift+Tab 反向
    keydown(document.activeElement!, { key: 'Tab', shiftKey: true });
    expect(focusedCls()).toContain('cd-btn');
    wrapper.unmount();
  });

  it('关闭状态下不响应任何按键', async () => {
    const wrapper = await mountDialog({ open: false });
    keydown(window, { key: 'Enter' });
    keydown(window, { key: 'Escape' });
    expect(wrapper.emitted('confirm')).toBeUndefined();
    expect(wrapper.emitted('cancel')).toBeUndefined();
    wrapper.unmount();
  });
});
