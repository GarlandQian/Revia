/**
 * 诊断信息（M7 任务 10、Q183）。
 *
 * 预览与导出使用 Rust 生成的同一份脱敏内容；界面只提示脱敏次数，
 * 不展示被替换的原文，也不读取仓库内容。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { Codicon } from "@/components/ui/codicon";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7 } from "@/lib/m7/api";

export interface DiagnosticsDialogProps {
  open: boolean;
  worktreeId: string | null;
  onClose: () => void;
}

export function DiagnosticsDialog({ open, worktreeId, onClose }: DiagnosticsDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const [text, setText] = useState("");
  const [redactions, setRedactions] = useState(0);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [noticeText, setNoticeText] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || !open) return;
    dialog.showModal();
    return () => dialog.close();
  }, [open]);

  const loadPreview = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const preview = await m7.previewDiagnostics({ destination: "", worktreeId });
      setText(preview.text);
      setRedactions(preview.redactions);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setLoading(false);
    }
  }, [worktreeId]);

  useEffect(() => {
    if (!open) return;
    setNotice(null);
    void loadPreview();
  }, [loadPreview, open]);

  const exportFile = useCallback(async () => {
    const picked = await saveDialog({
      title: t("diagnostics.saveTitle"),
      defaultPath: "revia-diagnostics.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    setError(null);
    try {
      const result = await m7.exportDiagnostics({ destination: picked, worktreeId });
      setNotice(t("diagnostics.exported", { path: result.path, bytes: result.bytes, redactions: result.redactions }));
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(false);
    }
  }, [worktreeId]);

  if (!open) return null;

  return (
    <dialog
      ref={ref}
      className="app-dialog glass-isolate flex h-[min(680px,calc(100vh-96px))] w-[min(760px,calc(100vw-48px))] flex-col overflow-hidden p-0 text-[13px]"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <GlassLayer variant="popover" />
      <div className="flex items-center gap-2 border-b border-panel-border px-3 py-2">
        <Codicon name="output" size={15} />
        <h2 className="flex-1 text-[13.5px] font-semibold">{t("diagnostics.title")}</h2>
        <button type="button" className="icon-action size-[22px]" aria-label={t("common.close")} onClick={onClose}>
          <Codicon name="close" size={14} />
        </button>
      </div>
      <div className="space-y-2 border-b border-panel-border px-3 py-2 text-[12px]">
        <p className="text-descriptionforeground">{t("diagnostics.hint")}</p>
        <p className="text-descriptionforeground">{t("diagnostics.redactions", { count: redactions })}</p>
        <div className="flex items-center gap-2">
          <button type="button" className="toolbar-widget h-6 px-2" disabled={loading} onClick={() => void loadPreview()}>
            {loading ? t("common.loading") : t("diagnostics.refresh")}
          </button>
          <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void navigator.clipboard.writeText(text)}>
            {t("diagnostics.copy")}
          </button>
          <button type="button" className="toolbar-widget h-6 px-2 font-semibold" disabled={busy} onClick={() => void exportFile()}>
            {t("diagnostics.export")}
          </button>
          <button
            type="button"
            className="toolbar-widget h-6 px-2"
            onClick={() => {
              // F21：清空 7 天持久化摘要（当前仓库范围）。
              void (worktreeId
                ? m7.diagnosticsClear(worktreeId).then(() => setNoticeText(t("diagnostics.cleared"))).catch(() => undefined)
                : Promise.resolve());
            }}
          >
            {t("diagnostics.clear")}
          </button>
        </div>
        {error ? (
          <p role="alert" className="rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1">
            {formatAppError(error)}
          </p>
        ) : null}
        {noticeText ? <p role="status">{noticeText}</p> : null}
        {notice ? <p role="status">{notice}</p> : null}
      </div>
      <pre className="min-h-0 flex-1 overflow-auto bg-[var(--editor-background)] p-3 font-mono text-[11.5px] whitespace-pre-wrap">{text}</pre>
    </dialog>
  );
}
