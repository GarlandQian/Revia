/**
 * 快照接收门（Q88、Q89、Q90、T05）。
 *
 * 所有进入界面的快照都经过这里，保证：
 * - 只接受**当前激活** worktree 的快照；切换后旧 worktree 的响应一律丢弃（含 A→B→A）；
 * - 同一 worktree 内只接受版本更大的快照，晚到的旧响应不能覆盖新内容；
 * - 空列表同样带版本，因此「清空 → 又有改动」的顺序也可判定。
 *
 * 这里只做判定，不发起请求，也不缓存快照内容。
 */
export interface SnapshotEnvelope {
  worktreeId: string;
  revision: number;
}

export class SnapshotGate {
  /** 每次切换 worktree 递增；用于识别 A→B→A 里第一次 A 的迟到响应。 */
  private activation = 0;
  private activeWorktree: string | null = null;
  /** 每个 worktree 已接受的最大版本。 */
  private accepted = new Map<string, number>();

  /** 切换激活目标；返回新的 activation。 */
  activate(worktreeId: string | null): number {
    this.activation += 1;
    this.activeWorktree = worktreeId;
    return this.activation;
  }

  current(): { activation: number; worktreeId: string | null } {
    return { activation: this.activation, worktreeId: this.activeWorktree };
  }

  /** 请求发起时记录上下文；响应回来时用它判定是否仍然有效。 */
  token(): { activation: number; worktreeId: string | null } {
    return this.current();
  }

  /**
   * 是否接受该响应。接受后记录版本；同一版本重复到达视为已接受（幂等）。
   */
  accept(
    token: { activation: number; worktreeId: string | null },
    envelope: SnapshotEnvelope,
  ): boolean {
    if (token.activation !== this.activation) return false;
    if (token.worktreeId === null || token.worktreeId !== this.activeWorktree) return false;
    if (envelope.worktreeId !== this.activeWorktree) return false;
    const previous = this.accepted.get(envelope.worktreeId) ?? 0;
    if (envelope.revision < previous) return false;
    this.accepted.set(envelope.worktreeId, envelope.revision);
    return true;
  }

  /** 已接受的最大版本（诊断与测试用）。 */
  acceptedRevision(worktreeId: string): number {
    return this.accepted.get(worktreeId) ?? 0;
  }
}

/**
 * 刷新协调器：合并事件、单飞执行、执行期间的变化只补一次（Q88）。
 *
 * - `request()` 不排队：执行中再次请求只置脏，执行结束再补一轮；
 * - 事件风暴不会产生无限队列；
 * - 每轮结束回调一次结果，由调用方决定是否接收。
 */
export class RefreshCoordinator {
  private running = false;
  private dirty = false;
  private disposed = false;

  private readonly run: () => Promise<void>;

  constructor(run: () => Promise<void>) {
    this.run = run;
  }

  /** 请求一次刷新；正在执行时只标记脏，不排队。 */
  request(): void {
    if (this.disposed) return;
    if (this.running) {
      this.dirty = true;
      return;
    }
    void this.drain();
  }

  private async drain(): Promise<void> {
    this.running = true;
    try {
      do {
        this.dirty = false;
        try {
          await this.run();
        } catch {
          // 单轮失败不终止协调器；错误由 run 自身汇报。
        }
      } while (this.dirty && !this.disposed);
    } finally {
      this.running = false;
    }
  }

  /** 是否有请求正在进行（诊断用）。 */
  isRunning(): boolean {
    return this.running;
  }

  dispose(): void {
    this.disposed = true;
    this.dirty = false;
  }
}
