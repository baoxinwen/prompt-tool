import { describe, it, expect } from 'vitest';
import { enqueueSettingsSave } from './settingsSave';

describe('enqueueSettingsSave：设置保存串行化（评审 I3）', () => {
  it('前一次保存完成前发起的下一次变更，不得基于旧快照把已保存的变更回滚', async () => {
    // 模拟后端 + Manager.refresh：latest 只在 save 完成后才对下一次读取可见
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    let latest: Record<string, number> = { a: 1, b: 2 };
    const saved: Array<Record<string, number>> = [];
    const save = async (s: Record<string, number>) => {
      await sleep(20); // saveSettings IPC 往返
      latest = { ...s };
      saved.push({ ...s });
    };
    const change = (mutate: (s: Record<string, number>) => void) =>
      enqueueSettingsSave(async () => {
        const next = { ...latest }; // 读取当前快照
        mutate(next);
        await save(next);
      });

    const p1 = change((s) => (s.a = 10));
    const p2 = change((s) => (s.b = 20)); // 在 p1 的保存+刷新完成前发起
    await Promise.all([p1, p2]);

    // 无串行化时 p2 读到 {a:1,b:2}，写回后把 a=10 静默回滚
    expect(saved[0]).toEqual({ a: 10, b: 2 });
    expect(saved[1]).toEqual({ a: 10, b: 20 });
    expect(latest).toEqual({ a: 10, b: 20 });
  });

  it('前一次保存失败不阻塞后续保存', async () => {
    const results: string[] = [];
    await enqueueSettingsSave(async () => {
      throw new Error('磁盘已满');
    }).catch(() => results.push('err'));
    await enqueueSettingsSave(async () => {
      results.push('ok');
    });
    expect(results).toEqual(['err', 'ok']);
  });
});
