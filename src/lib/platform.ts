/** Windows 平台常量 */

/** 修饰键展示名（快捷键内部存储值保持 ctrl/alt/super/shift 不变） */
export const MOD_LABELS: Record<string, string> = { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', super: 'Win', meta: 'Win' };

/** 快捷键组合提示文案 */
export const hotkeyHint = '需包含 Alt / Ctrl / Win 修饰键';

/** 单独的键位展示名 */
export function keyLabel(k: string): string {
  const key = k.trim().toLowerCase();
  if (MOD_LABELS[key]) return MOD_LABELS[key];
  return key.length === 1 ? key.toUpperCase() : key.charAt(0).toUpperCase() + key.slice(1);
}
