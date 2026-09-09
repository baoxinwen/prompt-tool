import { VAR_RE } from './vars';

/** 预览片段：chip 标记变量原子片段（不截断变量本身），cut 标记后面还有内容被截断 */
export interface PreviewSeg { t: string; chip?: boolean; cut?: boolean }

/** 把提示词文本切成「文本 + 变量 chip」片段序列，总预算 max 字符；变量作为原子片段，
 *  放不下时整 chip 丢弃并以 … 收尾，避免 chip 被拦腰截断造成语义碎片 */
export function previewSegments(text: string, max: number): PreviewSeg[] {
  // 预览只讲内容概要，连续空白（含换行/制表）折叠为单空格
  const one = text.replace(/\s+/g, ' ').trim();
  const out: PreviewSeg[] = [];
  let used = 0;
  let last = 0;
  const cut = () => { out.push({ t: '…', cut: true }); return out; };
  VAR_RE.lastIndex = 0;
  for (let m = VAR_RE.exec(one); m; m = VAR_RE.exec(one)) {
    if (m.index > last) {
      const plain = one.slice(last, m.index);
      if (used + plain.length <= max) { out.push({ t: plain }); used += plain.length; }
      else {
        const room = max - used;
        if (room > 2) out.push({ t: plain.slice(0, room) });
        return cut();
      }
    }
    const name = m[1].trim();
    // 用 >= 而非 >：chip 必须预留至少 1 字符余量——chip 恰好顶满预算时后面必然还有内容要截断，
    // 「chip 紧贴 …」观感差，不如整 chip 丢弃（见「截断落在变量前：整 chip 丢弃」用例）
    if (used + name.length >= max) return cut();
    out.push({ t: name, chip: true });
    used += name.length;
    last = m.index + m[0].length;
  }
  const tail = one.slice(last);
  if (!tail) return out;
  if (used + tail.length <= max) out.push({ t: tail });
  else {
    const room = max - used;
    if (room > 2) out.push({ t: tail.slice(0, room) });
    out.push({ t: '…', cut: true });
  }
  return out;
}
