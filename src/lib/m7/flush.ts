/**
 * 本机草稿的关闭前冲刷登记（M7 任务 03）。
 *
 * 编辑面板与终端把「关闭前必须落盘 / 保存页签」的动作登记到这里，
 * App 的原生关闭流程在销毁窗口前调用 `flushM7State()`。
 */

type Flusher = () => Promise<void>;

const flushers = new Set<Flusher>();

export function registerM7Flusher(flusher: Flusher): () => void {
  flushers.add(flusher);
  return () => flushers.delete(flusher);
}

/** 依次执行所有登记项；失败向上抛出，调用方据此阻止关闭。 */
export async function flushM7State(): Promise<void> {
  for (const flusher of flushers) await flusher();
}
