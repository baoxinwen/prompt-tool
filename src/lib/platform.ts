/** Windows 平台常量 */

/** 修饰键展示名（快捷键内部存储值保持 ctrl/alt/super/shift 不变） */
export const MOD_LABELS: Record<string, string> = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', super: 'Win', meta: 'Win' };

/** 快捷键组合提示文案 */
export const hotkeyHint = '需包含 Alt / Ctrl / Win 修饰键';

/** 单独的键位展示名 */
export function keyLabel(k: string): string {
  const key = k.trim().toLowerCase();
  // 只认自有属性：键名是外部数据（HotkeyInput 存储），沿原型链解析
  // constructor/toString 会把函数对象显示出来（评审 2026-09-10 M5#8）
  if (Object.prototype.hasOwnProperty.call(MOD_LABELS, key)) return MOD_LABELS[key];
  // 方向键按 Windows 惯例整体首词大写：arrowleft → ArrowLeft（评审 M4#12）
  if (key.startsWith('arrow')) {
    return 'Arrow' + key.slice(5).toUpperCase();
  }
  return key.length === 1 ? key.toUpperCase() : key.charAt(0).toUpperCase() + key.slice(1);
}
