import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/** capabilities 契约：无框窗口的自定义标题栏依赖这批窗口权限。
 *  缺任一条，对应按钮/拖拽在真实 Tauri 里会被权限系统静默拒绝
 *  （浏览器 e2e 因 shim mock 无法暴露，故用本契约测试守住）。 */
const REQUIRED = [
  'core:window:allow-minimize',
  'core:window:allow-toggle-maximize',
  'core:window:allow-unmaximize',
  'core:window:allow-start-dragging',
  'core:window:allow-close',
] as const;

describe('tauri capabilities', () => {
  it('包含自定义标题栏所需的全部窗口权限', () => {
    const path = fileURLToPath(new URL('../../src-tauri/capabilities/default.json', import.meta.url));
    const cap = JSON.parse(readFileSync(path, 'utf8'));
    expect(cap.windows).toContain('manager');
    for (const perm of REQUIRED) {
      expect(cap.permissions, `缺少权限 ${perm}`).toContain(perm);
    }
  });
});
