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
 * 两个先后来源必须分开（ST07-R8-1）：
 * - `order`：用户在界面上**发起**这次标记操作的次序，由 `beginMark()` 在点击时分配。
 *   同一路径的相反操作以它判定谁更新，绝不用回执到达次序。
 * - `commitSeq`：写入叠加层的次序，只用于读取分界（判断某次读取能否观察到它）。
 *
 * `source` 区分「用户直接操作这条路径」与「后端全量结果顺带确认了这条路径」：
 * 聚合回执不能覆盖用户对同一路径的明确操作。
 */
export interface ReviewOverride {
  path: string;
  versions: ReviewVersionsLike;
  reviewed: boolean;
  order: number;
  source: ReviewRecordSource;
  commitSeq: number;
}

export type ReviewRecordSource = "explicit" | "aggregate";

/**
 * 一次标记操作的句柄：序号在**用户点击时**分配，回执到达时用它记账。
 *
 * 只由 `ReviewOverlay.beginMark()` 创建，调用方无法在回执到达时才决定先后。
 */
export class ReviewMarkRequest {
  private readonly order: number;

  constructor(order: number) {
    this.order = order;
  }

  /** 这次操作的发起次序（1 起，单调递增）。 */
  orderOf(): number {
    return this.order;
  }
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
}

export interface ReviewSnapshotLike<E extends ReviewMarkEntryLike> {
  worktreeId: string;
  entries: E[];
  review: ReviewStateLike;
}

export class ReviewOverlay {
  /** 写入叠加层的次序（读取分界用）。 */
  private commitSeq = 0;
  /** 用户标记操作的发起次序。 */
  private operationSeq = 0;
  private readonly overrides = new Map<string, ReviewOverride>();
  /** 上限：只保留最近若干条，避免长时间使用后无界增长。 */
  private readonly limit: number;

  constructor(limit = 1024) {
    this.limit = limit;
  }

  private static key(worktreeId: string, path: string): string {
    return `${worktreeId}\u0000${path}`;
  }

  /** 用户点击标记时分配操作次序（ST07-R8-1 的先后来源）。 */
  beginMark(): ReviewMarkRequest {
    this.operationSeq += 1;
    return new ReviewMarkRequest(this.operationSeq);
  }

  /**
   * 记录一次**已成功**的本地审阅变更；返回是否真的写入了叠加层。
   *
   * **必须在拿到成功结果后调用**，不能被展示代际（`isCurrent`）挡住：
   * 后端已经持久化，迟到响应只是不该更新界面，不能让后续旧读取把这条真实变更回退
   * （ST07-R7-3 / SP07-R7-2）。
   *
   * 是否覆盖同路径已有记录**只取决于操作先后**（`order`），与回执到达次序无关：
   * 迟到的旧勾选回执不能覆盖已成功的取消（ST07-R8-1 / SP07-R8-1）。
   * 聚合回执（`source: "aggregate"`）不覆盖用户对同一路径的明确操作。
   */
  record(
    worktreeId: string,
    path: string,
    versions: ReviewVersionsLike,
    reviewed: boolean,
    meta: { order: number; source: ReviewRecordSource },
  ): boolean {
    const key = ReviewOverlay.key(worktreeId, path);
    const existing = this.overrides.get(key);
    if (existing) {
      if (existing.source === "explicit") {
        // 用户对这条路径的直接操作优先：聚合回执永远不覆盖它，
        // 更早发起的明确操作也不能覆盖它。
        if (meta.source === "aggregate") return false;
        if (meta.order < existing.order) return false;
      } else if (meta.source === "explicit") {
        // 明确操作总能覆盖聚合回执。
      } else if (meta.order < existing.order) {
        return false;
      }
    }
    this.commitSeq += 1;
    this.overrides.set(key, {
      path,
      versions: { head: versions.head, worktree: versions.worktree },
      reviewed,
      order: meta.order,
      source: meta.source,
      commitSeq: this.commitSeq,
    });
    if (this.overrides.size > this.limit) {
      const oldest = [...this.overrides.entries()].sort((a, b) => a[1].commitSeq - b[1].commitSeq)[0];
      if (oldest) this.overrides.delete(oldest[0]);
    }
    return true;
  }

  /**
   * 读取/写请求**发起前**调用，拿到这次请求的审阅分界（ST07-R7-1 / SP07-R7-1 / SP07-R7-4）。
   *
   * 返回的句柄同时承担交付时的合并，所以「先取分界、后交付」由类型强制：
   * 拿不到句柄就无法交付，而句柄只在请求发起处创建。
   */
  beginRead(): ReviewReadRequest {
    return new ReviewReadRequest(this, this.commitSeq);
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
      if (!override || override.commitSeq <= baseline) return entry;
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

  /**
   * 用同路径的当前条目与本地记录修正一条被**整体替换**的条目（未跟踪元数据补全用）。
   *
   * `loaded` 是较早读取拿到的条目，`current` 是界面当前同组条目。
   * 只调整 `reviewed`：版本一致时沿用当前条目/较新的本地记录，
   * 补元数据不能把已成功的标记勾回去，也不会凭空勾上（版本不一致一律用 `loaded`）。
   */
  mergeEntry<E extends ReviewMarkEntryLike>(
    worktreeId: string,
    loaded: E,
    current: E | undefined,
    boundary: number,
  ): E {
    const head = loaded.headVersion ?? null;
    const worktree = loaded.worktreeVersion ?? null;
    let reviewed = loaded.reviewed;
    if (current
      && (current.headVersion ?? null) === head
      && (current.worktreeVersion ?? null) === worktree) {
      reviewed = current.reviewed;
    }
    const override = this.overrides.get(ReviewOverlay.key(worktreeId, loaded.path));
    if (override && override.commitSeq > boundary
      && override.versions.head === head && override.versions.worktree === worktree) {
      reviewed = override.reviewed;
    }
    return reviewed === loaded.reviewed ? loaded : { ...loaded, reviewed };
  }

  /**
   * 按条目重算进度，保证 `reviewed` / `reviewedCount` 与勾选一致（SP07-R7-3）。
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

  /** 交付结果：把这次请求发起之后、版本仍匹配的本地审阅变更叠加进去。 */
  deliver<S extends ReviewSnapshotLike<ReviewMarkEntryLike>>(snapshot: S): S {
    return this.overlay.apply(snapshot, this.boundary);
  }
}

/**
 * 生产入口的统一读取方式：**先取分界，再发请求**，结果到达时合并交付。
 *
 * `src/App.tsx` 的 `readAndAccept`、`refreshTarget`、`openWorktree` 与写结果分支
 * 都必须经这里，否则「分界取晚」会重新出现（ST07-R7-1 / SP07-R7-1 / SP07-R7-4）。
 * 分界与请求的先后在函数体内固定，调用方无法传反。
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
  /** 需要写入叠加层的条目，每条都带**它自己**被确认时的版本。 */
  records: Array<{
    path: string;
    versions: ReviewVersionsLike;
    reviewed: boolean;
    source: ReviewRecordSource;
  }>;
  /** 后端返回的完整已审阅集合，用于更新界面。 */
  reviewed: Set<string>;
}

/**
 * 计算一次**已成功**的标记结果要记录的全部变更（ST07-R7-3 / SP07-R7-2）。
 *
 * 两条硬约束：
 *
 * 1. **版本来源**（ST07-R8-2 / SP07-R8-2）：每条记录的版本必须来自
 *    **请求发起时**的条目（`entriesAtRequest`），也就是回执实际确认的那个版本。
 *    绝不能用回执到达时的当前条目补版本——否则「B 在 v1 已审、外部改成 v2 后
 *    旧回执到达」会被记成 B(v2, 已审)，把用户没看过的 v2 勾上。
 *    请求时不存在或版本不可确认的路径，一律不记（宁可漏记，不可错记）。
 *
 * 2. **先后来源**（ST07-R8-1 / SP07-R8-1）：每条记录带这次操作的 `order`
 *    （由 `beginMark()` 在点击时分配）。同路径的相反操作谁胜出只看 order，
 *    与回执到达次序无关。
 *
 * 除本次目标外，后端全量集合里被改变的其它路径以 `aggregate` 记账：
 * 它们确实已被后端持久化（A/B 并发时不能漏账），但不会覆盖用户对同一路径的明确操作。
 */
export function planReviewResult<E extends ReviewMarkEntryLike>(input: {
  worktreeId: string;
  path: string;
  /** 本次目标在**请求发起时**的版本（用户点击时看到的那一份）。 */
  versions: ReviewVersionsLike;
  /** 后端返回的完整已审阅集合。 */
  reviewed: Iterable<string>;
  /** **请求发起时**的条目快照，用于给聚合路径绑定其被确认的版本。 */
  entriesAtRequest: readonly E[];
  /** 本次操作的发起次序。 */
  order: number;
}): ReviewResultPlan {
  const reviewed = new Set(input.reviewed);
  const records: ReviewResultPlan["records"] = [{
    path: input.path,
    versions: { head: input.versions.head, worktree: input.versions.worktree },
    reviewed: reviewed.has(input.path),
    source: "explicit",
  }];
  for (const entry of input.entriesAtRequest) {
    if (entry.path === input.path) continue;
    // 版本不可确认的路径无法绑定，宁可不记。
    if (entry.worktreeVersionConfirmed === false) continue;
    const nowReviewed = reviewed.has(entry.path);
    if (nowReviewed === entry.reviewed) continue;
    records.push({
      path: entry.path,
      // 版本取自**请求时**的这条条目，不是回执到达时的当前条目。
      versions: { head: entry.headVersion ?? null, worktree: entry.worktreeVersion ?? null },
      reviewed: nowReviewed,
      source: "aggregate",
    });
  }
  return { records, reviewed };
}

/**
 * 把计划写进叠加层，带上这次操作的发起次序。
 *
 * 必须在展示门禁（`isCurrent`）之前调用：后端已经持久化，迟到响应只是不该更新界面，
 * 不能让后续旧读取把这条真实变更回退。
 */
export function commitReviewResult(
  overlay: ReviewOverlay,
  worktreeId: string,
  plan: ReviewResultPlan,
  order: number,
): void {
  for (const record of plan.records) {
    overlay.record(worktreeId, record.path, record.versions, record.reviewed, {
      order,
      source: record.source,
    });
  }
}

/**
 * 一次**已成功**的标记回执的完整记账入口（生产调用点唯一）。
 *
 * 参数刻意只收「**请求发起时**的条目快照」：版本来源只能是回执实际确认的那一份，
 * 调用方无法把回执到达时的当前条目塞进来补版本（ST07-R8-2 / SP07-R8-2）。
 * 先后来源是点击时分配的 `order`，与回执到达次序无关（ST07-R8-1 / SP07-R8-1）。
 */
export function applyReviewSuccess<E extends ReviewMarkEntryLike>(input: {
  overlay: ReviewOverlay;
  worktreeId: string;
  path: string;
  versions: ReviewVersionsLike;
  reviewed: Iterable<string>;
  /** 请求发起时的条目快照。 */
  entriesAtRequest: readonly E[];
  /** 点击时分配的发起次序。 */
  order: number;
}): ReviewResultPlan {
  const plan = planReviewResult({
    worktreeId: input.worktreeId,
    path: input.path,
    versions: input.versions,
    reviewed: input.reviewed,
    entriesAtRequest: input.entriesAtRequest,
    order: input.order,
  });
  commitReviewResult(input.overlay, input.worktreeId, plan, input.order);
  return plan;
}
