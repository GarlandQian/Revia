import type { ReviewDraft } from "@/types/bindings/ReviewDraft";
import type { ReviewDraftBundle } from "@/types/bindings/ReviewDraftBundle";
import type { ReviewDraftScope } from "@/types/bindings/ReviewDraftScope";
export type ReviewForm = "commentBody" | "reviewBody" | "inlineForm";
export interface ReviewDraftDeps {
  save: (scope: ReviewDraftScope, draft: ReviewDraft) => Promise<ReviewDraft>;
  remove: (scope: ReviewDraftScope, id: string) => Promise<void>;
}
export interface ReviewDraftState { forms: Record<ReviewForm, ReviewDraft>; inline: ReviewDraft[]; older: ReviewDraft[]; error: unknown | null }
const empty = (kind: string): ReviewDraft => ({ id: "", repoKey: "", number: 0, headSha: undefined, scopeKey: null, targetKey: null, kind, path: "", side: "new", line: 1, body: "", createdAtMs: 0, updatedAtMs: 0 });
export class ReviewDraftSession {
  readonly scope: ReviewDraftScope;
  private deps: ReviewDraftDeps;
  private listeners = new Set<() => void>();
  private tail: Promise<unknown> = Promise.resolve();
  private state: ReviewDraftState;
  private pending = new Set<ReviewDraft>();
  private queued = 0;
  get hasWork() { return this.pending.size > 0 || this.queued > 0; }
  constructor(bundle: ReviewDraftBundle, deps: ReviewDraftDeps) {
    this.scope = bundle.scope; this.deps = deps;
    const form = (kind: ReviewForm) => bundle.drafts.find((draft) => draft.kind === kind) ?? empty(kind);
    this.state = { forms: { commentBody: form("commentBody"), reviewBody: form("reviewBody"), inlineForm: form("inlineForm") }, inline: bundle.drafts.filter((d) => d.kind === "inline"), older: bundle.older, error: null };
  }
  snapshot = () => this.state;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => this.listeners.delete(listener); };
  private publish(patch: Partial<ReviewDraftState>) { this.state = { ...this.state, ...patch }; for (const listener of this.listeners) listener(); }
  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    this.queued++; this.publish({});
    const result = this.tail.then(operation).finally(() => { this.queued--; this.publish({}); });
    this.tail = result.then(() => undefined, () => undefined); return result;
  }
  change(kind: ReviewForm, patch: Partial<Pick<ReviewDraft, "body" | "path" | "line" | "side">>) {
    const before = this.state.forms[kind];
    const draft = { ...before, ...patch, kind, updatedAtMs: Math.max(Date.now(), before.updatedAtMs + 1) };
    this.pending.add(draft);
    this.publish({ forms: { ...this.state.forms, [kind]: draft } });
    void this.enqueue(async () => {
      try {
        const saved = await this.deps.save(this.scope, draft);
        for (const pending of this.pending) if (pending.kind === kind && pending.updatedAtMs <= draft.updatedAtMs) this.pending.delete(pending);
        // 保存完成不覆盖其间新输入，也不向另一个版本的会话落状态。
        if (this.state.forms[kind] === draft) this.publish({ forms: { ...this.state.forms, [kind]: saved }, error: null });
      } catch (error) { this.publish({ error }); }
    });
  }
  async addInline(): Promise<void> {
    const form = this.state.forms.inlineForm;
    await this.enqueue(async () => {
      const saved = await this.deps.save(this.scope, { ...form, id: "", kind: "inline" });
      this.publish({ inline: [...this.state.inline.filter((d) => d.id !== saved.id), saved], error: null });
    });
    if (this.state.forms.inlineForm === form) this.change("inlineForm", { body: "" });
  }
  async removeInline(id: string) {
    await this.enqueue(() => this.deps.remove(this.scope, id));
    this.publish({ inline: this.state.inline.filter((d) => d.id !== id) });
  }
  async flush() {
    for (;;) {
      const tail = this.tail; await tail;
      if (tail !== this.tail) continue;
      if (!this.pending.size) return;
      // 仅在显式冲刷时重试保留的当前稿；期间的新输入也必须排完再允许正常退出。
      for (const kind of ["commentBody", "reviewBody", "inlineForm"] as const) {
        const draft = this.state.forms[kind];
        if (![...this.pending].some((d) => d.kind === kind)) continue;
        const saved = await this.enqueue(() => this.deps.save(this.scope, draft));
        for (const pending of this.pending) if (pending.kind === kind && pending.updatedAtMs <= draft.updatedAtMs) this.pending.delete(pending);
        if (this.state.forms[kind] === draft) this.publish({ forms: { ...this.state.forms, [kind]: saved }, error: null });
      }
    }
  }
  async published(kind: "commentBody" | "reviewBody", sent: ReviewDraft, inline: ReviewDraft[] = []) {
    // 后续输入仍属于同版本；只清掉实际发表的快照。
    const now = this.state.forms[kind];
    if (now.body === sent.body && now.updatedAtMs === sent.updatedAtMs) this.change(kind, { body: "" });
    this.publish({ inline: this.state.inline.filter((d) => !inline.some((p) => p.id === d.id && p.body === d.body && p.updatedAtMs === d.updatedAtMs)) });
    await this.flush();
  }
}
export class ReviewDraftSessions {
  private sessions = new Map<string, ReviewDraftSession>();
  private deps: ReviewDraftDeps;
  private listeners = new Set<() => void>();
  constructor(deps: ReviewDraftDeps) { this.deps = deps; }
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => this.listeners.delete(listener); };
  get hasWork() { return [...this.sessions.values()].some((session) => session.hasWork); }
  open(bundle: ReviewDraftBundle) {
    let session = this.sessions.get(bundle.scope.key);
    if (!session) { session = new ReviewDraftSession(bundle, this.deps); this.sessions.set(bundle.scope.key, session); session.subscribe(() => { for (const listener of this.listeners) listener(); }); }
    return session;
  }
  async flush() { for (const session of this.sessions.values()) await session.flush(); }
}
