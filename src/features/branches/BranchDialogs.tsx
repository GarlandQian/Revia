/**
 * 分支与远端的对话框（M5 B02、B04、B05）。
 *
 * - 新建分支：名字（按 Git 规则校验）、起点说明、“检出新分支”（默认勾选）；
 * - 检出被拒：列出会被覆盖的文件，提供智能检出 / 强制检出 / 取消（Q150）；
 * - 推送：列出将要推送的提交、目标远端 / 分支、推送标签、强制推送（受保护分支禁用）。
 *
 * 这些都只是界面：真正的写入由 App 经统一写编排执行（`runRemoteWrite`）。
 */

import { useEffect, useMemo, useState } from "react";
import type { BranchOverview } from "@/types/bindings/BranchOverview";
import type { BranchStart } from "@/types/bindings/BranchStart";
import type { PushPreview } from "@/types/bindings/PushPreview";
import type { PushForce } from "@/types/bindings/PushForce";
import { messages } from "@/lib/i18n/index";
import { formatAppError } from "@/lib/error";
import { formatCommitDate, shortOid } from "@/features/log/logModel";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "@/components/app/Dialogs";
import { quickBranchNameProblem } from "./useBranches";
import { branchStartLabel } from "./BranchWidget";

/** 新建分支对话框。 */
export function NewBranchDialog({
  overview,
  start,
  busy,
  error,
  onCancel,
  onConfirm,
}: {
  overview: BranchOverview | null;
  start: BranchStart;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onConfirm: (request: { name: string; checkout: boolean }) => void;
}) {
  const [name, setName] = useState("");
  const [checkout, setCheckout] = useState(true);
  const problem = quickBranchNameProblem(name);
  const exists = overview?.local.some((branch) => branch.name === name.trim()) ?? false;
  const blocking = problem ?? (exists ? messages.branches.nameExists : null);
  return (
    <Modal title={messages.branches.newBranch} onClose={onCancel} width={460}>
      <div className="space-y-3">
        <label className="block">
          <span className="mb-1 block text-[12px] text-descriptionforeground">{messages.branches.name}</span>
          <input
            className="settings-input w-full"
            autoFocus
            value={name}
            spellCheck={false}
            placeholder={messages.branches.namePlaceholder}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !blocking && !busy) onConfirm({ name: name.trim(), checkout });
            }}
          />
        </label>
        <p className="text-[11.5px] text-descriptionforeground">
          {branchStartLabel(start, overview?.currentBranch ?? null, overview?.shortHead ?? null)}
        </p>
        <label className="flex items-center gap-2 text-[12.5px]">
          <input type="checkbox" checked={checkout} onChange={(event) => setCheckout(event.target.checked)} />
          {messages.branches.checkoutNew}
        </label>
        {blocking ? <p className="text-[11.5px] text-destructive">{blocking}</p> : null}
        {error ? <p className="text-[11.5px] text-destructive">{error}</p> : null}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" className="dialog-button" onClick={onCancel}>
            {messages.branches.cancel}
          </button>
          <button
            type="button"
            className="dialog-button primary"
            disabled={!!blocking || busy}
            onClick={() => onConfirm({ name: name.trim(), checkout })}
          >
            {messages.branches.create}
          </button>
        </div>
      </div>
    </Modal>
  );
}

/** 检出被拒：会被覆盖的文件与智能 / 强制检出（Q150）。 */
export function CheckoutConflictDialog({
  target,
  files,
  busy,
  onCancel,
  onSmart,
  onForce,
}: {
  target: string;
  files: string[];
  busy: boolean;
  onCancel: () => void;
  onSmart: () => void;
  onForce: () => void;
}) {
  return (
    <Modal title={messages.branches.checkoutBlocked} onClose={onCancel} width={560}>
      <div className="space-y-3">
        <p className="text-[12.5px]">
          {messages.branches.checkoutBlockedDetail(target)}
        </p>
        <ul className="max-h-[220px] overflow-auto rounded border border-panel-border bg-[var(--sidebar-background)] px-2 py-1 font-mono text-[11.5px]">
          {files.map((file) => (
            <li key={file} className="truncate" title={file}>{file}</li>
          ))}
        </ul>
        <p className="text-[11.5px] text-descriptionforeground">
          {messages.branches.smartDetail}
        </p>
        <p className="text-[11.5px] text-[var(--editorwarning-foreground)]">
          {messages.branches.forceDetail}
        </p>
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" className="dialog-button" onClick={onCancel}>
            {messages.branches.cancel}
          </button>
          <button type="button" className="dialog-button" disabled={busy} onClick={onForce}>
            {messages.branches.forceCheckout}
          </button>
          <button type="button" className="dialog-button primary" disabled={busy} onClick={onSmart}>
            {messages.branches.smartCheckout}
          </button>
        </div>
      </div>
    </Modal>
  );
}

/** 推送对话框（B05）。 */
export function PushDialog({
  preview,
  busy,
  error,
  onCancel,
  onPush,
}: {
  preview: PushPreview;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onPush: (request: { remote: string; branch: string; tags: boolean; force: PushForce }) => void;
}) {
  const [remote, setRemote] = useState(preview.remote);
  const [branch, setBranch] = useState(preview.targetBranch);
  const [tags, setTags] = useState(false);
  const [forceConfirmed, setForceConfirmed] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const forceProblem = preview.forceAllowed
    ? null
    : preview.forceReason
      ? formatAppError(preview.forceReason)
      : messages.branches.forceDisabled;
  const commits = useMemo(() => preview.commits.slice(0, 50), [preview.commits]);

  useEffect(() => {
    setRemote(preview.remote);
    setBranch(preview.targetBranch);
  }, [preview.remote, preview.targetBranch]);

  const submit = (force: PushForce) => {
    if (force === "withLease" && !forceConfirmed) return;
    onPush({ remote, branch, tags, force });
  };

  return (
    <Modal title={messages.branches.pushTitle} onClose={onCancel} width={640}>
      <div className="space-y-3">
        <div className="flex items-center gap-2 text-[12.5px]">
          <Codicon name="git-branch" size={13} />
          <span className="font-medium">{preview.branch}</span>
          <Codicon name="arrow-right" size={13} />
          {preview.remotes.length > 1 ? (
            <select className="settings-select" value={remote} onChange={(event) => setRemote(event.target.value)}>
              {preview.remotes.map((name) => (
                <option key={name} value={name}>{name}</option>
              ))}
            </select>
          ) : (
            <span>{remote}</span>
          )}
          <span className="text-descriptionforeground">/</span>
          <input
            className="settings-input w-[180px]"
            value={branch}
            spellCheck={false}
            onChange={(event) => setBranch(event.target.value)}
          />
          {preview.hasUpstream ? null : (
            <span className="text-[11px] text-descriptionforeground">{messages.branches.pushSetsUpstream}</span>
          )}
        </div>
        <div className="rounded border border-panel-border">
          <div className="flex items-center gap-2 border-b border-panel-border px-2 py-1 text-[11.5px] text-descriptionforeground">
            <span className="flex-1">{messages.branches.commitsToPush(preview.commitCount)}</span>
            {preview.truncated ? <span>{messages.branches.truncated}</span> : null}
          </div>
          <div className="max-h-[220px] overflow-auto py-1">
            {commits.length === 0 ? (
              <p className="px-2 py-1 text-[11.5px] text-descriptionforeground">{messages.branches.noCommitsToPush}</p>
            ) : null}
            {commits.map((commit) => (
              <div key={commit.oid} className="flex items-center gap-2 px-2 py-[3px] text-[12px]">
                <span className="w-[64px] shrink-0 font-mono text-[11px] text-descriptionforeground">{shortOid(commit.oid)}</span>
                <span className="min-w-0 flex-1 truncate" title={commit.subject}>{commit.subject}</span>
                <span className="w-[120px] shrink-0 truncate text-[11px] text-descriptionforeground">{commit.author.name}</span>
                <span className="w-[96px] shrink-0 text-[11px] text-descriptionforeground tabular-nums">
                  {formatCommitDate(commit.author.time)}
                </span>
              </div>
            ))}
          </div>
        </div>
        <label className="flex items-center gap-2 text-[12.5px]">
          <input type="checkbox" checked={tags} onChange={(event) => setTags(event.target.checked)} />
          {messages.branches.pushTags}
        </label>
        {forceProblem ? (
          <p className="text-[11.5px] text-[var(--editorwarning-foreground)]">{forceProblem}</p>
        ) : null}
        {error ? <p className="text-[11.5px] text-destructive">{error}</p> : null}
        <div className="flex items-center justify-end gap-2 pt-1">
          <button type="button" className="dialog-button" onClick={onCancel}>
            {messages.branches.cancel}
          </button>
          <div className="relative">
            <button
              type="button"
              className="dialog-button primary"
              disabled={busy || !branch.trim()}
              onClick={() => submit("none")}
            >
              {messages.branches.push}
            </button>
            <button
              type="button"
              className="dialog-button primary ml-1 px-1"
              aria-label={messages.branches.forcePushMenu}
              disabled={busy}
              onClick={() => setMenuOpen((value) => !value)}
            >
              <Codicon name="chevron-down" size={12} />
            </button>
            {menuOpen ? (
              <div className="revia-popper absolute right-0 bottom-[calc(100%+4px)] z-10 p-1 text-[12.5px]">
                <button
                  type="button"
                  role="menuitem"
                  className="flex w-full items-center gap-2 rounded-[3px] px-2 py-1 text-left whitespace-nowrap hover:bg-toolbar-hoverbackground disabled:opacity-50"
                  disabled={!preview.forceAllowed}
                  onClick={() => {
                    setMenuOpen(false);
                    setForceConfirmed(true);
                  }}
                >
                  <Codicon name="warning" size={12} />
                  {messages.branches.forceWithLease}
                </button>
                {forceConfirmed && !forceProblem ? (
                  <div className="px-2 py-1 text-[11px] text-[var(--editorwarning-foreground)]">
                    {messages.branches.forceConfirmHint(remote, branch)}
                    <button
                      type="button"
                      className="dialog-button ml-1"
                      disabled={busy}
                      onClick={() => {
                        setMenuOpen(false);
                        submit("withLease");
                      }}
                    >
                      {messages.branches.forceConfirm}
                    </button>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </Modal>
  );
}
