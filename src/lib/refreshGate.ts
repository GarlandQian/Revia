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

export interface ActivationContext {
  activation: number;
  worktreeId: string | null;
}

export interface OperationContext extends ActivationContext {
  operation: number;
}

/** 激活是归属，事件序号才是进度；首个事件和同激活内的连续保存都必须消费。 */
export class WatchEventGate {
  private seenSequence = 0;

  consume(activation: number, event: { changedActivation: number | null; eventSequence: number }): boolean {
    if (event.changedActivation !== activation || event.eventSequence <= this.seenSequence) return false;
    this.seenSequence = event.eventSequence;
    return true;
  }
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

/**
 * 写结果消费路径（Q90）：把「发起时的激活上下文」与「当前已接受快照」绑定在一起，
 * 供集成测试与 App 共用，避免只测 SnapshotGate 本身而没证明调用方真的用了它。
 *
 * 规则：
 * - 结果快照必须仍然属于发起时的激活目标；
 * - 结果版本不得低于该目标已接受的版本（晚到的 r2 不能覆盖 r3）；
 * - 失败/待核对结果保留在原 worktree；只有读取与当前快照更新受激活约束。
 */
export class OperationResultGate {
  private readonly gate: SnapshotGate;
  private operations = new Map<string, number>();

  constructor(gate: SnapshotGate) {
    this.gate = gate;
  }

  begin(worktreeId: string): OperationContext {
    const { activation } = this.gate.current();
    const operation = (this.operations.get(worktreeId) ?? 0) + 1;
    this.operations.set(worktreeId, operation);
    return { activation, worktreeId, operation };
  }

  canRecord(context: OperationContext): boolean {
    return context.worktreeId !== null && this.operations.get(context.worktreeId) === context.operation;
  }

  /** 成功结果：返回是否应当应用到界面。 */
  acceptSuccess(
    token: OperationContext,
    result: SnapshotEnvelope,
  ): boolean {
    return this.canRecord(token) && this.gate.accept(token, result);
  }

  /** 失败/待核对结果：只在发起目标仍激活时生效。 */
  acceptFailure(token: OperationContext): boolean {
    return this.canRecord(token) && this.gate.token().activation === token.activation
      && this.gate.token().worktreeId === token.worktreeId;
  }
}

/**
 * 选择上下文（Q88）：异步选择与元数据读取在应用结果前必须核对它。
 *
 * - `epoch`：每次选中/切换自增，同 worktree 内 U→V 也会作废 U 的迟到结果；
 * - `activation`：A→B→A 时第一次 A 的结果必须作废；
 * - `selection`：应用前再确认当前选中仍是这次请求的文件。
 */
export interface SelectionContext {
  epoch: number;
  activation: number;
  worktreeId: string | null;
  path: string;
  changes: string;
}

export class SelectionGate {
  private epoch = 0;
  private current: SelectionContext | null = null;

  begin(worktreeId: string | null, path: string, changes: string, activation: number): SelectionContext {
    this.epoch += 1;
    this.current = { epoch: this.epoch, activation, worktreeId, path, changes };
    return this.current;
  }

  /** 记录「已切走」（切换 worktree 或清空选择）。 */
  clear(): void {
    this.epoch += 1;
    this.current = null;
  }

  currentContext(): SelectionContext | null {
    return this.current;
  }

  /** 该请求的结果现在是否仍然有效。 */
  isValid(context: SelectionContext, activation: number): boolean {
    const current = this.current;
    if (!current) return false;
    if (current.epoch !== context.epoch) return false;
    if (current.activation !== context.activation) return false;
    if (activation !== context.activation) return false;
    return current.path === context.path && current.changes === context.changes;
  }
}

/**
 * 审阅变更叠加层（R07-D1）：较早的读取不能回退较新的本地审阅结果。
 *
 * 场景：`list_changes` 已在后端完成（拿到 revision N），用户随后点击勾选并**成功**；
 * 之后这个 revision N 的快照才被交付。若直接采用快照里的 `reviewed`，
 * 界面会回退到标记前，而持久记录仍是对的（真实桌面已观察到）。
 *
 * 规则：
 * - 每次本地成功的标记/取消都记一条 override，带**当时的文件版本**与单调序号；
 * - 读取请求发起时取 `baseline()`，只有序号**大于** baseline 的变更才会叠加；
 * - 叠加前要求版本仍一致：HEAD 或工作区版本真的变了，override 不生效，
 *   快照（以及后端已失效的标记）说了算——不会把已失效的标记勾回来。
 */
export interface ReviewVersionsLike {
  head: string | null;
  worktree: string | null;
}

export interface ReviewOverride {
  path: string;
  versions: ReviewVersionsLike;
  reviewed: boolean;
  seq: number;
}

export interface ReviewMarkEntryLike {
  path: string;
  reviewed: boolean;
  headVersion?: string | null;
  worktreeVersion?: string | null;
}

export interface ReviewStateLike {
  paths: string[];
  reviewed: string[];
  total: number;
  reviewedCount: number;
}

export interface ReviewSnapshotLike<E extends ReviewMarkEntryLike> {
  worktreeId: string;
  entries: E[];
  review: ReviewStateLike;
}

export class ReviewOverlay {
  private seq = 0;
  private readonly overrides = new Map<string, ReviewOverride>();
  /** 上限：只保留最近若干条，避免长时间使用后无界增长。 */
  private readonly limit: number;

  constructor(limit = 1024) {
    this.limit = limit;
  }

  private static key(worktreeId: string, path: string): string {
    return `${worktreeId}\u0000${path}`;
  }

  /** 记录一次**已成功**的本地审阅变更，返回它的序号。 */
  record(worktreeId: string, path: string, versions: ReviewVersionsLike, reviewed: boolean): number {
    this.seq += 1;
    this.overrides.set(ReviewOverlay.key(worktreeId, path), {
      path,
      versions: { head: versions.head, worktree: versions.worktree },
      reviewed,
      seq: this.seq,
    });
    if (this.overrides.size > this.limit) {
      const oldest = [...this.overrides.entries()].sort((a, b) => a[1].seq - b[1].seq)[0];
      if (oldest) this.overrides.delete(oldest[0]);
    }
    return this.seq;
  }

  /** 读取请求**发起时**调用；返回当前序号，作为「这次读取之后发生的变更」的分界。 */
  baseline(): number {
    return this.seq;
  }

  /** 该路径当前记录的本地状态（诊断与测试用）。 */
  get(worktreeId: string, path: string): ReviewOverride | undefined {
    return this.overrides.get(ReviewOverlay.key(worktreeId, path));
  }

  /**
   * 把 `baseline` 之后、且版本仍匹配的本地变更叠加到快照上。
   *
   * 只改 `reviewed` 与派生进度，不动其他字段；不修改传入对象。
   */
  apply<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(snapshot: S, baseline: number): S {
    let changed = false;
    const entries = snapshot.entries.map((entry) => {
      const override = this.overrides.get(ReviewOverlay.key(snapshot.worktreeId, entry.path));
      if (!override || override.seq <= baseline) return entry;
      const head = entry.headVersion ?? null;
      const worktree = entry.worktreeVersion ?? null;
      if (override.versions.head !== head || override.versions.worktree !== worktree) {
        // 版本真的变了：本地记录不再适用，标记按后端结果失效。
        return entry;
      }
      if (entry.reviewed === override.reviewed) return entry;
      changed = true;
      return { ...entry, reviewed: override.reviewed };
    });
    if (!changed) return snapshot;

    const reviewedSet = new Set(entries.filter((entry) => entry.reviewed).map((entry) => entry.path));
    const reviewed = snapshot.review.paths.filter((path) => reviewedSet.has(path));
    // 展开保留快照的全部字段（`apply` 只覆盖 `entries` 与派生进度）。
    return {
      ...snapshot,
      entries,
      review: { ...snapshot.review, reviewed, reviewedCount: reviewed.length },
    } as S;
  }
}
