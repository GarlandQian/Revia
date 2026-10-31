/** 三方合并未应用内容仍由原编辑器持有；生命周期必须等用户先应用或取消。 */
export class MergeBufferGuards {
  private dirty = new Set<string>();
  private listeners = new Set<() => void>();
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => this.listeners.delete(listener); };
  get hasWork() { return this.dirty.size > 0; }
  mark(key: string) { if (!this.dirty.has(key)) { this.dirty.add(key); this.changed(); } }
  clear(key: string) { if (this.dirty.delete(key)) this.changed(); }
  private changed() { for (const listener of this.listeners) listener(); }
}
export const mergeBufferGuards = new MergeBufferGuards();
