/**
 * 应用更新（M7 任务 11、Q199）。
 *
 * 更新包只由 tauri-plugin-updater 下载并校验签名；界面负责展示状态、
 * 让用户确认安装，并在安装前把草稿落盘（由 App 传入的门禁完成）。
 */

import { useEffect, useRef, useState } from "react";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { AppUpdateMode } from "@/types/bindings/AppUpdateMode";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import { Codicon } from "@/components/ui/codicon";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { updateReasonText } from "@/lib/i18n/text";
import { updateCandidate } from "@/lib/m7/updateState";
import { m7, onM7Events } from "@/lib/m7/api";
import { useModalDialog } from "@/features/m7/glass/nativeGlass";

export interface UpdatePanelProps {
  open: boolean;
  status: AppUpdateStatus | null;
  mode: AppUpdateMode;
  onStatusChange: (status: AppUpdateStatus) => void;
  onModeChange: (mode: AppUpdateMode) => void;
  /** 安装前门禁：落盘草稿、结束终端；返回 false 表示用户取消。 */
  onInstall: () => Promise<AppUpdateStatus | null>;
  onRestart: () => Promise<AppUpdateStatus | null>;
  onClose: () => void;
}

export function UpdatePanel({ open, status, mode, onStatusChange, onModeChange, onInstall, onRestart, onClose }: UpdatePanelProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const [progress, setProgress] = useState<{ downloaded: number; total: number | null } | null>(null);
  const [operation, setOperation] = useState<"check" | "download" | "install" | "restart" | null>(null);
  const busy = operation !== null;
  const requestEpoch = useRef(0);
  const statusCallback = useRef(onStatusChange);
  statusCallback.current = onStatusChange;
  const candidate = updateCandidate(status);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useModalDialog(ref, open);

  useEffect(() => {
    if (!open) return;
    const epoch = ++requestEpoch.current;
    setProgress(null); setError(null); setNotice(null);
    void m7.updateStatus().then((next) => { if (epoch === requestEpoch.current) statusCallback.current(next); })
      .catch((caught) => { if (epoch === requestEpoch.current) setError(toAppError(caught)); });
    return () => { requestEpoch.current++; };
  }, [open]);

  useEffect(() => {
    return onM7Events({
      onUpdateProgress: (event) => setProgress({ downloaded: event.downloaded, total: event.total ?? null }),
    });
  }, []);

  if (!open) return null;

  const publishStatus = (next: AppUpdateStatus) => {
    // 同一个应用的操作完成结果比其间打开面板的旧status读取更新。
    requestEpoch.current++; statusCallback.current(next);
  };
  const check = async (force: boolean) => {
    const epoch = ++requestEpoch.current; setOperation("check");
    try { const next = await m7.updateCheck(force); publishStatus(next); setError(null); }
    catch (caught) { if (epoch === requestEpoch.current) setError(toAppError(caught)); }
    finally { setOperation(null); }
  };
  const install = async () => {
    const epoch = ++requestEpoch.current; setError(null);
    try {
      if (!candidate.downloaded) {
        setOperation("download");
        const next = await m7.updateDownload();
        publishStatus(next); setNotice(null);
      } else {
        setOperation("install");
        const next = await onInstall();
        if (next) { publishStatus(next); setNotice(t("update.installed", { version: next.receipt?.version ?? "" })); }
      }
    } catch (caught) { if (epoch === requestEpoch.current) setError(toAppError(caught)); }
    finally { setOperation(null); }
  };
  const restart = async () => {
    const epoch = ++requestEpoch.current; setOperation("restart");
    try { await onRestart(); }
    catch (caught) { if (epoch === requestEpoch.current) setError(toAppError(caught)); }
    finally { setOperation(null); }
  };

  const percent = progress && progress.total && progress.total > 0 ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : null;

  return (
    <dialog ref={ref} className="app-dialog glass-isolate w-[min(560px,calc(100vw-48px))] overflow-hidden p-0 text-[13px]" onCancel={(event) => { event.preventDefault(); if (operation !== "install" && operation !== "restart") onClose(); }}>
      <GlassLayer variant="popover" />
      <div className="flex items-center gap-2 border-b border-panel-border px-3 py-2">
        <Codicon name="cloud-download" size={15} />
        <h2 className="flex-1 text-[13.5px] font-semibold">{t("update.title")}</h2>
        <button type="button" className="icon-action size-[22px]" aria-label={t("common.close")} disabled={operation === "install" || operation === "restart"} onClick={onClose}>
          <Codicon name="close" size={14} />
        </button>
      </div>
      <div className="space-y-3 p-4 text-[12.5px]">
        {error ? (
          <p role="alert" className="rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1">
            {formatAppError(error)}
          </p>
        ) : null}
        {notice ? <p role="status">{notice}</p> : null}
        <p className="text-descriptionforeground">{t("update.current", { version: status?.currentVersion ?? "—" })}</p>
        {status && !status.configured ? (
          <p className="rounded border border-panel-border bg-[var(--side-bar-background)] px-2 py-1 text-descriptionforeground">
            {status.unconfiguredReason ? updateReasonText(status.unconfiguredReason) : t("update.unconfigured")}
          </p>
        ) : null}
        <div className="flex items-center gap-2">
          <select className="settings-select h-6 w-auto" value={mode} onChange={(event) => onModeChange(event.target.value as AppUpdateMode)}>
            <option value="manual">{t("update.modeManual")}</option>
            <option value="notify">{t("update.modeNotify")}</option>
          </select>
          <button type="button" className="toolbar-widget h-6 px-2" disabled={busy} onClick={() => void check(true)}>
            {busy ? t("update.checking") : t("update.check")}
          </button>
        </div>
        {candidate.restartVersion ? (
          <div className="space-y-2 rounded border border-panel-border p-2">
            <p className="font-semibold">{t("update.installed", { version: candidate.restartVersion })}</p>
            <button type="button" className="toolbar-widget h-7 px-3 font-semibold" disabled={busy} onClick={() => void restart()}>{t("update.restart")}</button>
          </div>
        ) : null}
        {candidate.version ? (
          <div className="space-y-2 rounded border border-panel-border p-2">
            <p className="font-semibold">{t("update.available", { version: candidate.version })}</p>
            {status?.available?.publishedAt ? <p className="text-descriptionforeground">{t("update.published", { time: status.available.publishedAt })}</p> : null}
            {status?.available?.notes ? <pre className="max-h-40 overflow-auto whitespace-pre-wrap text-[11.5px]">{status.available.notes}</pre> : null}
            {candidate.downloaded ? <p>{t("update.downloaded")}</p> : null}
            {percent !== null && operation === "download" ? <div className="h-1.5 w-full overflow-hidden rounded bg-[var(--progressbar-background)]"><div className="h-full bg-[var(--progressbar-foreground)]" style={{ width: `${percent}%` }} /></div> : null}
            <div className="flex items-center gap-2">
              <button type="button" className="toolbar-widget h-7 px-3 font-semibold" disabled={busy || status?.restartRequired || error?.kind === "needsVerification"} onClick={() => void install()}>{candidate.downloaded ? t("update.installNow") : t("update.downloadConfirm")}</button>
              {operation === "download" ? <span className="text-descriptionforeground">{t("update.downloading")}</span> : null}
            </div>
          </div>
        ) : !candidate.restartVersion ? <p className="text-descriptionforeground">{t("update.upToDate")}</p> : null}
        {status?.lastCheckedAtMs ? (
          <p className="text-[11.5px] text-descriptionforeground">{t("update.lastChecked", { time: new Date(status.lastCheckedAtMs).toLocaleString() })}</p>
        ) : null}
      </div>
    </dialog>
  );
}

/** 顶部提示条：有新版本时出现，不阻塞审阅。 */
export function UpdateBanner({
  version,
  onOpen,
  onDismiss,
}: {
  version: string;
  onOpen: () => void;
  onDismiss: () => void;
}) {
  return (
    <div role="status" className="flex items-center gap-2 border-b border-panel-border bg-[var(--side-bar-background)] px-3 py-1 text-[12px]">
      <Codicon name="cloud-download" size={13} />
      <span className="min-w-0 flex-1 truncate">{t("update.banner", { version })}</span>
      <button type="button" className="toolbar-widget h-5 px-2" onClick={onOpen}>
        {t("update.bannerOpen")}
      </button>
      <button type="button" className="icon-action size-5" aria-label={t("common.dismiss")} onClick={onDismiss}>
        <Codicon name="close" size={12} />
      </button>
    </div>
  );
}
