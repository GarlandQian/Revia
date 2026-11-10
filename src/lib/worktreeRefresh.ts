/**
 * Worktree 激活与刷新（M10 任务 01）。
 *
 * 同一 module 拥有：
 * - **激活归属**：切换目标时推进激活代号，旧目标的迟到读取、监视状态与错误一律丢弃（含 A→B→A）；
 * - **监视归属**：监视登记绑定在激活代号上；手动刷新、环境就绪与重载后都会重新核对登记，
 *   不会出现「刷新推进了 activation，但后端监视仍是旧 activation，于是再也收不到事件」；
 * - **读取接收**：所有读取都先取审阅分界再发请求，交付时合并；同一目标只接受版本更大的快照；
 * - **错误资格**：读取失败只归属它自己的目标；只有目标仍是当前激活目标时才画到界面上；
 * - **原目标核对**：写后核对永远读**原目标**，界面是否接收单独判定（Q90、Q91）；
 * - **事件合并**：监视事件 / 回到前台经同一个单飞协调器合并，写入期间暂停，结束后补一轮。
 *
 * 调用方（`src/App.tsx`）只表达意图（打开、手动刷新、写后核对、环境就绪），
 * 不再自己维护 activation 与监视登记之间的同步，也不再各自复制「是否还该发布」的判断。
 */
import {
  RefreshCoordinator,
  ReviewOverlay,
  SnapshotGate,
  type ReviewMarkEntryLike,
  type ReviewSnapshotLike,
  WatchEventGate,
  readThroughReview,
} from "./refreshGate";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";

/** 读取接收使用的激活上下文。 */
export interface RefreshToken {
  activation: number;
  worktreeId: string | null;
}

/** 快照只需要这两项就能参与接收判定。 */
export interface RefreshSnapshotLike extends ReviewSnapshotLike<ReviewMarkEntryLike> {
  revision: number;
}

export interface WatchStatusLike {
  active: boolean;
  error: unknown | null;
  target?: string | null;
}

export interface WatchPollLike {
  status: { activation: number; active: boolean; error: unknown | null; target?: string | null };
  changedActivation: number | null;
  eventSequence: number;
}

/** 一次读取的结果（打开 / 刷新 / 写后核对共用）。 */
export type RefreshReadResult<S> =
  | { ok: true; published: boolean; snapshot: S }
  | { ok: false; reason: "untrusted" | "stale" | "failed" };

export interface WorktreeRefreshDeps<S extends RefreshSnapshotLike> {
  /** 审阅叠加层：读取必须先取分界，交付时合并。 */
  overlay: ReviewOverlay;
  /** 一次真实 `list_changes`。 */
  listChanges: (worktreeId: string) => Promise<S>;
  /** 打开路径的发布（带选中项与首屏补取）。 */
  publishOpened: (snapshot: S) => void;
  /** 自动 / 手动刷新的发布（保留选中项语义）。 */
  publishRefreshed: (snapshot: S) => void;
  /** 按 worktree 展示读取失败（本 module 已判断资格）。 */
  onError: (error: AppErrorPayload, worktreeId: string) => void;
  /** 不可信读取的重新核对（由 App 的恢复调度器承担）。 */
  recover: (worktreeId: string) => Promise<unknown>;
  /** 后端登记监视；activation 必须与读取接收使用同一个代号。 */
  registerWatch: (worktreeId: string, activation: number) => Promise<WatchStatusLike>;
  /** 轮询监视状态与变化序号。 */
  watchStatus: () => Promise<WatchPollLike>;
  /** 监视状态变化（只在仍属当前激活时调用）。 */
  onWatchState: (state: { active: boolean; error: unknown | null }) => void;
  /** 该 worktree 是否正在写：写期间暂停自动刷新，结束后只补一轮。 */
  isWriting: (worktreeId: string) => boolean;
  /** 当前监视目标的真实路径（用于丢弃属于旧目标的轮询结果）；无信息时返回 null。 */
  watchTarget: () => string | null;
}

interface WatchRegistration {
  activation: number;
  worktreeId: string;
  registered: boolean;
  attempt: Promise<void> | null;
}

/**
 * 生产用的激活与刷新 module。
 *
 * 说明：这里不持有 React 状态，也不发起渲染；显示层只通过 deps 接收「已确认属于当前目标」的结果。
 */
export class WorktreeRefresh<S extends RefreshSnapshotLike> {
  private readonly gate: SnapshotGate;
  private readonly deps: WorktreeRefreshDeps<S>;
  private readonly coordinator: RefreshCoordinator;
  /** 每个激活代号一份事件序号：切换后旧事件不能消费新目标的序号空间。 */
  private notifications = new WatchEventGate();
  private watch: WatchRegistration | null = null;
  private disposed = false;

  constructor(deps: WorktreeRefreshDeps<S>, gate: SnapshotGate = new SnapshotGate()) {
    this.deps = deps;
    this.gate = gate;
    this.coordinator = new RefreshCoordinator(
      () => this.refreshActive(),
      () => {
        const active = this.gate.current().worktreeId;
        return active === null || !this.deps.isWriting(active);
      },
    );
  }

  /** 读取接收门（其它模块判断远程目标是否仍有效时共用同一份事实）。 */
  snapshotGate(): SnapshotGate {
    return this.gate;
  }

  token(): RefreshToken {
    return this.gate.token();
  }

  currentWorktree(): string | null {
    return this.gate.current().worktreeId;
  }

  /**
   * 切换激活目标。旧监视登记与事件序号立即作废；新目标的监视登记由 `ensureWatch()` 负责。
   */
  activate(worktreeId: string | null): RefreshToken {
    const activation = this.gate.activate(worktreeId);
    this.notifications = new WatchEventGate();
    this.watch = null;
    return { activation, worktreeId };
  }

  /**
   * 确保当前激活目标已经登记监视。可重复调用：
   * - 已登记成功：直接返回（不重复向后端发请求）；
   * - 正在登记：等待同一次尝试；
   * - 尚未登记或上次失败：重新登记（手动刷新、环境就绪、重载后的重试路径）。
   */
  async ensureWatch(): Promise<void> {
    const token = this.gate.token();
    if (this.disposed || token.worktreeId === null) return;
    const current = this.watch;
    if (current && current.activation === token.activation) {
      if (current.attempt) {
        await current.attempt;
        return;
      }
      if (current.registered) return;
    }
    const record: WatchRegistration = {
      activation: token.activation,
      worktreeId: token.worktreeId,
      registered: false,
      attempt: null,
    };
    this.watch = record;
    const attempt = this.deps
      .registerWatch(record.worktreeId, record.activation)
      .then((status) => {
        if (this.disposed || !this.isCurrent(record)) return;
        record.registered = true;
        this.deps.onWatchState({ active: status.active, error: status.error });
      })
      .catch(() => {
        // 监视启动失败不阻塞文件列表；状态栏另有降级提示，下一次显式刷新会重试。
      })
      .finally(() => {
        record.attempt = null;
      });
    record.attempt = attempt;
    await attempt;
  }

  /**
   * 打开路径的读取：使用**当前**激活代号接收；调用方负责选中项与首屏补取。
   * 监视登记在调用方并行发起（`ensureWatch()`），不挡文件列表。
   */
  async readOpened(): Promise<RefreshReadResult<S>> {
    const token = this.gate.token();
    if (!token.worktreeId) return { ok: false, reason: "stale" };
    return this.readInto(token, token.worktreeId, "opened");
  }

  /** 手动刷新与环境就绪后的重读：不推进激活代号，因此监视登记继续有效。 */
  async refresh(): Promise<boolean> {
    if (this.disposed) return false;
    const token = this.gate.token();
    if (!token.worktreeId) return false;
    void this.ensureWatch();
    const result = await this.readInto(token, token.worktreeId, "refresh");
    return result.ok;
  }

  /** 自动刷新（监视事件 / 回到前台）：经单飞协调器合并，写入期间暂停。 */
  requestRefresh(): void {
    this.coordinator.request();
  }

  resume(): void {
    this.coordinator.resume();
  }

  whenIdle(): Promise<void> {
    return this.coordinator.whenIdle();
  }

  /**
   * 写后核对：**永远**读原目标；只有它仍是当前激活目标时才发布到界面。
   * 返回读取本身是否成功（用于「写结果已核对」判定），与是否发布分开。
   */
  async verifyWrite(targetWorktreeId: string): Promise<boolean> {
    const token = this.gate.token();
    const result = await this.readInto(token, targetWorktreeId, "target");
    return result.ok;
  }

  /** 监视轮询：只接受属于当前激活代号的 状态 / 变化序号。 */
  async pollWatch(): Promise<void> {
    if (this.disposed) return;
    const token = this.gate.token();
    if (!token.worktreeId) return;
    try {
      const snapshot = await this.deps.watchStatus();
      if (this.disposed || this.gate.token().activation !== token.activation) return;
      // 后端监视仍绑定旧 activation：属于旧目标的状态，丢弃（切换后 A→B→A 同理）。
      if (snapshot.status.activation !== token.activation) return;
      const expectedTarget = this.deps.watchTarget();
      if (expectedTarget && snapshot.status.target && snapshot.status.target !== expectedTarget) return;
      this.deps.onWatchState({ active: snapshot.status.active, error: snapshot.status.error });
      if (this.notifications.consume(token.activation, snapshot)) this.coordinator.request();
    } catch {
      // 状态查询失败不改变监视结论，等下一轮。
    }
  }

  dispose(): void {
    this.disposed = true;
    this.coordinator.dispose();
    this.watch = null;
  }

  private isCurrent(record: WatchRegistration): boolean {
    const token = this.gate.current();
    return token.activation === record.activation && token.worktreeId === record.worktreeId;
  }

  /**
   * 统一读取入口：审阅分界 → 真实读取 → 接收门 → 发布。
   *
   * `mode` 决定发布路径与错误资格：
   * - `opened`：打开路径，调用方补充选中与首屏；
   * - `refresh`：自动 / 手动刷新，保留选中项；
   * - `target`：写后核对，只读原目标，界面是否接收另判。
   */
  private async readInto(
    token: RefreshToken,
    targetWorktreeId: string,
    mode: "opened" | "refresh" | "target",
  ): Promise<RefreshReadResult<S>> {
    try {
      const read = await readThroughReview(this.deps.overlay, () => this.deps.listChanges(targetWorktreeId));
      if (this.disposed) return { ok: false, reason: "stale" };
      if (!read.ok) {
        // 不可信读取：按**原目标**重新核对，不返回旧快照冒充成功。
        await this.deps.recover(read.worktreeId);
        return { ok: false, reason: "untrusted" };
      }
      const snapshot = read.snapshot;
      // 接收门既判定激活归属，也判定同一目标内的版本先后。
      // 写后核对读的是**原目标**：真实读取是否核对成功只看版本单调性，界面切换不使读取失败；
      // 是否画到当前界面另行判定（Q90、Q91、M10 返修 SP7）。
      const envelope = { worktreeId: snapshot.worktreeId, revision: snapshot.revision };
      const accepted = mode === "target"
        ? this.gate.acceptTarget(envelope)
        : this.gate.accept(token, envelope);
      if (!accepted) {
        return { ok: false, reason: "stale" };
      }
      if (mode === "opened") {
        this.deps.publishOpened(snapshot);
        return { ok: true, published: true, snapshot };
      }
      if (mode === "refresh") {
        this.deps.publishRefreshed(snapshot);
        return { ok: true, published: true, snapshot };
      }
      // 写后核对：结果属于原目标；只有它仍是当前激活目标、且激活代号未变时才画到界面。
      // 切走（含 A→B→A）后这份结果已经不属于当前界面，但不影响读取本身的核对结论。
      if (this.isActiveTarget(targetWorktreeId) && this.gate.token().activation === token.activation) {
        this.deps.publishRefreshed(snapshot);
        return { ok: true, published: true, snapshot };
      }
      return { ok: true, published: false, snapshot };
    } catch (caught) {
      if (this.errorEligible(token, targetWorktreeId, mode)) {
        this.deps.onError(caught as AppErrorPayload, targetWorktreeId);
      }
      return { ok: false, reason: "failed" };
    }
  }

  /** 自动刷新的执行体：只读当前目标；结果仍按接收门判定。 */
  private async refreshActive(): Promise<void> {
    const token = this.gate.token();
    if (!token.worktreeId) return;
    await this.readInto(token, token.worktreeId, "refresh");
  }

  private isActiveTarget(targetWorktreeId: string): boolean {
    const token = this.gate.token();
    return token.worktreeId === targetWorktreeId;
  }

  /**
   * 错误资格：读取失败只归属它自己的目标。
   * - 打开 / 刷新：激活代号未变即可（同目标的手动刷新仍在自己的界面上报错）；
   * - 写后核对：还必须仍是当前激活目标（切走后原目标的失败不画到新界面）。
   */
  private errorEligible(
    token: RefreshToken,
    targetWorktreeId: string,
    mode: "opened" | "refresh" | "target",
  ): boolean {
    if (this.gate.token().activation !== token.activation) return false;
    if (mode === "target") return this.isActiveTarget(targetWorktreeId);
    return token.worktreeId === targetWorktreeId;
  }
}
