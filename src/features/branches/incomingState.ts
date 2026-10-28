import type { BranchOverview } from "@/types/bindings/BranchOverview";

/** A completion may only change the branch context that issued it. */
export interface IncomingToken {
  readonly worktreeId: string;
  readonly activation: number;
  readonly branchRevision: number;
}

/** Owns remote-check results independently of the network polling schedule. */
export class IncomingState {
  private worktreeId: string | null = null;
  private activation = 0;
  private branchRevision = 0;
  private checkRevision = 0;
  private enabled = false;
  private hasOverview = false;
  private branchRef: string | null = null;
  private upstream: string | null = null;
  private value = false;

  get incoming(): boolean {
    return this.value;
  }

  select(worktreeId: string | null, enabled: boolean): void {
    if (worktreeId !== this.worktreeId) {
      this.worktreeId = worktreeId;
      this.activation += 1;
      this.hasOverview = false;
      this.branchRef = null;
      this.upstream = null;
      this.value = false;
      this.cancelChecks();
    }
    if (enabled !== this.enabled) {
      this.enabled = enabled;
      this.value = false;
      this.cancelChecks();
    }
  }

  capture(worktreeId: string): IncomingToken | null {
    if (worktreeId !== this.worktreeId) return null;
    return { worktreeId, activation: this.activation, branchRevision: this.branchRevision };
  }

  /** An overview may establish a new branch, but never reactivate an old worktree. */
  observeOverview(token: IncomingToken | null, overview: BranchOverview): boolean {
    if (!token || !this.sameActivation(token) || overview.worktreeId !== token.worktreeId) return false;
    const branchRef = overview.headRef;
    const upstream = overview.local.find((branch) => branch.fullName === branchRef)?.upstream ?? null;
    if (!this.hasOverview || branchRef !== this.branchRef || upstream !== this.upstream) {
      // The first overview binds this activation; only later identity changes retire completions.
      if (this.hasOverview) this.branchRevision += 1;
      this.hasOverview = true;
      this.branchRef = branchRef;
      this.upstream = upstream;
      this.value = false;
      this.cancelChecks();
    }
    return true;
  }

  /** A successful fetch/update also retires checks issued before its completion. */
  clear(token: IncomingToken | null): boolean {
    if (!token || !this.sameBranch(token)) return false;
    this.value = false;
    this.cancelChecks();
    return true;
  }

  cancelChecks(): void {
    this.checkRevision += 1;
  }

  /** Returns whether the response was accepted, not the remote's incoming flag. */
  async check(runCheck: (worktreeId: string) => Promise<boolean>): Promise<boolean> {
    if (!this.enabled || !this.worktreeId || !this.hasOverview || !this.branchRef || !this.upstream) return false;
    const token = this.capture(this.worktreeId)!;
    const revision = ++this.checkRevision;
    const incoming = await runCheck(token.worktreeId);
    if (!this.enabled || revision !== this.checkRevision || !this.sameBranch(token)) return false;
    this.value = incoming;
    return true;
  }

  private sameActivation(token: IncomingToken): boolean {
    return token.worktreeId === this.worktreeId && token.activation === this.activation;
  }

  private sameBranch(token: IncomingToken): boolean {
    return this.sameActivation(token) && token.branchRevision === this.branchRevision;
  }
}
