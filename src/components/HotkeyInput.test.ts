import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import HotkeyInput from './HotkeyInput.vue';

function keyEvent(init: Partial<KeyboardEvent> & { key: string }) {
  return new KeyboardEvent('keydown', {
    bubbles: true,
    cancelable: true,
    ...init,
  } as KeyboardEventInit);
}

describe('HotkeyInput：快捷键录制', () => {
  it('按 KeyCap 展示当前组合（非 mac 平台标签）', () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: 'alt+q' } });
    const caps = wrapper.findAll('kbd').map((k) => k.text());
    expect(caps).toEqual(['Alt', 'Q']);
  });

  it('未设置时空态显示「未设置 · 点击录制」引导文案', () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' } });
    expect(wrapper.find('.key-empty').text()).toBe('未设置 · 点击录制');
    expect(wrapper.find('kbd').exists()).toBe(false);
  });

  it('点击键位槽整槽进入录制，槽内显示「按下新组合，Esc 取消」', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('.key-slot').trigger('click');
    expect(document.querySelector('.cap-mask')).toBeTruthy();
    expect(wrapper.find('.key-empty').text()).toBe('按下新组合，Esc 取消');
    wrapper.unmount();
  });

  it('点击修改后进入录制态，捕获 Ctrl+J 归一为 e.code 大写键名（由后端 normalize 再转小写注册）', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    expect(document.querySelector('.cap-mask')).toBeTruthy();

    window.dispatchEvent(keyEvent({ key: 'j', code: 'KeyJ', ctrlKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['ctrl+J']);
    expect(document.querySelector('.cap-mask')).toBeNull();
    wrapper.unmount();
  });

  it('Ctrl+Shift+数字按 e.code 归一（% → 5）；Shift 单独不算有效修饰键', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: '%', code: 'Digit5', ctrlKey: true, shiftKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['ctrl+shift+5']);
    wrapper.unmount();
  });

  it('Ctrl+Shift+标点按 e.code 归一为基础符号（? → /），注册器可解析（评审 I4）', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    // Shift 按下时 e.key 是变体符号 '?'，必须归一成基础符号 '/'（global-hotkey 只认后者）
    window.dispatchEvent(keyEvent({ key: '?', code: 'Slash', ctrlKey: true, shiftKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['ctrl+shift+/']);
    wrapper.unmount();
  });

  it('Shift+句点归一为 base 符号（> → period 的 .）', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: '>', code: 'Period', ctrlKey: true, shiftKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['ctrl+shift+.']);
    wrapper.unmount();
  });

  it('输入法组合态按键不进入录制（评审 I2）', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: 'j', code: 'KeyJ', ctrlKey: true, isComposing: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    expect(document.querySelector('.cap-mask')).toBeTruthy();
    wrapper.unmount();
  });

  it('仅 Shift 修饰不满足绑定要求（必须 Alt/Ctrl/Super）', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: '5', code: 'Digit5', shiftKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    expect(document.querySelector('.cap-mask')).toBeTruthy();
    wrapper.unmount();
  });

  it('纯字母（无修饰键）不录制，保持等待输入', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: 'j', code: 'KeyJ' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    expect(document.querySelector('.cap-mask')).toBeTruthy();
    wrapper.unmount();
  });

  it('只按修饰键不录制', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: 'Control', ctrlKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    expect(document.querySelector('.cap-mask')).toBeTruthy();
    wrapper.unmount();
  });

  it('系统保留组合 Alt+F4 不允许绑定（Windows）', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: 'F4', code: 'F4', altKey: true }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    wrapper.unmount();
  });

  it('Esc 取消录制，不产生任何值', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('button.sm').trigger('click');
    window.dispatchEvent(keyEvent({ key: 'Escape' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    expect(document.querySelector('.cap-mask')).toBeNull();
    wrapper.unmount();
  });

  // 评审 2026-09-10 I9：遮罩若在 mousedown 瞬间移除（无过渡、无 up 保护），
  // 松开会把 click 落到底层 key-slot（重新弹开）或清除键（误清快捷键）。
  // 遮罩必须存活到完整 click 才关闭
  it('遮罩用 click 而非 mousedown 关闭：按下瞬间浮层不消失', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: '' }, attachTo: document.body });
    await wrapper.find('.key-slot').trigger('click');
    const mask = document.querySelector('.cap-mask')!;
    expect(mask).toBeTruthy();

    mask.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    await wrapper.vm.$nextTick();
    expect(document.querySelector('.cap-mask'), 'mousedown 瞬间浮层不得消失').toBeTruthy();

    mask.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await wrapper.vm.$nextTick();
    expect(document.querySelector('.cap-mask')).toBeNull();
    wrapper.unmount();
  });

  it('橡皮擦清除：aria/title=清除快捷键，点击发出空值并回到空态文案', async () => {
    const wrapper = mount(HotkeyInput, { props: { modelValue: 'ctrl+k' } });
    const eraser = wrapper.find('button[aria-label="清除快捷键"]');
    expect(eraser.exists()).toBe(true);
    expect(eraser.attributes('title')).toBe('清除快捷键');
    expect(eraser.find('svg').classes()).toContain('lucide-eraser');
    expect(eraser.text(), '图标按钮不再用文字 ×').not.toContain('×');
    await eraser.trigger('click');
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['']);
    await wrapper.setProps({ modelValue: '' });
    expect(wrapper.find('.key-empty').text()).toBe('未设置 · 点击录制');
  });
});
