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

/**
 * 一条本地审阅记录。
 *
 * - `seq`：后端返回的**真实提交序号**。所有改变审阅真值的事件（新增 / 取消 / 失效清理）
 *   都在后端按真实执行先后推进它，所以序号大小就是磁盘上的持久化先后。
 * - `commitSeq`：写入叠加层的次序，只用于「某次读取能不能观察到它」。
 * - `versions`：**后端返回的、这条状态实际绑定的**版本。
 * - `reviewed`：`false` 表示这次写入**确认取消**（ST07-R10-1）。
 */
export interface ReviewOverride {
  path: string;
  versions: ReviewVersionsLike;
  reviewed: boolean;
  /** 后端真实提交序号。 */
  seq: number;
  commitSeq: number;
}

export interface ReviewMarkEntryLike {
  path: string;
  reviewed: boolean;
  headVersion?: string | null;
  worktreeVersion?: string | null;
  /** 版本是否可确认；未确认的条目不能新增认领，也不参与批量记账。 */
  worktreeVersionConfirmed?: boolean;
}

export interface ReviewStateLike {
  paths: string[];
  reviewed: string[];
  total: number;
  reviewedCount: number;
  /** 读取时后端给出的真实提交序号：这条快照已经观察到哪一水位（ST07-R10-3）。 */
  seq?: number;
}

export interface ReviewSnapshotLike<E extends ReviewMarkEntryLike> {
  worktreeId: string;
  entries: E[];
  review: ReviewStateLike;
}

/** 后端返回的一条已确认状态（`ReviewMark`：path + reviewed + 版本）。 */
export interface ReviewMarkLike {
  path: string;
  reviewed: boolean;
  head: string | null;
  worktree: string | null;
}

/**
 * 一次读取交付的结果（ST07-R15-1）。
 *
 * 不可信的读取**不能**用「原样返回快照」冒充成功：调用方会把旧状态当成新真值
 * 接受并覆盖界面。这里用可辨识联合明确区分，`ok: false` 时必须重新核对。
 */
export type ReviewReadOutcome<S> =
  | { ok: true; snapshot: S }
  | {
      ok: false;
      reason: "untrusted";
      /** 被拒收的读取所属 worktree：重新核对要按**原目标**发起。 */
      worktreeId: string;
      /** 这次读取自报的审阅水位（低于淘汰水位，无法安全合成）。 */
      observedSeq: number;
    };

/** 一次逐路径读取合并的结果：不可信时明确拒收，调用方重新核对。 */
export type ReviewEntryOutcome<E> =
  | { ok: true; entry: E; observedSeq: number }
  | { ok: false; reason: "untrusted"; worktreeId: string; observedSeq: number };

export class ReviewOverlay {
  /** 写入叠加层的次序（读取分界用）。 */
  private commitSeq = 0;
  private readonly overrides = new Map<string, ReviewOverride>();
  /** 上限：只保留最近若干条，避免长时间使用后无界增长。 */
  private readonly limit: number;
  /**
   * 每个 worktree 被**淘汰**过的最高真实序号（R07-CAP-1）。
   *
   * 内存必须有界，所以叠加层会丢弃最早写入的记录。一旦某条记录被丢弃，
   * 「这条路径已确认到某个序号」这件事就**不再可知**：此时再来一份更旧的回执，
   * 我们既不能证明它更旧（记录没了），也不能证明它更新。把它当成新真值重新写入，
   * 就是让被淘汰的旧状态重新认领。
   *
   * 因此按 worktree 记住淘汰过的最高序号：任何 `seq <= 该水位` 的回执/读取都
   * 视为**信息不足**，拒绝发布并要求重新读取（不猜、不重新认领）。
   * 只在淘汰时升高，永不降低；按 worktree 分开，互不影响。
   */
  private readonly evictedHighSeq = new Map<string, number>();

  constructor(limit = 1024) {
    this.limit = limit;
  }

  /** 该 worktree 被淘汰过的最高序号；0 表示从未淘汰。 */
  evictedWatermark(worktreeId: string): number {
    return this.evictedHighSeq.get(worktreeId) ?? 0;
  }

  /**
   * 这个序号是否还**可信**：不大于淘汰水位说明可能有更高序号的记录已被丢弃，
   * 仅凭当前叠加层无法安全合成（R07-CAP-1）。
   */
  isTrustworthy(worktreeId: string, seq: number): boolean {
    const evicted = this.evictedWatermark(worktreeId);
    // 从未淘汰：现有记录就是全部真值，任何序号都能安全比较。
    if (evicted === 0) return true;
    // 淘汰水位记录的是「被丢弃记录的最大序号」。序号等于它说明这份回执本身
    // 就是那一批（或更新），它的真值仍然完整可用；只有**严格更旧**的回执
    // 才可能早于已被丢弃的更新真值，此时无法安全合成（R07-CAP-1）。
    return seq >= evicted;
  }

  private static key(worktreeId: string, path: string): string {
    return `${worktreeId}\u0000${path}`;
  }

  /**
   * 记录一条**已成功**的后端确认状态；返回是否真的写入了叠加层。
   *
   * 覆盖同路径已有记录**只看后端真实序号** `seq`（ST07-R9-1 / ST07-R10-3）：
   * 序号更大的是更晚的真实事件（新增 / 取消 / 失效），更早的一律不生效。
   * 取消同样是一条记录（`reviewed: false`），所以取消也能挡住旧的新增回执
   * （ST07-R10-1）。
   */
  record(
    worktreeId: string,
    path: string,
    versions: ReviewVersionsLike,
    reviewed: boolean,
    seq: number,
  ): boolean {
    const key = ReviewOverlay.key(worktreeId, path);
    const existing = this.overrides.get(key);
    if (existing && seq <= existing.seq) return false;
    // 路径已被淘汰、且这条回执**严格更旧**于淘汰水位：信息不足，
    // 不能当新真值重新认领（同序号的记录属于同一批回执，不算过时）。
    if (!existing && seq < this.evictedWatermark(worktreeId)) return false;
    this.commitSeq += 1;
    this.overrides.set(key, {
      path,
      versions: { head: versions.head, worktree: versions.worktree },
      reviewed,
      seq,
      commitSeq: this.commitSeq,
    });
    if (this.overrides.size > this.limit) {
      const oldest = [...this.overrides.entries()].sort((a, b) => a[1].commitSeq - b[1].commitSeq)[0];
      if (oldest) {
        const evicted = oldest[1];
        this.overrides.delete(oldest[0]);
        // 记住这条 worktree 淘汰过的最高序号：之后任何不比它新的回执都不可信。
        const evictedWorktree = oldest[0].slice(0, oldest[0].indexOf("\u0000"));
        const current = this.evictedWatermark(evictedWorktree);
        if (evicted.seq > current) this.evictedHighSeq.set(evictedWorktree, evicted.seq);
      }
    }
    return true;
  }

  /**
   * 读取/写请求**发起前**调用，拿到这次请求的审阅分界。
   *
   * 返回的句柄同时承担交付时的合并，所以「先取分界、后交付」由类型强制。
   */
  beginRead(): ReviewReadRequest {
    return new ReviewReadRequest(this, this.commitSeq);
  }

  /** 该路径当前记录的本地状态（诊断与测试用）。 */
  get(worktreeId: string, path: string): ReviewOverride | undefined {
    return this.overrides.get(ReviewOverlay.key(worktreeId, path));
  }

  /**
   * 把这次读取之后发生、且版本仍匹配、且**比读取水位更新**的本地状态叠加到快照上。
   *
   * 「读取水位」是快照自带的 `review.seq`：后端在那次读取时已经观察到的事件，
   * 读取本身就是更新的真值，不能被更早的本地记录覆盖（ST07-R10-3 / SP07-R10-3）。
   *
   * 只改 `reviewed` 与派生进度，不动其他字段；不修改传入对象。
   */
  apply<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(
    snapshot: S,
    boundary: number,
  ): ReviewReadOutcome<S> {
    const observedSeq = snapshot.review.seq ?? 0;
    // 这次读取的序号严格旧于淘汰水位：它可能早于已被丢弃的更新真值。
    // **明确拒收**（不是原样返回冒充成功），调用方必须重新核对真实状态
    // （ST07-R15-1 / SP07-R15-2）。
    if (!this.isTrustworthy(snapshot.worktreeId, observedSeq)) {
      return { ok: false, reason: "untrusted", worktreeId: snapshot.worktreeId, observedSeq };
    }
    let changed = false;
    const entries = snapshot.entries.map((entry) => {
      const override = this.overrides.get(ReviewOverlay.key(snapshot.worktreeId, entry.path));
      if (!override) return entry;
      if (override.commitSeq <= boundary) return entry;
      if (observedSeq >= override.seq) return entry;
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
    if (!changed) return { ok: true, snapshot };
    return { ok: true, snapshot: this.syncProgress({ ...snapshot, entries }) };
  }

  /**
   * 用同路径的当前条目与本地记录修正一条被**整体替换**的条目（未跟踪元数据补全用）。
   *
   * 两个候选都带**它自己已确认的审阅水位**（`loadedSeq` 来自后端的 `RowSlice.reviewSeq`，
   * `currentSeq` 来自界面快照的 `review.seq`）。合并规则：
   *
   * 1. 版本不一致 → 一律用 `loaded`（新读取适用于新版本）。
   * 2. 版本一致 → 取**水位更大**的那一侧作为底座：真值按真实执行先后判定，
   *    不看谁先到达界面（ST07-R12-2）。旧界面（水位小）不能覆盖后端新读取（水位大），
   *    反过来也一样。
   * 3. 再叠加**比底座更新**的本地记录（`seq` 严格更大、版本一致），
   *    让真正更晚的成功仍然生效。
   */
  mergeEntry<E extends ReviewMarkEntryLike>(
    worktreeId: string,
    loaded: E,
    loadedSeq: number,
    current: E | undefined,
    currentSeq: number,
  ): ReviewEntryOutcome<E> {
    // 逐路径读取也必须遵守同一可信度契约：这次读取的序号严格旧于淘汰水位时，
    // 可能早于已被丢弃的更新真值，不能拿它合成（ST07-R15-1）。
    if (!this.isTrustworthy(worktreeId, loadedSeq)) {
      return { ok: false, reason: "untrusted", worktreeId, observedSeq: loadedSeq };
    }
    const head = loaded.headVersion ?? null;
    const worktree = loaded.worktreeVersion ?? null;
    let reviewed = loaded.reviewed;
    let baseSeq = loadedSeq;
    if (current
      && (current.headVersion ?? null) === head
      && (current.worktreeVersion ?? null) === worktree) {
      // 版本一致：水位更大的一侧才是更新的真值。
      if (currentSeq > loadedSeq) {
        reviewed = current.reviewed;
        baseSeq = currentSeq;
      }
    }
    const override = this.overrides.get(ReviewOverlay.key(worktreeId, loaded.path));
    if (override && override.seq > baseSeq
      && override.versions.head === head && override.versions.worktree === worktree) {
      reviewed = override.reviewed;
      baseSeq = override.seq;
    }
    // **持有**这次已接受的真值（ST07-R13-1 / SP07-R13-1）。
    //
    // 只把结果写回界面布尔值是不够的：之后的 `composeState`/`apply` 需要知道
    // 「这条路径已经观察到哪一步」，否则一份更旧的全量回执会用它自己的底座
    // 把这次读取到的状态覆盖回去。记录带**该路径**的版本与真实序号，
    // 所以它只影响这条路径，不会把单文件水位当成整快照已确认。
    this.record(worktreeId, loaded.path, { head, worktree }, reviewed, baseSeq);
    return {
      ok: true,
      entry: reviewed === loaded.reviewed ? loaded : { ...loaded, reviewed },
      observedSeq: baseSeq,
    };
  }

  /**
   * 按条目重算进度，保证 `reviewed` / `reviewedCount` 与勾选一致。
   *
   * 只在 `review.paths`（后端给出的去重路径集合）内统计，不凭空扩大总数。
   */
  syncProgress<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(snapshot: S): S {
    const reviewedSet = new Set(snapshot.entries.filter((entry) => entry.reviewed).map((entry) => entry.path));
    const reviewed = snapshot.review.paths.filter((path) => reviewedSet.has(path));
    const same = reviewed.length === snapshot.review.reviewedCount
      && reviewed.length === snapshot.review.reviewed.length
      && reviewed.every((path, index) => path === snapshot.review.reviewed[index]);
    if (same) return snapshot;
    return { ...snapshot, review: { ...snapshot.review, reviewed, reviewedCount: reviewed.length } } as S;
  }

  /**
   * 把一批后端确认的状态（含取消）写进叠加层。
   *
   * `marks` 就是后端返回的 `ReviewMarks.marks`，每条自带 `reviewed` 与它绑定的版本；
   * 取消项也在里面（`reviewed: false`），所以取消不需要前端另行推断（ST07-R10-1）。
   */
  commitMarks(worktreeId: string, marks: ReadonlyArray<ReviewMarkLike>, seq: number): void {
    for (const mark of marks) {
      this.record(
        worktreeId,
        mark.path,
        { head: mark.head, worktree: mark.worktree },
        mark.reviewed,
        seq,
      );
    }
  }

  /**
   * 按**真实执行进度**合成一次成功回执要展示的状态。
   *
   * 底座不是「当前界面」，而是这次回执带来的**完整已确认真值**（`confirmed`）：
   * 后端对每个可确认路径都给出了这一刻的 true/false 与绑定版本，缺席路径的旧状态
   * 不能继续当真（ST07-R11-1 / SP07-R11-2）。只返回正标记会让 B 的取消丢失，
   * 于是界面继续显示 B 已审，而磁盘上它已经取消。
   *
   * 在底座上再叠加**序号严格更大**的本地记录（更晚的真实事件），
   * 这样「B 取消 seq1、A 标记 seq2 但 A 回执先到」时两者都能正确显示。
   *
   * 版本一律按条目自身版本匹配，不匹配就不叠加。
   */
  composeState<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(
    snapshot: S,
    confirmed: ReadonlyArray<ReviewMarkLike>,
    observedSeq: number,
  ): S {
    const confirmedByPath = new Map(confirmed.map((mark) => [mark.path, mark]));
    const entries = snapshot.entries.map((entry) => {
      const base = confirmedByPath.get(entry.path);
      const head = entry.headVersion ?? null;
      const worktree = entry.worktreeVersion ?? null;
      // 底座：回执确认的版本必须与当前条目一致，否则这条回执不适用于当前版本。
      let reviewed = entry.reviewed;
      if (base && base.head === head && base.worktree === worktree) reviewed = base.reviewed;
      // 叠加比这次回执更晚、且版本一致的本地记录。
      const override = this.overrides.get(ReviewOverlay.key(snapshot.worktreeId, entry.path));
      if (override && override.seq > observedSeq
        && override.versions.head === head && override.versions.worktree === worktree) {
        reviewed = override.reviewed;
      }
      return reviewed === entry.reviewed ? entry : { ...entry, reviewed };
    });
    return this.syncProgress({ ...snapshot, entries });
  }
}

/**
 * 一次读取/写请求的审阅分界句柄。
 *
 * 只由 `ReviewOverlay.beginRead()` 在**请求发起处**创建；`deliver` 在结果到达时合并，
 * 因此「分界取晚」在类型上就写不出来（ST07-R7-1 / SP07-R7-4）。
 */
export class ReviewReadRequest {
  private readonly overlay: ReviewOverlay;
  private readonly boundary: number;

  constructor(overlay: ReviewOverlay, boundary: number) {
    this.overlay = overlay;
    this.boundary = boundary;
  }

  /** 这次请求的分界序号（需要逐条合并时用，例如未跟踪元数据补全）。 */
  boundaryOf(): number {
    return this.boundary;
  }

  /** 交付结果：叠加这次请求发起之后、比读取水位更新、且版本仍匹配的本地状态。 */
  deliver<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(snapshot: S): ReviewReadOutcome<S> {
    return this.overlay.apply(snapshot, this.boundary);
  }
}

/**
 * 生产入口的统一读取方式：**先取分界，再发请求**，结果到达时合并交付。
 *
 * `src/App.tsx` 的 `readAndAccept`、`refreshTarget`、`openWorktree` 与写结果分支
 * 都必须经这里，否则「分界取晚」会重新出现（ST07-R7-1 / SP07-R7-1 / SP07-R7-4）。
 */
export async function readThroughReview<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(
  overlay: ReviewOverlay,
  read: () => Promise<S>,
): Promise<ReviewReadOutcome<S>> {
  const request = overlay.beginRead();
  const snapshot = await read();
  return request.deliver(snapshot);
}

/** 一次成功的标记结果要记哪些账。 */
export interface ReviewResultPlan {
  /** 要写入叠加层的条目：路径 + **后端实际绑定的版本** + 状态（含取消的 false）。 */
  records: Array<{ path: string; versions: ReviewVersionsLike; reviewed: boolean }>;
  /** 后端返回的完整已审阅集合（仅用于诊断；展示必须走 `composeState`）。 */
  reviewed: Set<string>;
  /** 后端返回的真实提交序号。 */
  seq: number;
}

/**
 * 计算一次**已成功**的标记结果要记录的全部变更。
 *
 * 三个来源都直接取自后端返回：
 * - **状态**：`marks[].reviewed`，取消也在其中（`false`），不是「只遍历正标记」
 *   （ST07-R10-1 / SP07-R10-1）。
 * - **版本**：每条标记用**它自己实际绑定**的版本（ST07-R9-3）。
 * - **先后**：后端真实提交序号 `seq`（ST07-R9-1）。
 */
export function planReviewResult(input: {
  /** 后端返回的完整已审阅集合。 */
  reviewed: Iterable<string>;
  /** 后端返回的已确认状态（`ReviewMark` 扁平结构：path + reviewed + head + worktree）。 */
  marks: ReadonlyArray<ReviewMarkLike>;
  /** 后端返回的真实提交序号。 */
  seq: number;
}): ReviewResultPlan {
  const reviewed = new Set(input.reviewed);
  const records = input.marks.map((mark) => ({
    path: mark.path,
    versions: { head: mark.head, worktree: mark.worktree },
    // 直接用后端给的状态：取消项是 false，不能被「已审阅集合里没有它」推断成「不用记」。
    reviewed: mark.reviewed,
  }));
  return { records, reviewed, seq: input.seq };
}

/**
 * 把计划写进叠加层，先后用后端真实序号。
 *
 * 必须在展示门禁（`isCurrent`）之前调用：后端已经持久化，迟到响应只是不该更新界面，
 * 不能让后续旧读取把这条真实变更回退。
 */
export function commitReviewResult(
  overlay: ReviewOverlay,
  worktreeId: string,
  plan: ReviewResultPlan,
): void {
  for (const record of plan.records) {
    overlay.record(worktreeId, record.path, record.versions, record.reviewed, plan.seq);
  }
}

/**
 * 一次**已成功**的标记回执的完整记账入口（生产调用点唯一）。
 *
 * 只接受后端返回值：状态与版本来自 `marks`，先后来自 `seq`。
 * 调用方无法用界面条目、点击次序或「已审阅集合里有没有」替代。
 */
export function applyReviewSuccess(input: {
  overlay: ReviewOverlay;
  worktreeId: string;
  reviewed: Iterable<string>;
  marks: ReadonlyArray<ReviewMarkLike>;
  seq: number;
}): ReviewResultPlan {
  const plan = planReviewResult({
    reviewed: input.reviewed,
    marks: input.marks,
    seq: input.seq,
  });
  commitReviewResult(input.overlay, input.worktreeId, plan);
  return plan;
}

/**
 * 一次成功回执的**完整生产入口**：记账 + 按真实执行进度合成展示/导航状态。
 *
 * `App` 只能通过它拿到要展示的审阅状态，不能直接用后端这次返回的 `reviewed`
 * （ST07-R10-2 / SP07-R10-2：更晚的其它成功可能已经记账）。
 * 返回 `null` 表示当前没有可展示的快照（调用方保持原状）。
 */
export function applyReviewSuccessAndCompose<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(input: {
  overlay: ReviewOverlay;
  worktreeId: string;
  reviewed: Iterable<string>;
  marks: ReadonlyArray<ReviewMarkLike>;
  seq: number;
  /** 界面当前快照；与回执不同 worktree 时返回 null。 */
  snapshot: S | null | undefined;
}): { plan: ReviewResultPlan; view: S | null } {
  const plan = applyReviewSuccess({
    overlay: input.overlay,
    worktreeId: input.worktreeId,
    reviewed: input.reviewed,
    marks: input.marks,
    seq: input.seq,
  });
  const snapshot = input.snapshot;
  const view = snapshot && snapshot.worktreeId === input.worktreeId
    ? input.overlay.composeState(snapshot, input.marks, input.seq)
    : null;
  return { plan, view };
}

/**
 * 一次成功回执的**完整发布决策**（生产唯一入口）。
 *
 * 把「记账 → 上下文判定 → 基于 current 发布/返回导航状态」这条链收在一个函数里，
 * 让 Node 回归能直接驱动真实生产路径，而不是只测各个 helper：
 *
 * - 记账与合成**先做**（后端已持久化，迟到不该丢账）；
 * - 发布只要求 `context` 仍有效（同 worktree / activation / revision），
 *   **不含点击代际**：更晚执行的真实成功不能被旧点击丢掉（ST07-R11-2）；
 * - 错误清理单独用 `clearError` 判定（仍按点击代际，较新的拒绝不能被清掉）；
 * - 发布结果由 `merge` 在**调用方当前的快照**上只合并审阅字段，
 *   不用捕获的整批 entries 覆盖已排队的元数据（SP07-R11-4）。
 */
export interface ReviewPublishInput<S extends ReviewSnapshotLike<ReviewMarkEntryLike>> {
  overlay: ReviewOverlay;
  worktreeId: string;
  reviewed: Iterable<string>;
  marks: ReadonlyArray<ReviewMarkLike>;
  seq: number;
  /** 合成时使用的快照（发起请求时看到的界面状态）。 */
  snapshot: S | null | undefined;
  /**
   * 发起请求时记下的上下文事实。判定由本函数统一做，调用方不能自己拼。
   *
   * `epochMatches` 是**点击代际**，只用于错误清理——不参与发布判定
   * （ST07-R11-2：更晚执行的真实成功不能被旧点击丢掉）。
   */
  context: {
    worktreeMatches: boolean;
    activationMatches: boolean;
    revisionMatches: boolean;
    epochMatches: boolean;
  };
}

export interface ReviewPublishResult<S extends ReviewSnapshotLike<ReviewMarkEntryLike>> {
  /** 合成后的状态；`null` 表示上下文已失效、不发布。 */
  view: S | null;
  /** 导航应使用的审阅进度（上下文失效时为后端这次返回的集合）。 */
  navigation: ReviewStateLike;
  /** 是否应该清理错误提示。 */
  clearError: boolean;
  /**
   * 叠加层信息不足（序号不大于淘汰水位），这次发布被拒绝。
   * 调用方必须重新读取真实状态，不能拿回执的旧值猜（R07-CAP-1）。
   */
  needsRefresh: boolean;
}

export function publishReviewSuccess<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(
  input: ReviewPublishInput<S>,
): ReviewPublishResult<S> {
  const { worktreeMatches, activationMatches, revisionMatches, epochMatches } = input.context;
  // 发布只看「这条真值是否还属于当前视图」：worktree / activation / revision。
  const contextValid = worktreeMatches && activationMatches && revisionMatches;
  const clearError = epochMatches && contextValid;
  const { plan, view } = applyReviewSuccessAndCompose({
    overlay: input.overlay,
    worktreeId: input.worktreeId,
    reviewed: input.reviewed,
    marks: input.marks,
    seq: input.seq,
    snapshot: input.snapshot,
  });
  // 叠加层在容量边界淘汰过更新的记录（R07-CAP-1）：这份回执的序号不大于淘汰水位，
  // 说明可能有更高序号的真值已被丢弃，仅凭现有信息无法安全合成。
  // **拒绝发布**并要求重新读取——不猜、不让被淘汰的旧状态重新认领。
  //
  // 判定放在整份回执写完之后：写入过程中被淘汰的记录与本回执同序号，
  // 它们被丢弃只说明「这条路径本回执确认过」，不构成信息不足。
  const trustworthy = input.overlay.isTrustworthy(input.worktreeId, input.seq);
  if (!contextValid || !view || !trustworthy) {
    return {
      view: null,
      navigation: { paths: [], reviewed: [...plan.reviewed], total: 0, reviewedCount: plan.reviewed.size, seq: plan.seq },
      clearError: false,
      needsRefresh: !trustworthy,
    };
  }
  return { view, navigation: view.review, clearError, needsRefresh: false };
}

/**
 * 在调用方**当前**快照上合并一次已合成的审阅结果。
 *
 * 只改 `reviewed` 与进度，其余字段（metricsLoaded、行数等）一律保留 `current` 的值：
 * 同 revision 的元数据补全可能已经排队但还没 render，整批覆盖会把它们退回旧值
 * （SP07-R11-4）。
 */
export function mergePublishedReview<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(
  current: S,
  view: ReviewSnapshotLike<ReviewMarkEntryLike> | null | undefined,
  seq: number,
): S {
  // 发布被拒绝（`needsRefresh`，例如叠加层容量淘汰后信息不足）时调用方可能直接
  // 传 `null`：此时**不改变界面**，等重新读取真实状态（R07-CAP-1）。
  if (!view) return current;
  const reviewedByPath = new Map(view.entries.map((entry) => [entry.path, entry.reviewed]));
  const entries = current.entries.map((entry) => {
    const reviewed = reviewedByPath.get(entry.path);
    return reviewed === undefined || reviewed === entry.reviewed
      ? entry
      : { ...entry, reviewed };
  });
  return {
    ...current,
    review: { ...view.review, seq },
    entries,
  } as S;
}

/**
 * 连续审阅导航要用的「已审阅集合」。
 *
 * **只**用合成后的真值（`published` 里的进度），不把发起时的当前文件强行补成已审：
 * 它可能已经被更晚的操作取消（⌥⌘R 标 A → B 的结果先显示 → 用户取消 A →
 * 旧 A 回执释放，合成真值是 A 未审）。强行补 true 会误报「全部已审阅」
 * 并跳过真正未审的文件（SP07-R12-1）。
 *
 * `navigation` 的 `paths` 为空（上下文失效、未发布）时返回 `null`，
 * 调用方应终止这次过时导航。
 */
export function reviewedSetForNavigation(
  published: { view: unknown; navigation: ReviewStateLike } | null,
): Set<string> | null {
  if (!published || !published.view) return null;
  return new Set(published.navigation.reviewed);
}

/**
 * 一次**单次**真实读取尝试的结果（ST07-R16-1）。
 *
 * runner 只报告这一次读取的可信度或失败，**不递归等待调度器**：
 * 后续要不要再试由外层批次决定，避免 task→runner→schedule→task 的自等待环。
 */
export type ReviewReadAttempt =
  | { status: "trusted" }
  | { status: "untrusted"; worktreeId: string; observedSeq: number }
  | { status: "error"; error: unknown };

/** 一个恢复批次的结果。 */
export interface ReviewRecoveryResult {
  /** 本批次是否拿到了可信结果。 */
  converged: boolean;
  /** 本批次实际尝试次数。 */
  attempts: number;
  /** 尝试用尽仍未收敛（调用方必须处理，不能静默）。 */
  exhausted: boolean;
  /** 本批次最后一次的失败原因（错误或不可信），用于错误展示。 */
  lastFailure: ReviewReadAttempt | null;
}

/**
 * 不可信读取的**批次式**重新核对（ST07-R16-1 / SP07-R16-2 / SP07-R16-3）。
 *
 * 设计要点：
 * - `runner` 只做**一次**真实读取并返回它的可信度/失败，绝不回调本调度器，
 *   所以不存在自等待；调用方的 await 一定会结束。
 * - 预算按**批次**计算：每批最多 `maxAttempts` 次，批次结束后计数清零。
 *   不是终身熔断——耗尽后新的显式核对（或可信读取）会开新批次。
 * - 单飞按 worktree：同一目标并发请求共享同一批次，不产生请求风暴。
 * - 只重读（`runner` 由调用方实现为 listChanges 之类的读取），不重跑写命令。
 */
export class ReviewRecoveryScheduler {
  private readonly inFlight = new Map<string, Promise<ReviewRecoveryBatchOutcome>>();
  private readonly attempts = new Map<string, number>();
  private readonly maxAttempts: number;

  constructor(maxAttempts = 3) {
    this.maxAttempts = maxAttempts;
  }

  /** 当前批次已尝试的次数（空闲时为 0）。 */
  attemptsFor(worktreeId: string): number {
    return this.attempts.get(worktreeId) ?? 0;
  }

  /** 是否有正在进行的批次。 */
  isRunning(worktreeId: string): boolean {
    return this.inFlight.has(worktreeId);
  }

  /**
   * 跑一个恢复批次；返回本批次结果。
   *
   * 同一 worktree 并发调用共享**同一个批次**（单飞）：批次结束即释放，
   * 之后新的调用会开启新批次（不永久禁用）。
   *
   * 资格与失败展示都在**批次内部**完成，因此：
   * - 晚加入者拿到的是同一个 promise，不会重新取得资格（ST-B01-07-1）；
   * - 失败最多展示一次（`showFailure` 只在本批次内调用一次）。
   */
  run(
    worktreeId: string,
    read: () => Promise<ReviewReadAttempt>,
    hooks?: {
      beginEligibility?: () => () => boolean;
      showFailure?: (failure: ReviewReadAttempt | null) => void;
    },
  ): Promise<ReviewRecoveryBatchOutcome> {
    const running = this.inFlight.get(worktreeId);
    if (running) return running;
    const task = (async (): Promise<ReviewRecoveryBatchOutcome> => {
      // 资格在**真实批次创建时**取一次；晚加入者共享本任务，不会再取。
      const eligible = hooks?.beginEligibility?.() ?? (() => true);
      let lastFailure: ReviewReadAttempt | null = null;
      for (let attempt = 1; attempt <= this.maxAttempts; attempt += 1) {
        this.attempts.set(worktreeId, attempt);
        let result: ReviewReadAttempt;
        try {
          result = await read();
        } catch (error) {
          result = { status: "error", error };
        }
        if (result.status === "trusted") {
          return { converged: true, attempts: attempt, exhausted: false, lastFailure: null, failureShown: false };
        }
        lastFailure = result;
      }
      const exhausted: ReviewRecoveryBatchOutcome = {
        converged: false,
        attempts: this.maxAttempts,
        exhausted: true,
        lastFailure,
        failureShown: false,
      };
      if (!eligible()) return exhausted;
      // 本批次唯一一次失败展示。
      hooks?.showFailure?.(lastFailure);
      return { ...exhausted, failureShown: true };
    })();
    this.inFlight.set(worktreeId, task);
    // 批次结束后释放单飞槽并清零计数：后续独立拒收能开新批次。
    void task.finally(() => {
      this.inFlight.delete(worktreeId);
      this.attempts.delete(worktreeId);
    });
    return task;
  }
}

/**
 * 一次恢复批次的**生产编排入口**（ST07-R17-1 / ST07-R17-2 / SP07-R17-2）。
 *
 * App 与 Node 回归都用这个函数，不各自实现一份错误资格判断：
 * - `beginEligibility()` 在**批次开始时只调用一次**，返回本批共同的展示资格判定。
 *   之后每次 attempt、以及最终耗尽提示，都用**同一份**资格，
 *   所以批次期间出现的新错误不会被重试或耗尽出口覆盖。
 * - `read()` 只做一次真实读取并返回可信度/失败，**不展示错误**；
 *   展示完全由本函数在批次结束时按资格决定，路径唯一。
 * - 用户关闭提示（`dismissError`）会推进代际，因此批次结束后旧资格自动失效，
 *   不会把已关闭的提示重新写回来。
 * - 只读取，不重跑任何写命令。
 */
export interface ReviewRecoveryBatchInput {
  scheduler: ReviewRecoveryScheduler;
  worktreeId: string;
  /** 单次真实读取；只返回结果，不展示错误。 */
  read: () => Promise<ReviewReadAttempt>;
  /** 批次开始时调用一次，返回本批共同的「仍可展示」判定。 */
  beginEligibility: () => () => boolean;
  /** 展示一条失败（错误或耗尽提示）；只在资格仍有效时由本函数调用。 */
  showFailure: (failure: ReviewReadAttempt | null) => void;
}

export interface ReviewRecoveryBatchOutcome extends ReviewRecoveryResult {
  /** 本批次是否真的展示了失败提示（受批次级资格保护）。 */
  failureShown: boolean;
}

export async function runReviewRecoveryBatch(
  input: ReviewRecoveryBatchInput,
): Promise<ReviewRecoveryBatchOutcome> {
  // 直接委托给**真实共享批次**：资格与失败展示都在批次内部完成，
  // 晚加入者共享同一 promise，不会重新取得资格（ST-B01-07-1 / SP-B01-07-1）。
  return input.scheduler.run(input.worktreeId, input.read, {
    beginEligibility: input.beginEligibility,
    showFailure: input.showFailure,
  });
}

// ---------------- 写入期间的安全关闭/退出（任务 08，Q90/Q91/Q96）----------------

/** 关闭/退出请求的类型。 */
export type CloseIntent = "close" | "exit";

/** 一次关闭请求在某个时刻应该做什么。 */
export type CloseDecision =
  /** 没有在途写操作：可以立刻执行原请求。 */
  | { action: "proceed" }
  /** 有在途写操作：保留窗口与进程，提示「等待完成后关闭/退出」或「留在应用」。 */
  | { action: "ask"; writing: string[] }
  /** 存在待核对结果：先展示，不静默退出（Q91、Q96）。 */
  | { action: "showPending"; writing: string[]; pending: string[] };

/** 用户在询问对话框里的选择。 */
export type CloseChoice = "wait" | "stay";

/**
 * 一次 reconcile 的结论（所有生命周期事件都走它）。
 *
 * - `none`：什么都不做（没有请求，或条件未满足）；
 * - `proceed`：执行 `intent`（**只能是用户已授权等待的原动作**）；
 * - `ask`：展示询问（含当前真实在途/待核对集合与**被保留的原动作**）。
 */
export type CloseReconcile =
  | { kind: "none" }
  | { kind: "proceed"; intent: CloseIntent }
  | { kind: "ask"; intent: CloseIntent; writing: string[]; pending: string[] };

/**
 * 写入期间的关闭/退出编排（任务 08，Q90/Q91/Q96）。
 *
 * 这是**唯一**的生产编排：App 在写结束、晚点 Wait、保护失败回滚、另存收尾、
 * 待核对成功/失败、Stay、重复请求等所有生命周期事件后都调用 `reconcile()`，
 * 由它给出「执行原请求 / 展示询问 / 什么都不做」。
 *
 * 规则：
 * - 原请求动作以**第一次**为准，询问期间再次请求不替换（ST-B03-08-7）；
 * - 记录原请求**不等于授权**，只有 `resolveAsk("wait")` 才授权（ST-B03-08-3）；
 * - `reconcile()` 会消费授权：满足条件就返回 `proceed`，否则返回 `none`/`ask`；
 * - 待核对按**代际**核对，失败或新增待核对都不清账（ST-B03-08-6）；
 * - 每次 `request()` 与 `resolveAsk("stay")` 都推进 `requestGeneration`，
 *   核对流程必须捕获它，旧核对不得授权新请求（ST-B03-08-5）。
 */
export class CloseCoordinator {
  /** 在途写操作的 worktree 集合（含无归属另存使用的占位键）。 */
  private readonly writing = new Set<string>();
  /** 待核对的目标 → 代际。 */
  private readonly pending = new Map<string, number>();
  private pendingSeq = 0;
  /** 原请求动作（第一次为准）；不构成授权。 */
  private requested: CloseIntent | null = null;
  /** 用户明确选择「等待」后保留的授权动作。 */
  private authorized: CloseIntent | null = null;
  /** 请求代际：每次新的请求或 Stay 取消都递增。 */
  private requestGeneration = 0;

  /** 当前请求代际（核对流程必须捕获它）。 */
  generation(): number {
    return this.requestGeneration;
  }

  beginWrite(worktreeId: string): void {
    this.writing.add(worktreeId);
  }

  /**
   * 结束一个写操作。`needsVerification` 表示结果无法确定 → 新增一条待核对（代际 +1）。
   * 结束写操作**不会**清除已有待核对（Q91）。
   */
  endWrite(worktreeId: string, needsVerification = false): void {
    this.writing.delete(worktreeId);
    if (needsVerification) {
      this.pendingSeq += 1;
      this.pending.set(worktreeId, this.pendingSeq);
    }
  }

  /** 保护前置失败时的回滚：撤销登记，不留幽灵在途/授权。 */
  rollbackWrite(worktreeId: string): void {
    this.writing.delete(worktreeId);
  }

  pendingWorktrees(): string[] {
    return [...this.pending.keys()];
  }

  pendingGeneration(worktreeId: string): number {
    return this.pending.get(worktreeId) ?? 0;
  }

  writingWorktrees(): string[] {
    return [...this.writing];
  }

  hasWriting(): boolean {
    return this.writing.size > 0;
  }

  hasPending(): boolean {
    return this.pending.size > 0;
  }

  /** 是否需要阻止原生退出（在途写 **或** 待核对）。 */
  needsExitGuard(): boolean {
    return this.writing.size > 0 || this.pending.size > 0;
  }

  /** 被保留的原请求动作（展示用；不等于授权）。 */
  pendingIntent(): CloseIntent | null {
    return this.requested;
  }

  authorizedIntent(): CloseIntent | null {
    return this.authorized;
  }

  /**
   * 请求关闭/退出；返回此刻应该做什么。
   *
   * 原动作以**第一次**为准；询问期间再次请求不替换（ST-B03-08-7）。
   * 记录请求不等于授权：只有用户之后选择「等待」才会执行。
   */
  request(intent: CloseIntent): CloseDecision {
    if (this.requested === null) {
      this.requested = intent;
      this.requestGeneration += 1;
    }
    if (this.writing.size > 0 || this.pending.size > 0) {
      if (this.pending.size > 0) {
        return { action: "showPending", writing: [...this.writing], pending: this.pendingWorktrees() };
      }
      return { action: "ask", writing: [...this.writing] };
    }
    // 无在途写、无待核对：直接执行原请求。
    const action = this.requested;
    this.requested = null;
    this.authorized = null;
    return action === null ? { action: "proceed" } : { action: "proceed" };
  }

  /**
   * 用户的选择。
   *
   * - `wait`：授权执行**原请求动作**；
   * - `stay`：清掉请求与授权，并**推进请求代际**，使在途核对回调失效。
   */
  resolveAsk(choice: CloseChoice): void {
    if (choice === "stay") {
      this.requested = null;
      this.authorized = null;
      this.requestGeneration += 1;
      return;
    }
    this.authorized = this.requested;
  }

  keepOpen(): void {
    this.resolveAsk("stay");
  }

  /**
   * 统一收尾：所有生命周期事件后调用。
   *
   * 返回 `proceed` 时调用方执行 `intent`；返回 `ask` 时调用方展示对话框
   * （动作取自 `intent`，即**被保留的原请求**）。
   */
  reconcile(): CloseReconcile {
    if (this.writing.size > 0 || this.pending.size > 0) {
      if (this.requested === null) return { kind: "none" };
      return {
        kind: "ask",
        intent: this.requested,
        writing: [...this.writing],
        pending: this.pendingWorktrees(),
      };
    }
    if (this.authorized === null) return { kind: "none" };
    const intent = this.authorized;
    this.authorized = null;
    this.requested = null;
    return { kind: "proceed", intent };
  }

  /**
   * 核对**指定代际**的待核对：只有代际仍一致才清除。
   * 核对失败或该目标又出现新待核对时代际已变，返回 false 且不清账。
   */
  acknowledgePending(worktreeId: string, generation: number): boolean {
    if ((this.pending.get(worktreeId) ?? 0) !== generation) return false;
    this.pending.delete(worktreeId);
    return true;
  }

  /**
   * 用户已核对并明确「继续」：授权原请求，之后由 `reconcile()` 执行。
   * **不**批量清除 pending（会误清核对期间新出现的待核对）。
   */
  confirmPendingContinue(): void {
    if (this.requested !== null) this.authorized = this.requested;
  }
}

/**
 * 退出保护的**版本化同步器**（任务 08、ST-B03-08-1）。
 *
 * 本地 Tauri 每个 invoke 是独立 fetch，仅靠 JS 丢弃旧回包不足以保证 Rust 的实际
 * 状态：旧 `false` 的请求晚到仍会解除新建立的保护。这里给每次更新分配单调递增
 * 的 `seq`，由 Rust 侧只接受更大的序号，所以**旧释放绝不能覆盖新占用**。
 *
 * App 与 Node 回归用同一个类；测试直接驱动它，不复制算法。
 */
export class ExitGuardSync {
  private seq = 0;
  /** 已被 Rust 接受的最新状态（诊断与测试用）。 */
  private applied: boolean | null = null;
  /** 已接受的最高序号。 */
  private appliedSeq = 0;
  private readonly send: (request: { blocked: boolean; seq: number }) => Promise<boolean>;

  constructor(send: (request: { blocked: boolean; seq: number }) => Promise<boolean>) {
    this.send = send;
  }

  /** 当前生效的保护状态（尚未同步过时为 `null`）。 */
  current(): boolean | null {
    return this.applied;
  }

  /** 已接受的最高序号。 */
  lastSeq(): number {
    return this.appliedSeq;
  }

  /**
   * 请求设置保护；返回是否被接受。
   *
   * 每次调用都分配更大的 `seq`；接收端忽略更小的序号，因此调用顺序与网络到达
   * 顺序无关。被拒绝（旧序号）时不改变本地已知状态。
   */
  async set(blocked: boolean): Promise<boolean> {
    this.seq += 1;
    const seq = this.seq;
    const accepted = await this.send({ blocked, seq });
    if (accepted) {
      this.applied = blocked;
      this.appliedSeq = Math.max(this.appliedSeq, seq);
    }
    return accepted;
  }
}

/**
 * 关闭/退出保护监听的**就绪闸门**（任务 08、ST-B03-08-2 / SP-B03-08-1）。
 *
 * 窗口关闭监听与原生退出监听**都**装好才允许开始写入；安装中、失败、卸载后
 * 一律视为未就绪。写入口据此拒绝执行，而不是"报告错误后继续未受保护地写"。
 */
export class CloseGuardGate {
  private windowReady = false;
  private exitReady = false;
  private disposed = false;
  private reason: string | null = null;

  /** 记录某个监听安装成功。 */
  markInstalled(which: "window" | "exit"): void {
    if (which === "window") this.windowReady = true;
    else this.exitReady = true;
    this.reason = null;
  }

  /** 记录安装失败（附带可见原因）。 */
  markFailed(reason: string): void {
    this.reason = reason;
  }

  /** 组件卸载/重装：立即回到未就绪。 */
  dispose(): void {
    this.disposed = true;
    this.windowReady = false;
    this.exitReady = false;
  }

  /** 两个监听都就绪且未卸载。 */
  get ready(): boolean {
    return !this.disposed && this.windowReady && this.exitReady;
  }

  /** 未就绪时的原因（就绪时为 `null`）。 */
  get blockedReason(): string | null {
    if (this.ready) return null;
    return this.reason;
  }
}
