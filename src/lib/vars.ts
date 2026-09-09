import type { VarField } from '../types';

/** 变量占位符正则：{{名字}} 或 {{名字|提示}}（g 标志，exec 循环用后须重置 lastIndex） */
export const VAR_RE = /\{\{\s*([^{}|]+?)\s*(?:\|([^{}]*))?\}\}/g;

/** 提取提示词中的 {{变量}}，按出现顺序去重 */
export function extractVars(content: string): VarField[] {
  const seen = new Map<string, VarField>();
  let m: RegExpExecArray | null;
  VAR_RE.lastIndex = 0;
  while ((m = VAR_RE.exec(content)) !== null) {
    const name = m[1].trim();
    if (!name || seen.has(name)) continue;
    seen.set(name, { name, hint: (m[2] ?? '').trim() });
  }
  return [...seen.values()];
}

export function hasVars(content: string): boolean {
  return extractVars(content).length > 0;
}

/** 内置自动变量：调用时自动填充，不出现在填写表单 */
export const AUTO_VARS = new Set(['clipboard']);

export function isAutoVar(name: string): boolean {
  return AUTO_VARS.has(name.trim().toLowerCase());
}

/** 是否存在需要手动填写的变量（排除 {{clipboard}} 等自动变量） */
export function hasManualVars(content: string): boolean {
  return extractVars(content).some((v) => !isAutoVar(v.name));
}

/** 用填写的值替换 {{变量}}；未填写的变量替换为空串；
 *  自动变量（如 {{clipboard}}）在调用方未显式提供值时保留占位符，
 *  由粘贴/复制链路的 fillClipboardVar 统一填充，避免变量窗路径把它清成空串 */
export function applyVars(content: string, values: Record<string, string>): string {
  return content.replace(VAR_RE, (match, rawName: string) => {
    const name = String(rawName).trim();
    // 只认自有键：变量名是用户可控的（如 __proto__），
    // `in`/直接取值会沿原型链读到 Object.prototype 被字符串化成 "[object Object]"
    if (!Object.prototype.hasOwnProperty.call(values, name)) {
      return isAutoVar(name) ? match : '';
    }
    return values[name] ?? '';
  });
}

/** {{clipboard}} 自动变量的占位符正则：允许带提示写法 {{clipboard|提示}} */
export function clipboardVarRe(): RegExp {
  return /\{\{\s*clipboard(?:\s*\|[^{}]*)?\s*\}\}/gi;
}

/** 用剪贴板文本填充 {{clipboard}} 占位符。
 *  必须用函数作替换参数：字符串替换里 `$&`/`$'`/`$`/`$$` 有特殊语义，
 *  剪贴板里的 shell/awk 片段会被静默改写（评审 C2，与 applyVars 同陷阱） */
export function applyClipboardVar(text: string, clip: string | null): string {
  return text.replace(clipboardVarRe(), () => clip ?? '');
}
