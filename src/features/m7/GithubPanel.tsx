/**
 * GitHub 面板（M7 任务 07、08、09、Q180 / Q181 / Q184）。
 *
 * 一个面板里完成：登录、PR 列表、详情与 diff、评论、本地评审草稿、
 * 检出、创建 PR、合并（含受保护分支确认）与回执。
 * 所有网络与 Git 动作都由 Rust 执行；这里只收集用户输入与展示结构化结果。
 */

import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { GithubStatus } from "@/types/bindings/GithubStatus";
import type { MergeMethod } from "@/types/bindings/MergeMethod";
import type { MergeReceipt } from "@/types/bindings/MergeReceipt";
import type { PrCheckoutOutcome } from "@/types/bindings/PrCheckoutOutcome";
import type { ProtectionInfo } from "@/types/bindings/ProtectionInfo";
import type { PullRequestComment } from "@/types/bindings/PullRequestComment";
import type { PullRequestDetail } from "@/types/bindings/PullRequestDetail";
import type { PullRequestFile } from "@/types/bindings/PullRequestFile";
import type { PullRequestFilter } from "@/types/bindings/PullRequestFilter";
import type { PullRequestSummary } from "@/types/bindings/PullRequestSummary";
import type { RateLimit } from "@/types/bindings/RateLimit";
import type { RemoteInfo } from "@/types/bindings/RemoteInfo";
import type { ReviewEvent } from "@/types/bindings/ReviewEvent";
import { Codicon } from "@/components/ui/codicon";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7, type PrCheckoutPayload } from "@/lib/m7/api";
import { reviewDraftSessions } from "@/lib/m7/reviewDrafts";
import type { ReviewDraftSession } from "@/lib/m7/reviewDraftSessions";
import { GithubRequests, githubResponse } from "@/lib/m7/githubRequests";
import { GithubDeviceLogin } from "./GithubDeviceLogin";

type Tab = "pulls" | "create" | "receipts";

export interface GithubPanelProps {
  worktreeId: string;
  worktreePath: string;
  currentBranch: string | null;
  onClose: () => void;
  /** 检出成功后让工作区刷新。 */
  onCheckedOut: (outcome: PrCheckoutOutcome) => void;
  /** 复用App全局写门、关闭协调与原目标待核对通道。 */
  onCheckout: (request: PrCheckoutPayload) => Promise<PrCheckoutOutcome>;
  /** 面板里做过的写操作（合并、评论等）也可能改变工作区状态。 */
  onWorkspaceTouched: () => void;
}

const MERGE_METHODS: MergeMethod[] = ["merge", "squash", "rebase"];
const REVIEW_EVENTS: ReviewEvent[] = ["comment", "approve", "requestChanges"];

/** 固定键映射：不使用拼接出来的翻译键（i18n 规则）。 */
function mergeMethodLabel(method: MergeMethod): string {
  switch (method) {
    case "merge":
      return t("github.mergeMethod.merge");
    case "squash":
      return t("github.mergeMethod.squash");
    case "rebase":
      return t("github.mergeMethod.rebase");
  }
}

function reviewEventLabel(event: ReviewEvent): string {
  switch (event) {
    case "comment":
      return t("github.reviewEvent.comment");
    case "approve":
      return t("github.reviewEvent.approve");
    case "requestChanges":
      return t("github.reviewEvent.requestChanges");
  }
}

function protectionSourceText(source: string): string {
  switch (source) {
    case "api": return t("github.protectionSynced");
    case "stale": return t("github.protectionStale");
    default: return t("github.protectionUnsynced");
  }
}

function patchLines(patch: string) {
  // F19：完整显示 patch 行；视口滚动由 max-h/overflow 控制，不再静默裁剪。
  return patch.split("\n");
}

export function GithubPanel({
  worktreeId,
  worktreePath,
  currentBranch,
  onClose,
  onCheckedOut,
  onCheckout,
  onWorkspaceTouched,
}: GithubPanelProps) {
  const [status, setStatus] = useState<GithubStatus | null>(null);
  const [remotes, setRemotes] = useState<RemoteInfo[]>([]);
  const [remote, setRemote] = useState<string | null>(null);
  const [filter, setFilter] = useState<PullRequestFilter>("open");
  const [page, setPage] = useState(1);
  const [items, setItems] = useState<PullRequestSummary[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [rateLimit, setRateLimit] = useState<RateLimit | null>(null);
  const [selected, setSelected] = useState<PullRequestSummary | null>(null);
  const [detail, setDetail] = useState<PullRequestDetail | null>(null);
  const [files, setFiles] = useState<PullRequestFile[]>([]);
  const [comments, setComments] = useState<PullRequestComment[]>([]);
  const [reviewSession, setReviewSession] = useState<ReviewDraftSession | null>(null);
  const reviewState = useSyncExternalStore(
    useCallback((listener: () => void) => reviewSession?.subscribe(listener) ?? (() => {}), [reviewSession]),
    useCallback(() => reviewSession?.snapshot() ?? null, [reviewSession]),
  );
  const drafts = reviewState?.inline ?? [];
  const commentBody = reviewState?.forms.commentBody.body ?? "";
  const reviewBody = reviewState?.forms.reviewBody.body ?? "";
  const draftBody = reviewState?.forms.inlineForm.body ?? "";
  const draftPath = reviewState?.forms.inlineForm.path ?? "";
  const draftLine = String(reviewState?.forms.inlineForm.line ?? 1);
  const reviewEvent: ReviewEvent = REVIEW_EVENTS.includes(reviewState?.forms.reviewBody.side as ReviewEvent) ? reviewState!.forms.reviewBody.side as ReviewEvent : "comment";
  const setCommentBody = (body: string) => reviewSession?.change("commentBody", { body });
  const setReviewBody = (body: string) => reviewSession?.change("reviewBody", { body });
  const setDraftBody = (body: string) => reviewSession?.change("inlineForm", { body });
  const setDraftPath = (path: string) => reviewSession?.change("inlineForm", { path });
  const setDraftLine = (line: string) => reviewSession?.change("inlineForm", { line: Math.max(1, Number.parseInt(line, 10) || 1) });
  const setReviewEvent = (side: ReviewEvent) => reviewSession?.change("reviewBody", { side });
  const [receipts, setReceipts] = useState<MergeReceipt[]>([]);
  const [protection, setProtection] = useState<ProtectionInfo | null>(null);
  const [tab, setTab] = useState<Tab>("pulls");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);

  // 登录相关
  const [deviceBusy, setDeviceBusy] = useState(false);
  const [tokenBusy, setTokenBusy] = useState(false);
  const [token, setToken] = useState("");

  // 表单
  const [mergeMethod, setMergeMethod] = useState<MergeMethod>("merge");
  const [mergeConfirmed, setMergeConfirmed] = useState(false);
  const [createTitle, setCreateTitle] = useState("");
  const [createBody, setCreateBody] = useState("");
  const [createBase, setCreateBase] = useState("main");
  const [createDraft, setCreateDraft] = useState(false);
  // F18：创建 PR 的来源远端（fork 场景可与目标远端不同）。
  const [createSourceRemote, setCreateSourceRemote] = useState<string | null>(null);
  // H4：同名本地分支有独有提交，等待用户确认覆盖。
  const [divergedBranch, setDivergedBranch] = useState(false);
  // F19：完整 PR diff（github diff 媒体类型，不裁剪）。
  const [fullDiff, setFullDiff] = useState<string | null>(null);

  const login = status?.repoAccount ?? status?.accounts[0]?.login ?? null;
  const scopeKey = JSON.stringify([worktreeId, login, remote, remotes.find((entry) => entry.name === remote)?.url]);
  const scopeRequests = useRef(new GithubRequests()).current;
  const viewRequests = useRef(new GithubRequests()).current;
  const statusRequests = useRef(new GithubRequests()).current;
  scopeRequests.select(scopeKey);
  viewRequests.select(JSON.stringify([scopeKey, selected?.number ?? null]));
  statusRequests.select(worktreeId);
  const loadStatus = useCallback(async () => {
    const ticket = statusRequests.begin("status");
    await githubResponse(ticket, () => Promise.all([m7.githubStatus(worktreeId), m7.githubRemotes(worktreeId)]),
      ([next, list]) => { setStatus(next); setRemotes(list); setRemote((current) => list.some((entry) => entry.name === current) ? current : list[0]?.name ?? null); },
      (caught) => setError(toAppError(caught)));
  }, [worktreeId, statusRequests]);

  useEffect(() => {
    setTokenBusy(false);
    void loadStatus();
    const refreshAccount = () => {
      scopeRequests.invalidate(); viewRequests.invalidate(); setSelected(null); setDetail(null);
      setBusy(null); setError(null); setNotice(null); void loadStatus();
    };
    const onFocus = () => { void loadStatus(); };
    window.addEventListener("focus", onFocus);
    window.addEventListener("revia-github-account-changed", refreshAccount);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("revia-github-account-changed", refreshAccount);
      statusRequests.invalidate(); scopeRequests.invalidate(); viewRequests.invalidate();
    };
  }, [loadStatus, statusRequests, scopeRequests, viewRequests]);

  useEffect(() => {
    const ticket = scopeRequests.begin("receipts");
    void githubResponse(ticket, () => m7.githubReceipts(worktreeId), setReceipts, (caught) => setError(toAppError(caught)));
  }, [scopeKey, worktreeId, scopeRequests]);

  const loadList = useCallback(
    async (nextPage: number) => {
      if (remote === null) return;
      const ticket = scopeRequests.begin("list");
      setBusy("list");
      setError(null);
      try {
        const result = await m7.prList(worktreeId, remote, filter, nextPage);
        // F08：只有最新一次列表请求可以落状态。
        if (!ticket.current()) return;
        setItems(result.items);
        setHasMore(result.hasMore);
        setPage(nextPage);
        setRateLimit(result.rateLimit);
      } catch (caught) {
        ticket.apply(() => setError(toAppError(caught)));
      } finally {
        ticket.apply(() => setBusy((current) => current === "list" ? null : current));
      }
    },
    [filter, remote, worktreeId, login, scopeRequests],
  );

  useEffect(() => {
    if (login !== null) void loadList(1);
  }, [login, loadList]);

  // F08：切 worktree / remote 后旧目标的选中与详情全部失效。
  useEffect(() => {
    setSelected(null);
    setDetail(null);
    setFiles([]);
    setComments([]);
    setReviewSession(null);
    setProtection(null);
    setReceipts([]);
    setFullDiff(null);
    setItems([]);
    setHasMore(false);
    setBusy(null);
    setError(null);
    setNotice(null);
    setMergeConfirmed(false);
    setDivergedBranch(false);
    setExpanded(null);
  }, [scopeKey]);

  const openDetail = useCallback(
    async (summary: PullRequestSummary) => {
      viewRequests.select(JSON.stringify([scopeKey, summary.number]));
      viewRequests.invalidate();
      const ticket = viewRequests.begin("detail");
      const request = { worktreeId, remote, number: summary.number, headSha: summary.headSha };
      setSelected(summary);
      setDetail(null);
      setFiles([]);
      setComments([]);
      setReviewSession(null);
      setProtection(null);
      setFullDiff(null);
      // 原会话保留在App级草稿登记中；新目标仅装入同身份同head的稿件。
      setBusy("detail");
      setError(null);
      try {
        const [detailResult, fileList, commentList] = await Promise.all([
          m7.prDetail(request),
          m7.prFiles(request),
          m7.prComments(request),
        ]);
        // 身份核对：只有仍然是“这一目标”的请求才能落状态；否则静默丢弃。
        if (!ticket.current()) return;
        setFiles(fileList);
        setComments(commentList);
        if (!login) return;
        const bundle = await m7.reviewDrafts({ ...request, login, headSha: detailResult.summary.headSha });
        if (!ticket.current()) return;
        setReviewSession(reviewDraftSessions.open(bundle));
        setDetail(detailResult);
        setCreateBase(summary.baseRef);
        void githubResponse(ticket, () => m7.protectionRefresh(worktreeId, remote, detailResult.summary.baseRef),
          setProtection, (caught) => {
            setProtection({ branch: detailResult.summary.baseRef, protected: true, source: "unknown", checkedAtMs: 0 });
            setError(toAppError(caught));
          });
      } catch (caught) {
        ticket.apply(() => setError(toAppError(caught)));
      } finally {
        ticket.apply(() => setBusy((current) => current === "detail" ? null : current));
      }
    },
    [remote, worktreeId, scopeKey, viewRequests, login],
  );

  // 同步完整远端集合不依赖存在或打开某个PR。
  const syncProtection = async () => {
    const ticket = viewRequests.begin("protection");
    const branch = selected?.baseRef ?? createBase;
    try {
      const info = await m7.protectionRefresh(worktreeId, remote, branch);
      ticket.apply(() => setProtection(info));
    } catch (caught) {
      ticket.apply(() => setError(toAppError(caught)));
    }
  };

  const loginWithToken = useCallback(async () => {
    if (tokenBusy || deviceBusy) return;
    const ticket = statusRequests.begin("auth");
    setError(null);
    setTokenBusy(true);
    try {
      const account = await m7.githubLoginToken(token.trim());
      if (!ticket.current()) return;
      setToken("");
      setNotice(t("github.loggedIn", { login: account.login }));
      await loadStatus();
      window.dispatchEvent(new Event("revia-github-account-changed"));
    } catch (caught) {
      ticket.apply(() => setError(toAppError(caught)));
    } finally {
      ticket.apply(() => setTokenBusy(false));
    }
  }, [loadStatus, token, tokenBusy, deviceBusy, statusRequests]);

  const submitComment = useCallback(async () => {
    if (!selected || !reviewSession || commentBody.trim().length === 0) return;
    const ticket = viewRequests.begin("action");
    const session = reviewSession;
    setBusy("comment");
    try {
      await session.flush();
      if (!ticket.current()) return;
      const sent = session.snapshot().forms.commentBody;
      await m7.prCommentCreate({ worktreeId, remote, number: selected.number, body: sent.body, draftScope: session.scope });
      await session.published("commentBody", sent);
      if (!ticket.current()) return;
      const next = await m7.prComments({ worktreeId, remote, number: selected.number });
      ticket.apply(() => { setComments(next); onWorkspaceTouched(); });
    } catch (caught) { ticket.apply(() => setError(toAppError(caught))); }
    finally { ticket.apply(() => setBusy((current) => current === "comment" ? null : current)); }
  }, [selected, reviewSession, commentBody, viewRequests, worktreeId, remote, onWorkspaceTouched]);

  const addDraft = async () => {
    if (!reviewSession || busy !== null) return;
    const ticket = viewRequests.begin("action");
    setBusy("draft");
    try { await reviewSession.addInline(); }
    catch (caught) { ticket.apply(() => setError(toAppError(caught))); }
    finally { ticket.apply(() => setBusy((current) => current === "draft" ? null : current)); }
  };
  const deleteDraft = async (id: string) => {
    if (!reviewSession || busy !== null) return;
    const ticket = viewRequests.begin("action"); setBusy("draft");
    try { await reviewSession.removeInline(id); }
    catch (caught) { ticket.apply(() => setError(toAppError(caught))); }
    finally { ticket.apply(() => setBusy((current) => current === "draft" ? null : current)); }
  };

  const submitReview = useCallback(async () => {
    if (!selected || !detail || !reviewSession) return;
    if (drafts.length === 0 && reviewBody.trim().length === 0) {
      setError({ kind: "rejected", message: t("github.reviewEmpty"), params: {} }); return;
    }
    const ticket = viewRequests.begin("action");
    const session = reviewSession;
    setBusy("review"); setError(null);
    try {
      await session.flush();
      if (!ticket.current()) return;
      const sent = session.snapshot();
      const receipt = await m7.reviewSubmit({ worktreeId, remote, number: selected.number,
        event: reviewEvent, body: sent.forms.reviewBody.body,
        comments: sent.inline.map((draft) => ({ path: draft.path, line: draft.line, side: draft.side, body: draft.body })),
        expectedHeadSha: session.scope.headSha, draftScope: session.scope, draftIds: sent.inline.map((draft) => draft.id),
      });
      await session.published("reviewBody", sent.forms.reviewBody, sent.inline);
      if (!ticket.current()) return;
      const nextReceipts = await m7.githubReceipts(worktreeId);
      ticket.apply(() => { setReceipts(nextReceipts); setNotice(t("github.reviewSubmitted", { url: receipt.reviewUrl ?? "" })); onWorkspaceTouched(); });
    } catch (caught) { ticket.apply(() => setError(toAppError(caught))); }
    finally { ticket.apply(() => setBusy((current) => current === "review" ? null : current)); }
  }, [detail, drafts, reviewBody, reviewEvent, selected, reviewSession, viewRequests, remote, worktreeId, onWorkspaceTouched]);

  const checkout = useCallback(async (overwriteConfirmed = false) => {
    if (!selected || !detail || remote === null) return;
    setBusy("checkout");
    setError(null);
    const ticket = viewRequests.begin("action");
    try {
      const outcome = await onCheckout({
        worktreeId,
        remote,
        number: selected.number,
        expectedHeadSha: detail.summary.headSha,
        strategy: "smart",
        localBranch: null,
        overwriteConfirmed,
      });
      if (!ticket.current()) return;
      setNotice(t("github.checkoutDone", { branch: outcome.branch }));
      onCheckedOut(outcome);
    } catch (caught) {
      if (!ticket.current()) return;
      const failure = toAppError(caught);
      // H4：分支分叉时给用户一次明确确认覆盖的机会。
      if (failure.code === "errors.m7.githubPrBranchDiverged" && !overwriteConfirmed) {
        setDivergedBranch(true);
      }
      setError(failure);
    } finally {
      ticket.apply(() => setBusy((current) => current === "checkout" ? null : current));
    }
  }, [detail, onCheckedOut, onCheckout, remote, selected, worktreeId, viewRequests]);

  const merge = useCallback(async () => {
    if (!selected || !detail) return;
    setBusy("merge");
    setError(null);
    const ticket = viewRequests.begin("action");
    try {
      // F07：合并落在列表 / 详情所选的远端仓库，不回退 origin。
      if (remote === null) {
        setError({ kind: "rejected", message: t("github.remoteRequired"), params: {} });
        return;
      }
      const outcome = await m7.prMerge({
        worktreeId,
        remote,
        number: selected.number,
        method: mergeMethod,
        expectedHeadSha: detail.summary.headSha,
        commitTitle: null,
        commitMessage: null,
        confirmed: mergeConfirmed,
      });
      if (!ticket.current()) return;
      if (outcome.status === "merged") {
        setNotice(t("github.mergeDone", { sha: outcome.sha ?? "" }));
      } else if (outcome.error) {
        // H7：结构化错误走生产 formatter（双语），不嵌中文诊断句。
        setError(outcome.error);
      } else {
        setNotice(t("github.mergeNotDone", { status: outcome.status, message: outcome.message }));
      }
      if (!ticket.current()) return;
      const nextReceipts = await m7.githubReceipts(worktreeId);
      if (!ticket.current()) return;
      setReceipts(nextReceipts);
      setMergeConfirmed(false);
      onWorkspaceTouched();
    } catch (caught) {
      if (!ticket.current()) return;
      setError(toAppError(caught));
    } finally {
      ticket.apply(() => setBusy((current) => current === "merge" ? null : current));
    }
  }, [detail, mergeConfirmed, mergeMethod, onWorkspaceTouched, selected, worktreeId, viewRequests, remote]);

  const createPr = useCallback(async () => {
    if (remote === null) return;
    setBusy("create");
    setError(null);
    const ticket = scopeRequests.begin("action");
    try {
      // F18：head 按“来源远端 owner:branch”（同仓库省略 owner）；支持已有 fork。
      const source = remotes.find((entry) => entry.name === createSourceRemote);
      const ownerPrefix = source?.owner && createSourceRemote !== remote ? `${source.owner}:` : "";
      const created = await m7.prCreate({
        worktreeId,
        remote,
        head: `${ownerPrefix}${currentBranch ?? ""}`,
        base: createBase.trim(),
        title: createTitle.trim(),
        body: createBody,
        draft: createDraft,
      });
      if (!ticket.current()) return;
      setNotice(t("github.created", { number: created.number }));
      setTab("pulls");
      await loadList(1);
    } catch (caught) {
      if (!ticket.current()) return;
      setError(toAppError(caught));
    } finally {
      ticket.apply(() => setBusy((current) => current === "create" ? null : current));
    }
  }, [createBase, createBody, createDraft, createSourceRemote, createTitle, currentBranch, loadList, remote, remotes, worktreeId, scopeRequests]);

  const needConfirm = useMemo(() => {
    if (!detail) return false;
    return !protection || protection.source !== "api" || protection.protected || detail.mergeableState !== "clean";
  }, [detail, protection]);

  return (
    <div className="github-panel absolute inset-0 z-20 flex flex-col bg-[var(--editor-background)]">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-panel-border px-3 text-[12.5px]">
        <Codicon name="github" size={15} />
        <span className="shrink-0 font-semibold whitespace-nowrap">{t("github.title")}</span>
        <span className="min-w-0 truncate text-descriptionforeground" title={worktreePath}>{worktreePath}</span>
        {login ? <span className="shrink-0 text-descriptionforeground">{t("github.account", { login })}</span> : null}
        {rateLimit && rateLimit.remaining !== null ? (
          <span className="shrink-0 text-descriptionforeground">{t("github.rateLimit", { remaining: rateLimit.remaining })}</span>
        ) : null}
        <div className="ml-auto flex items-center gap-1">
          <button type="button" className={`toolbar-widget h-6 px-2 ${tab === "pulls" ? "is-active" : ""}`} onClick={() => setTab("pulls")}>
            {t("github.tabPulls")}
          </button>
          <button type="button" className={`toolbar-widget h-6 px-2 ${tab === "create" ? "is-active" : ""}`} onClick={() => setTab("create")}>
            {t("github.tabCreate")}
          </button>
          <button type="button" className={`toolbar-widget h-6 px-2 ${tab === "receipts" ? "is-active" : ""}`} onClick={() => setTab("receipts")}>
            {t("github.tabReceipts")}
          </button>
          <button type="button" className="icon-action size-[22px]" aria-label={t("common.close")} onClick={onClose}>
            <Codicon name="close" size={14} />
          </button>
        </div>
      </div>

      {error ? (
        <div role="alert" className="flex items-start gap-2 border-b border-panel-border bg-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_10%,var(--editor-background))] px-3 py-2 text-[12px]">
          <span className="min-w-0 flex-1 whitespace-pre-wrap">{formatAppError(error)}</span>
          <button type="button" className="icon-action size-5" aria-label={t("common.dismiss")} onClick={() => setError(null)}>
            <Codicon name="close" size={12} />
          </button>
        </div>
      ) : null}
      {notice ? (
        <div className="flex items-start gap-2 border-b border-panel-border bg-[var(--side-bar-background)] px-3 py-2 text-[12px]">
          <span className="min-w-0 flex-1 whitespace-pre-wrap">{notice}</span>
          <button type="button" className="icon-action size-5" aria-label={t("common.dismiss")} onClick={() => setNotice(null)}>
            <Codicon name="close" size={12} />
          </button>
        </div>
      ) : null}

      {login && remote ? (
        <div className="flex shrink-0 items-center gap-2 border-b border-panel-border px-3 py-1 text-[12px]">
          <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void syncProtection()}>
            {t("github.syncProtection")}
          </button>
          <span className="text-descriptionforeground">{protectionSourceText(protection?.source ?? "unknown")}</span>
        </div>
      ) : null}

      {status && status.accounts.length === 0 ? (
        <div className="mx-auto mt-10 w-[520px] space-y-3 text-[12.5px]">
          <h2 className="text-[14px] font-semibold">{t("github.loginTitle")}</h2>
          <p className="text-descriptionforeground">{t("github.loginHint")}</p>
          {status.deviceFlowConfigured ? (
            <GithubDeviceLogin
              key={worktreeId}
              disabled={tokenBusy}
              onBusyChange={setDeviceBusy}
              onAuthorized={(account) => {
                setNotice(t("github.loggedIn", { login: account.login }));
                void loadStatus();
              }}
            />
          ) : (
            <p className="text-descriptionforeground">{t("github.deviceUnconfigured")}</p>
          )}
          <div className="space-y-2 rounded border border-panel-border p-3">
            <p className="font-semibold">{t("github.tokenTitle")}</p>
            <input
              className="settings-input w-full"
              type="password"
              value={token}
              spellCheck={false}
              placeholder={t("github.tokenPlaceholder")}
              onChange={(event) => setToken(event.target.value)}
            />
            <button type="button" className="toolbar-widget h-7 px-3" disabled={tokenBusy || deviceBusy || token.trim().length === 0} onClick={() => void loginWithToken()}>
              {t("github.tokenLogin")}
            </button>
          </div>
        </div>
      ) : tab === "pulls" ? (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(280px,34%)_1fr] divide-x divide-panel-border">
          <div className="flex min-h-0 flex-col">
            <div className="flex h-8 shrink-0 items-center gap-1 border-b border-panel-border px-2 text-[12px]">
              <select
                className="settings-select h-6 w-auto min-w-0 flex-1"
                value={remote ?? ""}
                onChange={(event) => {
                  setRemote(event.target.value || null);
                  setSelected(null);
                }}
              >
                {remotes.map((entry) => (
                  <option key={entry.name} value={entry.name}>
                    {entry.name} · {entry.owner ?? "?"}/{entry.repo ?? "?"}
                  </option>
                ))}
              </select>
              <select className="settings-select h-6 w-auto" value={filter} onChange={(event) => setFilter(event.target.value as PullRequestFilter)}>
                <option value="open">{t("github.filterOpen")}</option>
                <option value="closed">{t("github.filterClosed")}</option>
                <option value="all">{t("github.filterAll")}</option>
              </select>
              <button type="button" className="icon-action size-[22px]" aria-label={t("common.refresh")} onClick={() => void loadList(1)}>
                <Codicon name="refresh" size={13} />
              </button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {busy === "list" && items.length === 0 ? (
                <p className="p-3 text-[12px] text-descriptionforeground">{t("common.loading")}</p>
              ) : items.length === 0 ? (
                <p className="p-3 text-[12px] text-descriptionforeground">{t("github.empty")}</p>
              ) : (
                items.map((item) => (
                  <button
                    key={item.number}
                    type="button"
                    onClick={() => void openDetail(item)}
                    className={`block w-full border-b border-panel-border px-2 py-1.5 text-left text-[12px] ${
                      selected?.number === item.number ? "bg-[var(--list-activeSelectionBackground)] text-[var(--list-activeSelectionForeground)]" : "hover:bg-[var(--list-hoverBackground)]"
                    }`}
                  >
                    <div className="flex items-center gap-1">
                      <span className="shrink-0 text-descriptionforeground">#{item.number}</span>
                      <span className="truncate font-semibold">{item.title}</span>
                      {item.draft ? <span className="shrink-0 text-descriptionforeground">{t("github.draftBadge")}</span> : null}
                    </div>
                    <div className="mt-0.5 flex items-center gap-2 text-[11px] text-descriptionforeground">
                      <span className="truncate">{item.author}</span>
                      <span>{item.headRef} → {item.baseRef}</span>
                      <span className="ml-auto shrink-0">+{item.additions} −{item.deletions}</span>
                    </div>
                  </button>
                ))
              )}
            </div>
            <div className="flex h-8 shrink-0 items-center justify-between border-t border-panel-border px-2 text-[11.5px]">
              <button type="button" className="toolbar-widget h-6 px-2" disabled={page <= 1 || busy !== null} onClick={() => void loadList(page - 1)}>
                {t("github.prevPage")}
              </button>
              <span className="text-descriptionforeground">{t("github.page", { page })}</span>
              <button type="button" className="toolbar-widget h-6 px-2" disabled={!hasMore || busy !== null} onClick={() => void loadList(page + 1)}>
                {t("github.nextPage")}
              </button>
            </div>
          </div>

          <div className="min-h-0 overflow-y-auto">
            {selected === null ? (
              <p className="p-4 text-[12px] text-descriptionforeground">{t("github.pickOne")}</p>
            ) : detail === null ? (
              <p className="p-4 text-[12px] text-descriptionforeground">{t("common.loading")}</p>
            ) : (
              <div className="space-y-4 p-3 text-[12px]">
                <header className="space-y-1">
                  <h2 className="text-[14px] font-semibold">
                    #{detail.summary.number} {detail.summary.title}
                  </h2>
                  <p className="text-descriptionforeground">
                    {t("github.detailMeta", {
                      author: detail.summary.author,
                      state: detail.summary.state,
                      commits: detail.commits,
                      files: detail.summary.changedFiles,
                    })}
                  </p>
                  <p className="text-descriptionforeground">
                    {detail.summary.headRef} → {detail.summary.baseRef} · {detail.summary.headSha.slice(0, 8)}
                  </p>
                  <div className="flex flex-wrap items-center gap-2 pt-1">
                    {protection ? (
                      <span className={`rounded px-1.5 py-0.5 ${protection.protected ? "bg-[color-mix(in_srgb,var(--gitdecoration-modifiedresourceforeground)_18%,transparent)]" : "bg-[var(--side-bar-background)]"}`}>
                        {protection.source === "unknown" ? protectionSourceText(protection.source) : <>
                          {protection.protected ? t("github.protected", { branch: protection.branch }) : t("github.notProtected", { branch: protection.branch })}
                          {protection.source === "stale" ? <span className="ml-1">{protectionSourceText(protection.source)}</span> : null}
                        </>}
                      </span>
                    ) : null}
                    <span className="text-descriptionforeground">
                      {t("github.mergeable", { state: detail.mergeableState, checks: detail.checksState || t("github.checksNone") })}
                    </span>
                    <button type="button" className="toolbar-widget h-6 px-2" disabled={busy !== null} onClick={() => void checkout()}>
                      {t("github.checkout")}
                    </button>
                  </div>
                </header>

                {detail.body.trim().length > 0 ? (
                  <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded border border-panel-border bg-[var(--side-bar-background)] p-2">{detail.body}</pre>
                ) : null}
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    className="toolbar-widget h-6 px-2"
                    disabled={busy !== null}
                    onClick={() => {
                      // F19：完整 diff 走 GitHub diff 媒体类型，不受 files 补丁裁剪影响。
                      void githubResponse(viewRequests.begin("diff"), () => m7.prDiff({ worktreeId, remote, number: selected.number }), setFullDiff, (caught) => setError(toAppError(caught)));
                    }}
                  >
                    {t("github.viewFullDiff")}
                  </button>
                </div>
                {fullDiff !== null ? (
                  <pre className="max-h-96 overflow-auto rounded border border-panel-border bg-[var(--editor-background)] p-2 font-mono text-[11.5px]">{fullDiff}</pre>
                ) : null}

                <section className="space-y-1">
                  <h3 className="font-semibold">{t("github.files", { count: files.length })}</h3>
                  {files.map((file) => (
                    <div key={file.filename} className="rounded border border-panel-border">
                      <button
                        type="button"
                        className="flex w-full items-center gap-2 px-2 py-1 text-left"
                        onClick={() => setExpanded(expanded === file.filename ? null : file.filename)}
                      >
                        <Codicon name={expanded === file.filename ? "chevron-down" : "chevron-right"} size={12} />
                        <span className="truncate font-mono text-[11.5px]">{file.filename}</span>
                        <span className="ml-auto shrink-0 text-descriptionforeground">+{file.additions} −{file.deletions}</span>
                      </button>
                      {expanded === file.filename ? (
                        <div className="border-t border-panel-border">
                          {file.patch ? (
                            <pre className="max-h-72 overflow-auto bg-[var(--editor-background)] p-2 font-mono text-[11.5px]">
                              {patchLines(file.patch).map((line, index) => (
                                <div
                                  key={index}
                                  className={
                                    line.startsWith("+") ? "bg-[var(--diff-ins-bg)]" : line.startsWith("-") ? "bg-[var(--diff-del-bg)]" : ""
                                  }
                                >
                                  {line}
                                </div>
                              ))}
                            </pre>
                          ) : (
                            <p className="p-2 text-descriptionforeground">{t("github.patchTooLarge")}</p>
                          )}
                          <button
                            type="button"
                            className="toolbar-widget m-2 h-6 px-2"
                            onClick={() => {
                              setDraftPath(file.filename);
                              setDraftLine("1");
                            }}
                          >
                            {t("github.draftOnFile")}
                          </button>
                        </div>
                      ) : null}
                    </div>
                  ))}
                </section>

                <section className="space-y-2">
                  <h3 className="font-semibold">{t("github.comments", { count: comments.length })}</h3>
                  {comments.length === 0 ? <p className="text-descriptionforeground">{t("github.noComments")}</p> : null}
                  {comments.map((comment) => (
                    <div key={comment.id} className="rounded border border-panel-border p-2">
                      <p className="text-[11.5px] text-descriptionforeground">
                        {comment.author} · {comment.createdAt}
                        {comment.path ? ` · ${comment.path}${comment.line ? `:${comment.line}` : ""}` : ""}
                      </p>
                      <pre className="whitespace-pre-wrap">{comment.body}</pre>
                    </div>
                  ))}
                  <textarea
                    className="settings-input h-16 w-full resize-y py-1"
                    value={commentBody}
                    placeholder={t("github.commentPlaceholder")}
                    onChange={(event) => setCommentBody(event.target.value)}
                  />
                  <button type="button" className="toolbar-widget h-6 px-2" disabled={busy !== null || !reviewSession || error?.kind === "needsVerification" || commentBody.trim().length === 0} onClick={() => void submitComment()}>
                    {t("github.commentSend")}
                  </button>
                </section>

                <section className="space-y-2 rounded border border-panel-border p-2">
                  <h3 className="font-semibold">{t("github.reviewTitle")}</h3>
                  <p className="text-descriptionforeground">{t("github.reviewHint")}</p>
                  {reviewState?.error ? <p role="alert">{formatAppError(toAppError(reviewState.error))}</p> : null}
                  {reviewState && reviewState.older.length > 0 ? <details className="rounded border border-panel-border p-2">
                    <summary>{t("github.olderDrafts")}</summary>
                    {reviewState.older.filter((draft) => draft.body.length > 0).map((draft) => <div key={draft.id} className="mt-2">
                      <p className="font-mono text-[11px]">{draft.headSha?.slice(0, 8)} {draft.path}</p>
                      <pre className="select-text whitespace-pre-wrap">{draft.body}</pre>
                    </div>)}
                  </details> : null}
                  {drafts.length === 0 ? <p className="text-descriptionforeground">{t("github.noDrafts")}</p> : null}
                  {drafts.map((draft) => (
                    <div key={draft.id} className="flex items-start gap-2 rounded bg-[var(--side-bar-background)] p-2">
                      <div className="min-w-0 flex-1">
                        <p className="font-mono text-[11.5px] text-descriptionforeground">{draft.path}:{draft.line}</p>
                        <pre className="whitespace-pre-wrap">{draft.body}</pre>
                      </div>
                      <button
                        type="button"
                        className="icon-action size-5"
                        aria-label={t("common.delete")}
                        disabled={busy !== null}
                        onClick={() => void deleteDraft(draft.id)}
                      >
                        <Codicon name="trash" size={12} />
                      </button>
                    </div>
                  ))}
                  <div className="grid grid-cols-[1fr_80px] gap-2">
                    <input className="settings-input" value={draftPath} placeholder={t("github.draftPath")} onChange={(event) => setDraftPath(event.target.value)} />
                    <input className="settings-input" value={draftLine} inputMode="numeric" placeholder={t("github.draftLine")} onChange={(event) => setDraftLine(event.target.value)} />
                  </div>
                  <textarea className="settings-input h-14 w-full resize-y py-1" value={draftBody} placeholder={t("github.draftBody")} onChange={(event) => setDraftBody(event.target.value)} />
                  <button type="button" className="toolbar-widget h-6 px-2" disabled={busy !== null || !reviewSession} onClick={() => void addDraft()}>
                    {t("github.draftAdd")}
                  </button>
                  <div className="flex flex-wrap items-center gap-2 border-t border-panel-border pt-2">
                    <select className="settings-select h-6 w-auto" value={reviewEvent} onChange={(event) => setReviewEvent(event.target.value as ReviewEvent)}>
                      {REVIEW_EVENTS.map((event) => (
                        <option key={event} value={event}>
                          {reviewEventLabel(event)}
                        </option>
                      ))}
                    </select>
                    <input className="settings-input h-6 min-w-0 flex-1" value={reviewBody} placeholder={t("github.reviewBody")} onChange={(event) => setReviewBody(event.target.value)} />
                    <button type="button" className="toolbar-widget h-6 px-2" disabled={busy !== null || !reviewSession || error?.kind === "needsVerification"} onClick={() => void submitReview()}>
                      {t("github.reviewSubmit")}
                    </button>
                  </div>
                </section>

                <section className="space-y-2 rounded border border-panel-border p-2">
                  <h3 className="font-semibold">{t("github.mergeTitle")}</h3>
                  <div className="flex flex-wrap items-center gap-2">
                    <select className="settings-select h-6 w-auto" value={mergeMethod} onChange={(event) => setMergeMethod(event.target.value as MergeMethod)}>
                      {MERGE_METHODS.map((method) => (
                        <option key={method} value={method}>
                          {mergeMethodLabel(method)}
                        </option>
                      ))}
                    </select>
                    <label className="flex items-center gap-1.5">
                      <input type="checkbox" className="accent-[var(--focusborder)]" checked={mergeConfirmed} onChange={(event) => setMergeConfirmed(event.target.checked)} />
                      {t("github.mergeConfirm")}
                    </label>
                    <button
                      type="button"
                      className="toolbar-widget h-6 px-2 font-semibold"
                      disabled={busy !== null || (needConfirm && !mergeConfirmed)}
                      onClick={() => void merge()}
                    >
                      {t("github.mergeSubmit")}
                    </button>
                  </div>
                  {needConfirm ? <p className="text-descriptionforeground">{t("github.mergeNeedConfirm")}</p> : null}
                </section>
              </div>
            )}
          </div>
        </div>
      ) : tab === "create" ? (
        <div className="mx-auto w-[560px] space-y-3 p-4 text-[12.5px]">
          <h2 className="text-[14px] font-semibold">{t("github.createTitle")}</h2>
          <label className="block">
            <span className="mb-1 block font-semibold">{t("github.createBase")}</span>
            <input className="settings-input" value={createBase} onChange={(event) => setCreateBase(event.target.value)} />
          </label>
          <label className="block">
            <span className="mb-1 block font-semibold">{t("github.createTitleField")}</span>
            <input className="settings-input" value={createTitle} onChange={(event) => setCreateTitle(event.target.value)} />
          </label>
          <label className="block">
            <span className="mb-1 block font-semibold">{t("github.createBody")}</span>
            <textarea className="settings-input h-32 w-full resize-y py-1" value={createBody} onChange={(event) => setCreateBody(event.target.value)} />
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" className="accent-[var(--focusborder)]" checked={createDraft} onChange={(event) => setCreateDraft(event.target.checked)} />
            {t("github.createDraft")}
          </label>
          <label className="block">
            <span className="mb-1 block font-semibold">{t("github.createSource")}</span>
            <select
              className="settings-select w-full"
              value={createSourceRemote ?? remote ?? ""}
              onChange={(event) => setCreateSourceRemote(event.target.value || null)}
            >
              {remotes.map((entry) => (
                <option key={entry.name} value={entry.name}>
                  {entry.owner ? `${entry.name} (${entry.owner})` : entry.name}
                </option>
              ))}
            </select>
          </label>
          <p className="text-descriptionforeground">{t("github.createHead", { head: currentBranch ?? "?" })}</p>
          <button
            type="button"
            className="toolbar-widget h-7 px-3 font-semibold"
            disabled={busy !== null || createTitle.trim().length === 0 || !currentBranch}
            onClick={() => void createPr()}
          >
            {t("github.createSubmit")}
          </button>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto p-3 text-[12px]">
          <h2 className="mb-2 text-[14px] font-semibold">{t("github.receiptsTitle")}</h2>
          {receipts.length === 0 ? <p className="text-descriptionforeground">{t("github.noReceipts")}</p> : null}
          <div className="space-y-1">
            {receipts.map((receipt) => (
              <div key={receipt.id} className="rounded border border-panel-border px-2 py-1">
                <p>
                  #{receipt.number} · {receipt.action} · {receipt.resultSha?.slice(0, 8) ?? "—"}
                  {receipt.protectedBranch ? ` · ${t("github.protectedShort")}` : ""}
                </p>
                <p className="text-descriptionforeground">
                  {new Date(receipt.createdAtMs).toLocaleString()} · {protectionSourceText(receipt.protectionSource)}
                  {receipt.notes.length > 0 ? ` · ${receipt.notes.join("; ")}` : ""}
                </p>
              </div>
            ))}
          </div>
        </div>
      )}
      {divergedBranch ? (
        // H4：覆盖本地分支的独有提交前必须由用户确认。
        <div className="absolute inset-0 z-30 flex items-center justify-center bg-black/30">
          <div role="dialog" aria-modal="true" className="w-[420px] rounded-md border border-panel-border bg-[var(--editor-background)] p-4 text-[12.5px]">
            <p className="mb-3">{t("github.divergedConfirm", { branch: `pr/${selected?.number ?? ""}` })}</p>
            <div className="flex justify-end gap-2">
              <button type="button" className="toolbar-widget h-7 px-3" onClick={() => setDivergedBranch(false)}>
                {t("common.cancel")}
              </button>
              <button
                type="button"
                className="toolbar-widget h-7 px-3 font-semibold"
                onClick={() => {
                  setDivergedBranch(false);
                  void checkout(true);
                }}
              >
                {t("github.divergedOverwrite")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
