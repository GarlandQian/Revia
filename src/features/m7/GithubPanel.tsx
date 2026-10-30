/**
 * GitHub 面板（M7 任务 07、08、09、Q180 / Q181 / Q184）。
 *
 * 一个面板里完成：登录、PR 列表、详情与 diff、评论、本地评审草稿、
 * 检出、创建 PR、合并（含受保护分支确认）与回执。
 * 所有网络与 Git 动作都由 Rust 执行；这里只收集用户输入与展示结构化结果。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { DeviceStart } from "@/types/bindings/DeviceStart";
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
import type { ReviewDraft } from "@/types/bindings/ReviewDraft";
import type { ReviewEvent } from "@/types/bindings/ReviewEvent";
import { Codicon } from "@/components/ui/codicon";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7 } from "@/lib/m7/api";

type Tab = "pulls" | "create" | "receipts";

export interface GithubPanelProps {
  worktreeId: string;
  worktreePath: string;
  currentBranch: string | null;
  onClose: () => void;
  /** 检出成功后让工作区刷新。 */
  onCheckedOut: (outcome: PrCheckoutOutcome) => void;
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

function patchLines(patch: string) {
  return patch.split("\n").slice(0, 400);
}

export function GithubPanel({
  worktreeId,
  worktreePath,
  currentBranch,
  onClose,
  onCheckedOut,
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
  const [drafts, setDrafts] = useState<ReviewDraft[]>([]);
  const [receipts, setReceipts] = useState<MergeReceipt[]>([]);
  const [protection, setProtection] = useState<ProtectionInfo | null>(null);
  const [tab, setTab] = useState<Tab>("pulls");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);

  // 登录相关
  const [device, setDevice] = useState<DeviceStart | null>(null);
  const [token, setToken] = useState("");
  const pollTimer = useRef<number | null>(null);

  // 表单
  const [commentBody, setCommentBody] = useState("");
  const [reviewBody, setReviewBody] = useState("");
  const [reviewEvent, setReviewEvent] = useState<ReviewEvent>("comment");
  const [draftPath, setDraftPath] = useState("");
  const [draftLine, setDraftLine] = useState("1");
  const [draftBody, setDraftBody] = useState("");
  const [mergeMethod, setMergeMethod] = useState<MergeMethod>("merge");
  const [mergeConfirmed, setMergeConfirmed] = useState(false);
  const [createTitle, setCreateTitle] = useState("");
  const [createBody, setCreateBody] = useState("");
  const [createBase, setCreateBase] = useState("main");
  const [createDraft, setCreateDraft] = useState(false);

  const login = status?.repoAccount ?? status?.accounts[0]?.login ?? null;

  const loadStatus = useCallback(async () => {
    try {
      const next = await m7.githubStatus(worktreeId);
      setStatus(next);
      const list = await m7.githubRemotes(worktreeId).catch(() => [] as RemoteInfo[]);
      setRemotes(list);
      setRemote((current) => current ?? list[0]?.name ?? null);
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [worktreeId]);

  useEffect(() => {
    void loadStatus();
    void m7.githubReceipts(worktreeId).then(setReceipts).catch(() => undefined);
  }, [loadStatus, worktreeId]);

  const loadList = useCallback(
    async (nextPage: number) => {
      if (remote === null) return;
      setBusy("list");
      setError(null);
      try {
        const result = await m7.prList(worktreeId, remote, filter, nextPage);
        setItems(result.items);
        setHasMore(result.hasMore);
        setPage(nextPage);
        setRateLimit(result.rateLimit);
      } catch (caught) {
        setError(toAppError(caught));
      } finally {
        setBusy(null);
      }
    },
    [filter, remote, worktreeId],
  );

  useEffect(() => {
    if (login !== null) void loadList(1);
  }, [login, loadList]);

  const openDetail = useCallback(
    async (summary: PullRequestSummary) => {
      setSelected(summary);
      setDetail(null);
      setFiles([]);
      setComments([]);
      setDrafts([]);
      setProtection(null);
      setBusy("detail");
      setError(null);
      const request = { worktreeId, remote, number: summary.number };
      try {
        const [detailResult, fileList, commentList, draftList] = await Promise.all([
          m7.prDetail(request),
          m7.prFiles(request),
          m7.prComments(request),
          m7.reviewDrafts(request),
        ]);
        setDetail(detailResult);
        setFiles(fileList);
        setComments(commentList);
        setDrafts(draftList);
        setCreateBase(summary.baseRef);
        void m7
          .protectionRefresh(worktreeId, remote, summary.baseRef)
          .then(setProtection)
          .catch(() => undefined);
      } catch (caught) {
        setError(toAppError(caught));
      } finally {
        setBusy(null);
      }
    },
    [remote, worktreeId],
  );

  const startDeviceFlow = useCallback(async () => {
    setError(null);
    try {
      const started = await m7.githubDeviceStart();
      setDevice(started);
      const deadline = Date.now() + started.expiresInMs;
      const poll = async () => {
        try {
          const result = await m7.githubDevicePoll(started.flowId);
          if (result.kind === "authorized") {
            setDevice(null);
            setNotice(t("github.loggedIn", { login: result.account.login }));
            await loadStatus();
            return;
          }
          if (result.kind === "expired" || result.kind === "denied") {
            setDevice(null);
            setError({ kind: "rejected", message: t(result.kind === "expired" ? "github.deviceExpired" : "github.deviceDenied"), params: {} });
            return;
          }
          if (Date.now() > deadline) {
            setDevice(null);
            return;
          }
          pollTimer.current = window.setTimeout(() => void poll(), Math.max(result.intervalMs, 1000));
        } catch (caught) {
          setError(toAppError(caught));
          setDevice(null);
        }
      };
      pollTimer.current = window.setTimeout(() => void poll(), Math.max(started.intervalMs, 1000));
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [loadStatus]);

  useEffect(() => {
    return () => {
      if (pollTimer.current !== null) window.clearTimeout(pollTimer.current);
    };
  }, []);

  const loginWithToken = useCallback(async () => {
    setError(null);
    try {
      const account = await m7.githubLoginToken(token.trim());
      setToken("");
      setNotice(t("github.loggedIn", { login: account.login }));
      await loadStatus();
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [loadStatus, token]);

  const submitComment = useCallback(async () => {
    if (!selected || commentBody.trim().length === 0) return;
    setBusy("comment");
    try {
      await m7.prCommentCreate({ worktreeId, remote, number: selected.number, body: commentBody });
      setCommentBody("");
      setComments(await m7.prComments({ worktreeId, remote, number: selected.number }));
      onWorkspaceTouched();
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(null);
    }
  }, [commentBody, onWorkspaceTouched, remote, selected, worktreeId]);

  const addDraft = useCallback(async () => {
    if (!selected || !login || draftPath.trim().length === 0) return;
    const line = Math.max(1, Number.parseInt(draftLine, 10) || 1);
    try {
      await m7.reviewDraftSave({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        repoKey: `${login}/${selected.headRef}`,
        number: selected.number,
        path: draftPath.trim(),
        side: "new",
        line,
        body: draftBody,
        createdAtMs: Date.now(),
        updatedAtMs: Date.now(),
      });
      setDraftBody("");
      setDrafts(await m7.reviewDrafts({ worktreeId, remote, number: selected.number }));
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [draftBody, draftLine, draftPath, login, remote, selected, worktreeId]);

  const submitReview = useCallback(async () => {
    if (!selected || !detail) return;
    if (drafts.length === 0 && reviewBody.trim().length === 0) {
      setError({ kind: "rejected", message: t("github.reviewEmpty"), params: {} });
      return;
    }
    setBusy("review");
    setError(null);
    try {
      const receipt = await m7.reviewSubmit({
        worktreeId,
        number: selected.number,
        event: reviewEvent,
        body: reviewBody,
        comments: drafts.map((draft) => ({ path: draft.path, line: draft.line, side: draft.side, body: draft.body })),
        expectedHeadSha: detail.summary.headSha,
      });
      setReceipts(await m7.githubReceipts(worktreeId));
      setDrafts([]);
      setReviewBody("");
      setNotice(t("github.reviewSubmitted", { url: receipt.reviewUrl ?? "" }));
      onWorkspaceTouched();
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(null);
    }
  }, [detail, drafts, onWorkspaceTouched, reviewBody, reviewEvent, selected, worktreeId]);

  const checkout = useCallback(async () => {
    if (!selected || !detail || remote === null) return;
    setBusy("checkout");
    setError(null);
    try {
      const outcome = await m7.prCheckout({
        worktreeId,
        remote,
        number: selected.number,
        expectedHeadSha: detail.summary.headSha,
        strategy: "smart",
        localBranch: null,
      });
      setNotice(t("github.checkoutDone", { branch: outcome.branch }));
      onCheckedOut(outcome);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(null);
    }
  }, [detail, onCheckedOut, remote, selected, worktreeId]);

  const merge = useCallback(async () => {
    if (!selected || !detail) return;
    setBusy("merge");
    setError(null);
    try {
      const outcome = await m7.prMerge({
        worktreeId,
        number: selected.number,
        method: mergeMethod,
        expectedHeadSha: detail.summary.headSha,
        commitTitle: null,
        commitMessage: null,
        confirmed: mergeConfirmed,
      });
      setNotice(
        outcome.status === "merged"
          ? t("github.mergeDone", { sha: outcome.sha ?? "" })
          : t("github.mergeNotDone", { status: outcome.status, message: outcome.message }),
      );
      setReceipts(await m7.githubReceipts(worktreeId));
      setMergeConfirmed(false);
      onWorkspaceTouched();
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(null);
    }
  }, [detail, mergeConfirmed, mergeMethod, onWorkspaceTouched, selected, worktreeId]);

  const createPr = useCallback(async () => {
    if (remote === null) return;
    setBusy("create");
    setError(null);
    try {
      const created = await m7.prCreate({
        worktreeId,
        remote,
        head: currentBranch ?? "",
        base: createBase.trim(),
        title: createTitle.trim(),
        body: createBody,
        draft: createDraft,
      });
      setNotice(t("github.created", { number: created.number }));
      setTab("pulls");
      await loadList(1);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(null);
    }
  }, [createBase, createBody, createDraft, createTitle, currentBranch, loadList, remote, worktreeId]);

  const needConfirm = useMemo(() => {
    if (!detail) return false;
    return protection?.protected === true || detail.mergeableState !== "clean";
  }, [detail, protection]);

  return (
    <div className="absolute inset-0 z-20 flex flex-col bg-[var(--editor-background)]">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-panel-border px-3 text-[12.5px]">
        <Codicon name="github" size={15} />
        <span className="font-semibold">{t("github.title")}</span>
        <span className="truncate text-descriptionforeground" title={worktreePath}>{worktreePath}</span>
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

      {status && status.accounts.length === 0 ? (
        <div className="mx-auto mt-10 w-[520px] space-y-3 text-[12.5px]">
          <h2 className="text-[14px] font-semibold">{t("github.loginTitle")}</h2>
          <p className="text-descriptionforeground">{t("github.loginHint")}</p>
          {status.deviceFlowConfigured ? (
            <div className="space-y-2 rounded border border-panel-border p-3">
              <p className="font-semibold">{t("github.deviceTitle")}</p>
              {device ? (
                <div className="space-y-1">
                  <p>{t("github.deviceCode")}</p>
                  <p className="select-all font-mono text-[16px] tracking-widest">{device.userCode}</p>
                  <p className="text-descriptionforeground">{device.verificationUri}</p>
                  <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void navigator.clipboard.writeText(device.userCode)}>
                    {t("github.deviceCopy")}
                  </button>
                </div>
              ) : (
                <button type="button" className="toolbar-widget h-7 px-3" onClick={() => void startDeviceFlow()}>
                  {t("github.deviceStart")}
                </button>
              )}
            </div>
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
            <button type="button" className="toolbar-widget h-7 px-3" disabled={token.trim().length === 0} onClick={() => void loginWithToken()}>
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
                        {protection.protected
                          ? t("github.protected", { branch: protection.branch })
                          : t("github.notProtected", { branch: protection.branch })}
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
                  <button type="button" className="toolbar-widget h-6 px-2" disabled={busy !== null || commentBody.trim().length === 0} onClick={() => void submitComment()}>
                    {t("github.commentSend")}
                  </button>
                </section>

                <section className="space-y-2 rounded border border-panel-border p-2">
                  <h3 className="font-semibold">{t("github.reviewTitle")}</h3>
                  <p className="text-descriptionforeground">{t("github.reviewHint")}</p>
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
                        onClick={() =>
                          void m7.reviewDraftDelete(draft.id).then(async () => {
                            setDrafts(await m7.reviewDrafts({ worktreeId, remote, number: selected.number }));
                          })
                        }
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
                  <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void addDraft()}>
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
                    <button type="button" className="toolbar-widget h-6 px-2" disabled={busy !== null} onClick={() => void submitReview()}>
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
                  {new Date(receipt.createdAtMs).toLocaleString()} · {receipt.protectionSource}
                  {receipt.notes.length > 0 ? ` · ${receipt.notes.join("; ")}` : ""}
                </p>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
