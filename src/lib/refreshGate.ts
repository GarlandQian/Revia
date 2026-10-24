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

export class ReviewOverlay {
  /** 写入叠加层的次序（读取分界用）。 */
  private commitSeq = 0;
  private readonly overrides = new Map<string, ReviewOverride>();
  /** 上限：只保留最近若干条，避免长时间使用后无界增长。 */
  private readonly limit: number;

  constructor(limit = 1024) {
    this.limit = limit;
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
      if (oldest) this.overrides.delete(oldest[0]);
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
  apply<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(snapshot: S, boundary: number): S {
    const observedSeq = snapshot.review.seq ?? 0;
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
    if (!changed) return snapshot;
    return this.syncProgress({ ...snapshot, entries });
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
  ): E {
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
    return reviewed === loaded.reviewed ? loaded : { ...loaded, reviewed };
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
  deliver<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(snapshot: S): S {
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
): Promise<S> {
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
  if (!contextValid || !view) {
    // 上下文失效：不发布，但记账已经完成（迟到的真实成功不丢账）。
    return {
      view: null,
      navigation: { paths: [], reviewed: [...plan.reviewed], total: 0, reviewedCount: plan.reviewed.size, seq: plan.seq },
      clearError: false,
    };
  }
  return { view, navigation: view.review, clearError };
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
  view: ReviewSnapshotLike<ReviewMarkEntryLike>,
  seq: number,
): S {
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
