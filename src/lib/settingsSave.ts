/** 设置保存串行化：设置是「读整个 Settings 快照 → 全量写回」的保存模型，
 *  SettingsPane 与 UpdateSection 都会发起保存。两次保存交叠时，后发者
 *  读到的还是上一次 refresh 完成前的旧快照，会把先发者已持久化的变更
 *  静默回滚且双双提示成功（评审 I3）。把读-改-写-刷新整段串行化：
 *  后一次保存在前一次的 refresh 完成后才读快照。 */
let pending: Promise<unknown> = Promise.resolve();

export function enqueueSettingsSave<T>(task: () => Promise<T>): Promise<T> {
  // 前一次失败也继续执行：单次保存失败不能卡死后续所有设置变更
  const run = pending.then(task, task);
  pending = run.catch(() => undefined);
  return run;
}
