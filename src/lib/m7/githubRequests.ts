/** 同一目标、同类请求只有最新响应可以展示；切目标后成功、错误和收尾均失效。 */
export interface GithubRequestTicket {
  current: () => boolean;
  apply: (action: () => void) => void;
}
export class GithubRequests {
  private identity = "";
  private version = 0;
  private channels = new Map<string, number>();
  select(identity: string) {
    if (identity !== this.identity) { this.identity = identity; this.invalidate(); }
  }
  invalidate() { this.version++; this.channels.clear(); }
  begin(channel: string): GithubRequestTicket {
    const version = this.version;
    const sequence = (this.channels.get(channel) ?? 0) + 1;
    this.channels.set(channel, sequence);
    const current = () => version === this.version && sequence === this.channels.get(channel);
    return { current, apply: (action) => { if (current()) action(); } };
  }
}
/** 异步显示路径共用门禁，包括拒绝和finally，避免旧结果解除新目标的busy。 */
export async function githubResponse<T>(ticket: GithubRequestTicket, request: () => Promise<T>,
  accepted: (value: T) => void, failed: (error: unknown) => void, finished: () => void = () => {}) {
  try { const value = await request(); ticket.apply(() => accepted(value)); }
  catch (error) { ticket.apply(() => failed(error)); }
  finally { ticket.apply(finished); }
}
