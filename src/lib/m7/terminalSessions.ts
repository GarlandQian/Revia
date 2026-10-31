/** 全局终端所有权与持久化；面板隐藏/切换不改变原worktree或丢失运行态。 */
import type { TerminalSessionInfo } from "@/types/bindings/TerminalSessionInfo";
import type { TerminalTabRecord } from "@/types/bindings/TerminalTabRecord";
import type { TerminalOutput } from "@/types/bindings/TerminalOutput";
import type { TerminalExit } from "@/types/bindings/TerminalExit";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { localizedError, toAppError } from "@/lib/error";
export interface TerminalDeps {
  list: () => Promise<TerminalSessionInfo[]>;
  loadTabs: () => Promise<TerminalTabRecord[]>;
  saveTabs: (tabs: TerminalTabRecord[]) => Promise<void>;
  open: (id: string, cwd: string | null) => Promise<TerminalSessionInfo>;
  close: (id: number) => Promise<void>;
  write: (id: number, data: string) => Promise<void>;
  resize: (id: number, rows: number, cols: number) => Promise<void>;
  blocked: (id: string) => boolean;
  report: (id: string | null, error: AppErrorPayload) => void;
  activity: (id: string) => void;
}
export class TerminalSessions {
  private readonly deps: TerminalDeps;
  private readonly sessions = new Map<number, TerminalSessionInfo>();
  private readonly closed = new Set<number>();
  private readonly exited = new Map<number, number | null>();
  private records: TerminalTabRecord[] = [];
  private readonly errors = new Map<string, AppErrorPayload>();
  private readonly pending = new Set<Promise<unknown>>();
  private readonly starting = new Map<string, number>();
  private readonly restores = new Map<string, Promise<void>>();
  private readonly restored = new Set<string>();
  private initialized: Promise<void> | null = null;
  private ready = false;
  private revision = 0;
  private readGeneration = 0;
  private writeTail: Promise<void> = Promise.resolve();
  private persistError: AppErrorPayload | null = null;
  private closing = false;
  private prepared: number[] | null = null;
  private readonly listeners = new Set<() => void>();
  private readonly outputListeners = new Set<(event: TerminalOutput) => void>();
  private readonly exitListeners = new Set<(event: TerminalExit) => void>();
  constructor(deps: TerminalDeps) { this.deps = deps; }
  subscribe = (fn: () => void): (() => void) => { this.listeners.add(fn); return () => this.listeners.delete(fn); };
  snapshot = (): number => this.revision;
  private changed() { this.revision++; for (const fn of this.listeners) fn(); }
  private fail(id: string | null, caught: unknown): AppErrorPayload {
    const error = toAppError(caught); if (id) this.errors.set(id, error);
    this.deps.report(id, error); this.changed(); return error;
  }
  errorFor(id: string): AppErrorPayload | null { return this.errors.get(id) ?? this.persistError; }
  list(id?: string): TerminalSessionInfo[] {
    return [...this.sessions.values()].filter((s) => id === undefined || s.worktreeId === id).sort((a, b) => a.sessionId - b.sessionId);
  }
  isStarting(id: string): boolean { return (this.starting.get(id) ?? 0) > 0 || this.restores.has(id); }
  get needsExitGuard(): boolean { return this.closing || this.pending.size > 0 || this.persistError !== null || this.list().some((s) => s.running); }
  get isClosing(): boolean { return this.closing; }
  private adopt(info: TerminalSessionInfo) {
    if (this.closed.has(info.sessionId)) return;
    this.sessions.set(info.sessionId, this.exited.has(info.sessionId)
      ? { ...info, running: false, exitCode: this.exited.get(info.sessionId)! } : info);
  }
  initialize(): Promise<void> {
    if (this.ready) return Promise.resolve();
    if (this.initialized) return this.initialized;
    this.initialized = (async () => {
      try {
        // 先完整加载旧页签，再接收任何新页签写；不能用初始空数组覆盖旧记录。
        this.records = await this.deps.loadTabs();
        await this.refresh(); this.ready = true;
      } catch (caught) { throw this.fail(null, caught); }
      finally { this.initialized = null; }
    })();
    return this.initialized;
  }
  async refresh(): Promise<void> {
    const generation = ++this.readGeneration;
    const revision = this.revision;
    const live = await this.deps.list();
    if (generation !== this.readGeneration) return;
    // 新打开/关闭/退出发生后，较早list不能删除刚登记的进程。
    if (revision === this.revision) {
      const ids = new Set(live.map((info) => info.sessionId));
      for (const id of this.sessions.keys()) if (!ids.has(id)) this.sessions.delete(id);
    }
    for (const info of live) this.adopt(info);
    this.changed();
  }
  private track<T>(work: Promise<T>): Promise<T> {
    this.pending.add(work); this.changed();
    void work.then(() => { this.pending.delete(work); this.changed(); }, () => { this.pending.delete(work); this.changed(); });
    return work;
  }
  private persistOwner(id: string): Promise<void> {
    const next = this.list(id).map((s, order) => ({ worktreeId: id, cwd: s.cwd, title: s.title, order }));
    this.records = [...this.records.filter((r) => r.worktreeId !== id), ...next];
    const records = this.records.map((r) => ({ ...r }));
    const write = this.writeTail.then(() => this.deps.saveTabs(records));
    this.writeTail = write.then(() => { this.persistError = null; this.changed(); }, (caught) => { this.persistError = this.fail(id, caught); });
    return this.track(write);
  }
  private start(id: string, cwd: string | null, persist: boolean): Promise<TerminalSessionInfo | null> {
    if (this.closing || this.deps.blocked(id)) { this.fail(id, localizedError("rejected", "errors.m7.terminalWorktreeBusy")); return Promise.resolve(null); }
    this.starting.set(id, (this.starting.get(id) ?? 0) + 1);
    return this.track((async () => {
      try {
        await this.initialize();
        if (this.closing || this.deps.blocked(id)) return null;
        const info = await this.deps.open(id, cwd);
        // 结果永远登记原worktree；组件是否仍显示它不影响进程可达性。
        this.adopt(info); this.errors.delete(id); this.changed();
        if (persist) await this.persistOwner(id);
        return info;
      } catch (caught) { this.fail(id, caught); return null; }
      finally { this.starting.set(id, Math.max(0, (this.starting.get(id) ?? 1) - 1)); this.changed(); }
    })());
  }
  open(id: string, cwd: string | null): Promise<TerminalSessionInfo | null> { return this.start(id, cwd, true); }
  restore(id: string): Promise<void> {
    if (this.restored.has(id) || this.closing) return Promise.resolve();
    const existing = this.restores.get(id); if (existing) return existing;
    const restore = (async () => {
      await this.initialize();
      if (this.closing) return;
      const mine = this.records.filter((r) => r.worktreeId === id).sort((a, b) => a.order - b.order);
      const unused = this.list(id);
      if (mine.length === 0 && unused.length === 0) {
        if (!(await this.start(id, null, false))) return;
      } else for (const record of mine) {
        if (this.closing) return;
        const found = unused.findIndex((s) => s.cwd === record.cwd);
        if (found >= 0) unused.splice(found, 1);
        else if (!(await this.start(id, record.cwd, false))) return;
      }
      this.restored.add(id); await this.persistOwner(id);
    })();
    this.restores.set(id, restore); this.track(restore);
    void restore.then(() => { this.restores.delete(id); this.changed(); }, (caught) => { this.restores.delete(id); this.fail(id, caught); });
    return restore;
  }
  async flush(): Promise<void> {
    await this.initialize(); await this.writeTail;
    if (this.persistError) throw this.persistError;
  }
  private owner(sessionId: number): TerminalSessionInfo {
    const info = this.sessions.get(sessionId);
    if (!info) throw localizedError("rejected", "errors.m7.terminalSessionUnknown");
    return info;
  }
  async input(sessionId: number, data: string): Promise<void> {
    const info = this.owner(sessionId);
    try {
      if (this.closing || this.deps.blocked(info.worktreeId)) throw localizedError("rejected", "errors.m7.terminalWorktreeBusy");
      await this.deps.write(sessionId, data);
    } catch (caught) { throw this.fail(info.worktreeId, caught); }
  }
  reportSessionFailure(sessionId: number, caught: unknown): void {
    const info = this.sessions.get(sessionId); if (info) this.fail(info.worktreeId, caught);
  }
  async resize(sessionId: number, rows: number, cols: number): Promise<void> {
    const info = this.owner(sessionId);
    try { await this.deps.resize(sessionId, rows, cols); }
    catch (caught) { throw this.fail(info.worktreeId, caught); }
  }
  async close(sessionId: number, confirmed = false): Promise<void> {
    const info = this.owner(sessionId);
    try {
      if (info.running && !confirmed) throw localizedError("rejected", "terminal.closeRunningConfirm");
      await this.deps.close(sessionId); this.closed.add(sessionId); this.sessions.delete(sessionId);
      this.changed(); await this.persistOwner(info.worktreeId);
    } catch (caught) { throw this.fail(info.worktreeId, caught); }
  }
  onOutput = (fn: (event: TerminalOutput) => void): (() => void) => { this.outputListeners.add(fn); return () => this.outputListeners.delete(fn); };
  onExit = (fn: (event: TerminalExit) => void): (() => void) => { this.exitListeners.add(fn); return () => this.exitListeners.delete(fn); };
  output(event: TerminalOutput): void {
    for (const fn of this.outputListeners) fn(event);
    const info = this.sessions.get(event.sessionId); if (info) this.deps.activity(info.worktreeId);
  }
  exitedSession(event: TerminalExit): void {
    this.exited.set(event.sessionId, event.exitCode ?? null);
    const info = this.sessions.get(event.sessionId);
    if (info) { this.sessions.set(event.sessionId, { ...info, running: false, exitCode: event.exitCode ?? null }); this.deps.activity(info.worktreeId); }
    this.changed(); for (const fn of this.exitListeners) fn(event);
  }
  /** 冻结新终端，等已发打开完成，再一次确认全局仍运行的原进程；取消不杀进程。 */
  async prepareShutdown(confirm: (sessions: TerminalSessionInfo[]) => Promise<boolean>): Promise<boolean> {
    if (this.closing) return false;
    this.closing = true; this.changed();
    try {
      while (this.pending.size) await Promise.allSettled([...this.pending]);
      await this.initialize(); await this.refresh();
      const running = this.list().filter((s) => s.running);
      if (running.length && !(await confirm(running))) { this.cancelShutdown(); return false; }
      this.prepared = this.list().map((s) => s.sessionId); return true;
    } catch (caught) { this.cancelShutdown(); this.fail(null, caught); return false; }
  }
  /** 只有关闭驱动实际执行原生动作时才终止已确认进程；先持久化页签，重启仍新开shell。 */
  async finishShutdown(): Promise<void> {
    if (!this.closing || !this.prepared) throw localizedError("rejected", "close.closing");
    await this.flush();
    for (const id of this.prepared) {
      if (!this.sessions.has(id)) continue;
      await this.deps.close(id); this.closed.add(id); this.sessions.delete(id);
    }
    this.changed();
  }
  cancelShutdown(): void { this.closing = false; this.prepared = null; this.changed(); }
}
