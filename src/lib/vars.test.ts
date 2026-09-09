import { describe, it, expect } from 'vitest';
import { extractVars, hasVars, hasManualVars, isAutoVar, applyVars, applyClipboardVar } from './vars';

describe('extractVars：变量提取', () => {
  it('按出现顺序提取，重复变量去重且首个 hint 获胜', () => {
    const vars = extractVars('你好 {{name|姓名}}，来自 {{city|城市}}，再见 {{name|别名}}');
    expect(vars).toEqual([
      { name: 'name', hint: '姓名' },
      { name: 'city', hint: '城市' },
    ]);
  });

  it('变量名与提示两侧空白被修剪', () => {
    expect(extractVars('{{ spaced_name | 带提示 }}')).toEqual([
      { name: 'spaced_name', hint: '带提示' },
    ]);
  });

  it('空变量、无变量返回空数组', () => {
    expect(extractVars('{{}}')).toEqual([]);
    expect(extractVars('没有任何变量的普通文本')).toEqual([]);
    expect(extractVars('')).toEqual([]);
  });

  it('未闭合的花括号不产生变量', () => {
    expect(extractVars('{{未闭合')).toEqual([]);
    expect(extractVars('闭合}}')).toEqual([]);
  });

  it('嵌套花括号只提取内层合法变量', () => {
    expect(extractVars('{{a{{b}}}}')).toEqual([{ name: 'b', hint: '' }]);
  });

  it('提示中可以包含竖线', () => {
    expect(extractVars('{{a|左或右}}')).toEqual([{ name: 'a', hint: '左或右' }]);
  });

  it('纯提示无变量名（如 {{|提示}}）不产生条目', () => {
    expect(extractVars('{{|提示}}')).toEqual([]);
  });
});

describe('hasVars / hasManualVars / isAutoVar', () => {
  it('hasVars 对任何变量（含自动变量）返回 true', () => {
    expect(hasVars('{{clipboard}}')).toBe(true);
    expect(hasVars('{{name}}')).toBe(true);
    expect(hasVars('纯文本')).toBe(false);
  });

  it('isAutoVar 忽略大小写与空白', () => {
    expect(isAutoVar('clipboard')).toBe(true);
    expect(isAutoVar('  Clipboard ')).toBe(true);
    expect(isAutoVar('clip')).toBe(false);
    expect(isAutoVar('')).toBe(false);
  });

  it('hasManualVars：仅自动变量时为 false，含手动变量时为 true', () => {
    expect(hasManualVars('粘贴：{{clipboard}}')).toBe(false);
    expect(hasManualVars('{{name}} 请看剪贴板 {{clipboard}}')).toBe(true);
    expect(hasManualVars('没有变量')).toBe(false);
  });
});

describe('applyVars：变量替换', () => {
  it('替换已填写的变量，未填写的替换为空串', () => {
    expect(applyVars('你好 {{name}}，来自 {{city}}', { name: '小明' })).toBe('你好 小明，来自 ');
  });

  it('同一变量多处出现全部替换', () => {
    expect(applyVars('{{x}}+{{x}}={{sum}}', { x: '1', sum: '2' })).toBe('1+1=2');
  });

  it('带提示语法的变量整体替换（含竖线提示）', () => {
    expect(applyVars('数量：{{count|请填写数量}}', { count: '42' })).toBe('数量：42');
    expect(applyVars('数量：{{count|请填写数量}}', {})).toBe('数量：');
  });

  it('自动变量 {{clipboard}} 未提供值时保留占位符，交给粘贴链路填充', () => {
    expect(applyVars('剪贴板：{{clipboard}}', {})).toBe('剪贴板：{{clipboard}}');
  });

  it('自动变量带提示写法未提供值时同样整体保留', () => {
    expect(applyVars('参考：{{clipboard|将用剪贴板内容替换}}', {})).toBe(
      '参考：{{clipboard|将用剪贴板内容替换}}',
    );
  });

  it('自动变量提供值时同样替换', () => {
    expect(applyVars('剪贴板：{{clipboard}}', { clipboard: '已复制内容' })).toBe(
      '剪贴板：已复制内容',
    );
  });

  it('空内容与空值表返回空串', () => {
    expect(applyVars('', {})).toBe('');
  });

  it('无变量的文本原样返回', () => {
    expect(applyVars('普通文本', { a: 'b' })).toBe('普通文本');
  });

  it('变量名命中原型链键名（__proto__）且值表无自有键时，替换为空串而非原型对象（评审 C3）', () => {
    // 复现 VarDialog 的实际路径：reactive 普通对象上 values['__proto__'] = 'x'
    // 的赋值被原型访问器吞掉，自有键永远建不出来
    const values: Record<string, string> = {};
    values['__proto__'] = 'HELLO';
    expect(applyVars('A={{__proto__}}', values)).toBe('A=');
  });

  it('变量名 __proto__ 存在自有键时正常替换', () => {
    const values: Record<string, string> = JSON.parse('{"__proto__":"HELLO"}');
    expect(applyVars('A={{__proto__}}', values)).toBe('A=HELLO');
  });
});

describe('applyClipboardVar：{{clipboard}} 填充（评审 C2）', () => {
  it('剪贴板文本含 $& / $` / $\' / $$ 等替换序列时按字面填充，不被展开', () => {
    expect(applyClipboardVar('run: {{clipboard}}', "awk '{print $&}'")).toBe("run: awk '{print $&}'");
    expect(applyClipboardVar('{{clipboard}}', "a $$ b $& c $' d $` e")).toBe("a $$ b $& c $' d $` e");
    expect(applyClipboardVar('x {{clipboard|提示}} y', '$`')).toBe('x $` y');
  });

  it('剪贴板为空（null）时占位符替换为空串', () => {
    expect(applyClipboardVar('a{{clipboard}}b', null)).toBe('ab');
  });

  it('无占位符的文本原样返回', () => {
    expect(applyClipboardVar('普通文本', '剪贴内容')).toBe('普通文本');
  });
});
