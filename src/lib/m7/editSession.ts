/** 编辑工作区与草稿的实际生命周期；不依赖组件是否挂载，所有请求保留原目标。 */
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { EditableFile } from "@/types/bindings/EditableFile";
import type { EditDraft } from "@/types/bindings/EditDraft";
import type { EditSaved } from "@/types/bindings/EditSaved";
import type { EditSavePayload } from "@/lib/m7/api";
import { localizedError, toAppError } from "@/lib/error";

export interface EditTarget { worktreeId: string; path: string }
export interface EditSessionDeps {
  open: (target: EditTarget) => Promise<EditableFile>;
  loadDraft: (target: EditTarget) => Promise<EditDraft | null>;
  listDrafts: (worktreeId: string) => Promise<EditDraft[]>;
  persist: (draft: EditDraft) => Promise<unknown>;
  clear: (target: EditTarget, identity: string) => Promise<void>;
  save: (request: EditSavePayload) => Promise<EditSaved>;
  blocked: (worktreeId: string) => boolean;
  report: (target: EditTarget, error: AppErrorPayload) => void;
}
export interface EditState {
  file: EditableFile | null;
  text: string;
  diskText: string;
  savedText: string;
  baseSha: string | null;
  diskSha: string | null;
  identity: string | null;
  identityValid: boolean;
  loading: boolean;
  saving: boolean;
  conflict: boolean;
  storedDraft: boolean;
  needsVerification: boolean;
  error: AppErrorPayload | null;
  draftError: AppErrorPayload | null;
}
const normalized = (text: string) => text.replace(/\r\n/g, "\n");

export class EditSession {
  readonly target: EditTarget;
  private readonly deps: EditSessionDeps;
  private readonly changed: () => void;
  private readonly listeners = new Set<() => void>();
  private opening: Promise<void> | null = null;
  private inFlight: Promise<boolean> | null = null;
  private discarding = false;
  private draftTail: Promise<unknown> = Promise.resolve();
  private state: EditState = { file: null, text: "", diskText: "", savedText: "", baseSha: null,
    diskSha: null, identity: null, identityValid: true, loading: true, saving: false,
    conflict: false, storedDraft: false, needsVerification: false, error: null, draftError: null };

  constructor(target: EditTarget, deps: EditSessionDeps, changed: () => void) {
    this.target = { ...target }; this.deps = deps; this.changed = changed;
  }
  snapshot = (): EditState => this.state;
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener); return () => this.listeners.delete(listener);
  };
  get dirty(): boolean { return normalized(this.state.text) !== normalized(this.state.savedText); }
  get hasWork(): boolean { return this.dirty || this.state.saving || this.state.storedDraft || this.state.draftError !== null; }
  get blocked(): boolean { return this.deps.blocked(this.target.worktreeId); }
  private publish(patch: Partial<EditState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
    this.changed();
  }
  private failure(caught: unknown, draft = false): AppErrorPayload {
    const error = toAppError(caught);
    this.publish(draft ? { draftError: error } : { error });
    this.deps.report(this.target, error); return error;
  }
  seed(draft: EditDraft): void {
    if (this.state.file || this.dirty || this.state.storedDraft) return;
    this.publish({ text: draft.text, baseSha: draft.baseSha256, identity: draft.worktreeIdentity, storedDraft: true });
  }
  /** 每次重新进入都核对真实身份；有未保存版本时保留其原基线。 */
  open(): Promise<void> {
    if (this.opening) return this.opening;
    if (this.state.saving) return this.inFlight?.then(() => undefined) ?? Promise.resolve();
    this.publish({ loading: true });
    this.opening = (async () => {
      try {
        const opened = await this.deps.open(this.target);
        if (this.state.identity && this.state.identity !== opened.worktreeIdentity && this.hasWork) {
          this.publish({ identityValid: false });
          throw localizedError("rejected", "errors.session.worktreeIdentityChanged");
        }
        // 先等原目标已发草稿写结束；最新内存也可能是用户撤销回基线的正文。
        await this.draftTail;
        const draft = await this.deps.loadDraft(this.target);
        const keepMemory = this.state.file !== null && this.state.identity === opened.worktreeIdentity
          && (this.dirty || this.state.storedDraft || this.state.draftError !== null);
        const mine = keepMemory ? this.state.text : draft?.text ?? opened.text ?? "";
        const base = keepMemory ? this.state.baseSha : draft?.baseSha256 ?? opened.baseSha256;
        const dirty = normalized(mine) !== normalized(opened.text ?? "");
        this.publish({ file: opened, text: mine, diskText: opened.text ?? "", savedText: opened.text ?? "",
          baseSha: dirty ? base : opened.baseSha256, diskSha: opened.baseSha256,
          identity: opened.worktreeIdentity, identityValid: true, conflict: dirty && base !== opened.baseSha256,
          storedDraft: !!draft || (keepMemory && this.state.storedDraft), error: null });
        if (draft && !dirty) await this.clearStored();
      } catch (caught) { this.failure(caught); }
      finally { this.opening = null; this.publish({ loading: false }); }
    })();
    return this.opening;
  }
  change(text: string): void {
    if (this.blocked || this.state.loading || this.state.saving || !this.state.identityValid
      || !this.state.file || this.state.file.readonlyReason !== null) return;
    this.publish({ text });
  }
  private queue<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.draftTail.then(operation);
    // 失败被调用方显式报告；下一次用户输入/冲刷仍可尝试同一份保留内容。
    this.draftTail = result.then(() => undefined, () => undefined);
    return result;
  }
  async flushDraft(): Promise<void> {
    if (this.opening) await this.opening;
    // 放弃期间卸载/自动冲刷只能等待队列，不能把旧正文排在clear之后重建。
    if (this.discarding) { await this.draftTail; return; }
    const current = this.state;
    if (!current.identity || !current.baseSha) { await this.draftTail; return; }
    if (!this.dirty) {
      await this.draftTail;
      // 撤销回基线也必须排在旧持久化之后清理；期间有新输入则留给它的冲刷。
      if (!this.dirty && (this.state.storedDraft || this.state.draftError !== null)) await this.clearStored();
      return;
    }
    const draft: EditDraft = { ...this.target, worktreeIdentity: current.identity,
      baseSha256: current.baseSha, text: current.text, updatedAtMs: Date.now() };
    try {
      await this.queue(() => this.deps.persist(draft));
      this.publish({ storedDraft: true, draftError: null });
    } catch (caught) { throw this.failure(caught, true); }
  }
  async whenDraftIdle(): Promise<void> { if (this.opening) await this.opening; await this.draftTail; }
  private async clearStored(): Promise<void> {
    const identity = this.state.identity;
    if (!identity) return;
    try {
      await this.queue(() => this.deps.clear(this.target, identity));
      this.publish({ storedDraft: false, draftError: null });
    } catch (caught) { throw this.failure(caught, true); }
  }
  /** 返回实际成功，关闭不再等待React state提交来猜dirty值。 */
  save(): Promise<boolean> {
    if (this.inFlight) return this.inFlight;
    if (this.blocked || !this.state.identityValid || this.state.loading || this.state.conflict
      || this.state.needsVerification || !this.state.identity || !this.state.baseSha
      || !this.state.file || this.state.file.readonlyReason !== null) return Promise.resolve(false);
    const request: EditSavePayload = { ...this.target, worktreeIdentity: this.state.identity,
      baseSha256: this.state.baseSha, text: this.state.text };
    const needsWrite = this.dirty;
    this.publish({ saving: true, error: null });
    this.inFlight = (async () => {
      try {
        await this.flushDraft();
        if (needsWrite) {
          const result = await this.deps.save(request);
          this.publish({ baseSha: result.sha256, diskSha: result.sha256, savedText: request.text,
            diskText: request.text, conflict: false,
            file: { ...this.state.file!, text: request.text, baseSha256: result.sha256 } });
        }
        await this.clearStored();
        return !this.dirty;
      } catch (caught) {
        const error = this.failure(caught);
        if (error.kind === "needsVerification") this.publish({ needsVerification: true, conflict: true });
        if (error.code === "errors.m7.editFileChangedOnDisk") {
          // 留下草稿及旧基线，另取磁盘两版；此读取从不重发保存。
          try { const disk = await this.deps.open(this.target);
            this.publish({ diskText: disk.text ?? "", diskSha: disk.baseSha256, conflict: true });
          } catch (readError) { this.failure(readError); }
        }
        return false;
      } finally { this.inFlight = null; this.publish({ saving: false }); }
    })();
    return this.inFlight;
  }
  /** 用户核对了当前显示的磁盘版后确认新基线；磁盘再变时只更新展示，要求再次核对。 */
  async acceptDiskBaseline(): Promise<boolean> {
    if (this.blocked || this.state.saving || !this.state.identityValid) return false;
    try {
      const disk = await this.deps.open(this.target);
      if (disk.worktreeIdentity !== this.state.identity) throw localizedError("rejected", "errors.session.worktreeIdentityChanged");
      if (disk.text === null || disk.baseSha256 === null) return false;
      if (disk.baseSha256 !== this.state.diskSha) {
        this.publish({ diskText: disk.text, diskSha: disk.baseSha256, conflict: true }); return false;
      }
      this.publish({ file: disk, diskText: disk.text, savedText: disk.text, baseSha: disk.baseSha256,
        conflict: false, needsVerification: false, error: null });
      await this.flushDraft(); return true;
    } catch (caught) { this.failure(caught); return false; }
  }
  /** 排在全部已发草稿写之后清除；成功后先同步复位dirty，卸载不会重建它。 */
  async discard(): Promise<boolean> {
    if (this.state.saving || this.blocked) return false;
    this.discarding = true;
    this.publish({ saving: true });
    try {
      await this.clearStored();
      this.publish({ text: this.state.diskText, savedText: this.state.diskText, baseSha: this.state.diskSha,
        conflict: false, storedDraft: false, draftError: null, error: null, needsVerification: false });
      return true;
    } catch (caught) { this.failure(caught); return false; }
    finally { this.discarding = false; this.publish({ saving: false }); }
  }
}

export interface EditResolution { ok: boolean; saved: boolean }
export type EditDecision = (session: EditSession) => Promise<boolean>;
export class EditSessions {
  private readonly sessions = new Map<string, EditSession>();
  private readonly listeners = new Set<() => void>();
  private revision = 0;
  private closing = false;
  private resolving = false;
  private readonly deps: EditSessionDeps;
  constructor(deps: EditSessionDeps) { this.deps = deps; }
  subscribe = (listener: () => void): (() => void) => { this.listeners.add(listener); return () => this.listeners.delete(listener); };
  snapshot = (): number => this.revision;
  private changed = () => { this.revision += 1; for (const listener of this.listeners) listener(); };
  get(target: EditTarget): EditSession {
    const key = JSON.stringify([target.worktreeId, target.path]);
    let session = this.sessions.get(key);
    if (!session) {
      session = new EditSession(target, { ...this.deps, blocked: (id) => this.closing || this.deps.blocked(id) }, this.changed);
      this.sessions.set(key, session);
    }
    return session;
  }
  get hasWork(): boolean { return [...this.sessions.values()].some((session) => session.hasWork); }
  get worktreeIds(): string[] { return [...new Set([...this.sessions.values()].map((session) => session.target.worktreeId))]; }
  setClosing(value: boolean): void { if (this.closing !== value) { this.closing = value; this.changed(); } }
  async flush(): Promise<void> { for (const session of this.sessions.values()) await session.flushDraft(); }
  /** 启动时登记未挂载草稿，使原生退出保护也覆盖它们。 */
  async discover(ids: string[]): Promise<void> {
    for (const id of ids) for (const draft of await this.deps.listDrafts(id)) {
      this.get({ worktreeId: id, path: draft.path }).seed(draft);
    }
  }
  /** 持久草稿也参与相关文件处理；切到B的干净面板不会解除A的草稿保护。 */
  async resolve(ids: string[], paths: string[] | null, ask: EditDecision): Promise<EditResolution> {
    if (this.resolving || this.closing) return { ok: false, saved: false };
    this.resolving = true;
    let saved = false;
    const relevant = (target: EditTarget) => ids.includes(target.worktreeId) && (paths === null || paths.includes(target.path));
    try {
      const handled = new Set<EditSession>();
      const decide = async (session: EditSession): Promise<boolean> => {
        const before = session.snapshot().diskSha;
        if (!(await ask(session)) || session.hasWork) return false;
        if (session.snapshot().diskSha !== before) saved = true;
        handled.add(session); return true;
      };
      // 先处理内存中的原始版本，持久化失败也仍可选择明确放弃或取消。
      for (const session of this.sessions.values()) {
        if (!relevant(session.target)) continue;
        await session.whenDraftIdle();
        if (session.hasWork && !session.snapshot().file) await session.open();
        if (session.hasWork && !(await decide(session))) return { ok: false, saved };
      }
      for (const id of ids) for (const draft of await this.deps.listDrafts(id)) {
        if (paths && !paths.includes(draft.path)) continue;
        const session = this.get({ worktreeId: id, path: draft.path });
        session.seed(draft);
        if (!session.snapshot().file) await session.open();
      }
      for (const session of this.sessions.values()) {
        if (!relevant(session.target) || !session.hasWork || handled.has(session)) continue;
        if (!(await decide(session))) return { ok: false, saved };
      }
      // 决策/磁盘读取的await期间可能再次输入；不能用handled掩盖这份新草稿。
      const unresolved = [...this.sessions.values()].some((session) => relevant(session.target) && session.hasWork);
      return { ok: !unresolved, saved };
    } finally { this.resolving = false; }
  }
}
