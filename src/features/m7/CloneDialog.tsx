/**
 * 克隆仓库（M7 任务 05、Q179 / Q153）。
 *
 * 界面只收集 URL、目标目录与高级选项；Git 由 Rust 用独立 argv 调用。
 * 进度与结果走事件，取消与失败都保留目标目录里的残留内容并显示实际路径。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { CloneFinished } from "@/types/bindings/CloneFinished";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import { Codicon } from "@/components/ui/codicon";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7, onM7Events } from "@/lib/m7/api";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { useModalDialog } from "@/features/m7/glass/nativeGlass";

export interface CloneDialogProps {
  open: boolean;
  onClose: () => void;
  onRegistered: (repo: RepoEntry) => void;
}

type Phase = "form" | "running" | "finished" | "failed" | "cancelled";

export function CloneDialog({ open, onClose, onRegistered }: CloneDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const [url, setUrl] = useState("");
  const [target, setTarget] = useState("");
  const [branch, setBranch] = useState("");
  const [shallow, setShallow] = useState(false);
  const [depth, setDepth] = useState("1");
  const [submodules, setSubmodules] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const [phase, setPhase] = useState<Phase>("form");
  const [jobId, setJobId] = useState<number | null>(null);
  const [lines, setLines] = useState<string[]>([]);
  const [result, setResult] = useState<CloneFinished | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const logRef = useRef<HTMLPreElement | null>(null);
  const jobRef = useRef<number | null>(null);
  jobRef.current = jobId;

  useModalDialog(ref, open);

  useEffect(() => {
    if (!open) return;
    setPhase("form");
    setLines([]);
    setResult(null);
    setError(null);
    setJobId(null);
  }, [open]);

  // 进度与结束事件：只认当前任务，迟到结果不会切到另一个目标。
  useEffect(() => {
    return onM7Events({
      onCloneProgress: (event) => {
        if (event.jobId !== jobRef.current) return;
        setLines((current) => {
          const next = [...current, event.line];
          return next.length > 400 ? next.slice(next.length - 400) : next;
        });
      },
      onCloneFinished: (event) => {
        if (event.jobId !== jobRef.current) return;
        setResult(event);
        setPhase(event.success ? "finished" : event.cancelled ? "cancelled" : "failed");
        if (event.success && event.path) {
          void m7
            .cloneRegister(event.path)
            .then((repo) => onRegistered(repo))
            .catch((caught) => setError(toAppError(caught)));
        }
      },
    });
  }, [onRegistered]);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [lines]);

  const pickTarget = useCallback(async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: t("clone.pickTarget") });
    if (typeof picked === "string") setTarget(picked);
  }, []);

  const start = useCallback(async () => {
    setError(null);
    setLines([]);
    setResult(null);
    try {
      const id = await m7.cloneStart({
        url: url.trim(),
        target: target.trim(),
        branch: branch.trim().length > 0 ? branch.trim() : null,
        depth: shallow ? Math.max(1, Number.parseInt(depth, 10) || 1) : null,
        recurseSubmodules: submodules,
      });
      setJobId(id);
      setPhase("running");
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [branch, depth, shallow, submodules, target, url]);

  const cancel = useCallback(async () => {
    if (jobId === null) return;
    try {
      await m7.cloneCancel(jobId);
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [jobId]);

  if (!open) return null;

  const busy = phase === "running";

  return (
    <dialog ref={ref} className="app-dialog glass-isolate flex max-h-[80vh] w-[min(640px,calc(100vw-48px))] flex-col overflow-hidden p-0 text-[13px]" onCancel={(event) => { event.preventDefault(); if (!busy) onClose(); }}>
      <GlassLayer variant="popover" />
      <div className="flex h-9 items-center gap-2 border-b border-panel-border px-3 text-[13px] font-semibold">
        <Codicon name="cloud-download" size={15} />
        {t("clone.title")}
        <button type="button" className="icon-action ml-auto size-[22px]" aria-label={t("common.close")} onClick={onClose} disabled={busy}>
          <Codicon name="close" size={14} />
        </button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 text-[12.5px]">
        <label className="block">
          <span className="mb-1 block font-semibold">{t("clone.url")}</span>
          <input
            className="settings-input w-full"
            value={url}
            spellCheck={false}
            placeholder={t("clone.urlPlaceholder")}
            onChange={(event) => setUrl(event.target.value)}
            disabled={busy}
          />
        </label>
        <label className="block">
          <span className="mb-1 block font-semibold">{t("clone.target")}</span>
          <div className="flex gap-2">
            <input
              className="settings-input min-w-0 flex-1"
              value={target}
              spellCheck={false}
              placeholder={t("clone.targetPlaceholder")}
              onChange={(event) => setTarget(event.target.value)}
              disabled={busy}
            />
            <button type="button" className="toolbar-widget h-7 px-2" onClick={() => void pickTarget()} disabled={busy}>
              {t("clone.pickTarget")}
            </button>
          </div>
          <span className="mt-1 block text-[11.5px] text-descriptionforeground">{t("clone.targetHint")}</span>
        </label>
        <button type="button" className="flex items-center gap-1 text-left text-[12px] text-descriptionforeground" onClick={() => setAdvanced(!advanced)}>
          <Codicon name={advanced ? "chevron-down" : "chevron-right"} size={12} />
          {t("clone.advanced")}
        </button>
        {advanced ? (
          <div className="space-y-3 rounded border border-panel-border p-3">
            <label className="block">
              <span className="mb-1 block font-semibold">{t("clone.branch")}</span>
              <input className="settings-input w-full" value={branch} spellCheck={false} placeholder={t("clone.branchPlaceholder")} onChange={(event) => setBranch(event.target.value)} disabled={busy} />
            </label>
            <label className="flex items-center gap-2">
              <input type="checkbox" className="accent-[var(--focusborder)]" checked={shallow} onChange={(event) => setShallow(event.target.checked)} disabled={busy} />
              {t("clone.shallow")}
            </label>
            {shallow ? (
              <label className="block">
                <span className="mb-1 block font-semibold">{t("clone.depth")}</span>
                <input className="settings-input w-24" value={depth} inputMode="numeric" onChange={(event) => setDepth(event.target.value)} disabled={busy} />
              </label>
            ) : null}
            <label className="flex items-center gap-2">
              <input type="checkbox" className="accent-[var(--focusborder)]" checked={submodules} onChange={(event) => setSubmodules(event.target.checked)} disabled={busy} />
              {t("clone.submodules")}
            </label>
          </div>
        ) : null}

        {error ? (
          <p role="alert" className="rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1">
            {formatAppError(error)}
          </p>
        ) : null}

        {phase !== "form" ? (
          <div className="space-y-2">
            <p className="text-[12px] text-descriptionforeground">
              {phase === "running"
                ? t("clone.running", { target: result?.target ?? target })
                : phase === "finished"
                  ? t("clone.finished", { target: result?.target ?? target })
                  : phase === "cancelled"
                    ? t("clone.cancelled", { target: result?.target ?? target })
                    : t("clone.failed", { target: result?.target ?? target })}
            </p>
            {phase !== "finished" && phase !== "running" && result?.message ? (
              <pre className="max-h-40 overflow-auto rounded border border-panel-border bg-[var(--side-bar-background)] p-2 text-[11.5px] whitespace-pre-wrap">{result.message}</pre>
            ) : null}
            <pre ref={logRef} className="max-h-48 overflow-auto rounded border border-panel-border bg-[var(--side-bar-background)] p-2 text-[11.5px] whitespace-pre-wrap">{lines.join("\n")}</pre>
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center justify-end gap-2 border-t border-panel-border px-3 py-2">
        {busy ? (
          <button type="button" className="toolbar-widget h-7 px-3" onClick={() => void cancel()}>
            {t("clone.cancel")}
          </button>
        ) : (
          <button type="button" className="toolbar-widget h-7 px-3" onClick={onClose}>
            {t("common.close")}
          </button>
        )}
        <button
          type="button"
          className="toolbar-widget h-7 px-3 font-semibold"
          disabled={busy || url.trim().length === 0 || target.trim().length === 0}
          onClick={() => void start()}
        >
          {t("clone.start")}
        </button>
      </div>
    </dialog>
  );
}
