/**
 * 工作区文件编辑（M7 任务 03、Q122 / Q177 / Q186 / Q195）。
 *
 * 并排显示「打开时的内容」与「正在编辑的内容」，右侧可改、显式保存：
 * - 基线指纹由 Rust 提供，保存时写前核对；磁盘变化拒绝旧保存并保留两版；
 * - 每次改动自动存本机草稿（绑定 worktree、路径与基线），跨切换与重启保留；
 * - 只读原因（符号链接、子模块、二进制、过大、非 UTF-8、冲突）由 Rust 判定。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { EditNewline } from "@/types/bindings/EditNewline";
import type { EditReadonlyReason } from "@/types/bindings/EditReadonlyReason";
import type { EditableFile } from "@/types/bindings/EditableFile";
import { Codicon } from "@/components/ui/codicon";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { codeFontStack } from "@/lib/theme";
import { m7 } from "@/lib/m7/api";
import { registerM7Flusher } from "@/lib/m7/flush";
import { isPrimaryModifier } from "@/lib/platform";

/** 行号与基线面板的渲染上限：再大的文件只显示编辑区，避免界面卡死。 */
const MAX_RENDER_LINES = 20000;
/** 草稿自动保存的静默时间。 */
const DRAFT_DEBOUNCE_MS = 700;

export interface EditableWorkspaceProps {
  worktreeId: string;
  entry: ChangeEntry;
  fontSize: number;
  fontFamily: string | null;
  /** 保存成功后刷新快照（文件状态可能从 unstaged 变回已修改内容）。 */
  onSaved: () => void;
  /** 关闭编辑视图（返回 diff）。 */
  onClose: () => void;
  /** 有未保存改动时通知外层（关闭协调、更新安装门禁用）。 */
  onDirtyChange?: (dirty: boolean) => void;
}

type Conflict = { diskSha256: string | null };

function readonlyText(reason: EditReadonlyReason | null): string {
  switch (reason) {
    case "symlink":
      return t("edit.readonly.symlink");
    case "notRegularFile":
      return t("edit.readonly.notRegularFile");
    case "submodule":
      return t("edit.readonly.submodule");
    case "binary":
      return t("edit.readonly.binary");
    case "tooLarge":
      return t("edit.readonly.tooLarge");
    case "encoding":
      return t("edit.readonly.encoding");
    case "unmerged":
      return t("edit.readonly.unmerged");
    case "missing":
      return t("edit.readonly.missing");
    case "notInWorktree":
      return t("edit.readonly.notInWorktree");
    default:
      return t("edit.readonly.unknown");
  }
}

function newlineText(newline: EditNewline): string {
  return newline === "crlf" ? "CRLF" : "LF";
}

/** 文本 → 行数组（保留末尾空行的语义）。 */
function splitLines(text: string): string[] {
  return text.split("\n");
}

export function EditableWorkspace({
  worktreeId,
  entry,
  fontSize,
  fontFamily,
  onSaved,
  onClose,
  onDirtyChange,
}: EditableWorkspaceProps) {
  const [file, setFile] = useState<EditableFile | null>(null);
  const [baseline, setBaseline] = useState("");
  const [text, setText] = useState("");
  const [savedText, setSavedText] = useState("");
  const [baseSha, setBaseSha] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [conflict, setConflict] = useState<Conflict | null>(null);
  const [draftNotice, setDraftNotice] = useState(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const gutterRef = useRef<HTMLDivElement | null>(null);
  const draftTimer = useRef<number | null>(null);
  const latest = useRef({ text, baseSha, worktreeId, path: entry.path });
  latest.current = { text, baseSha, worktreeId, path: entry.path };
  const savedTextRef = useRef(savedText);
  savedTextRef.current = savedText;

  const dirty = file?.text !== null && text !== savedText;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  // 打开文件（含草稿恢复）。
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setConflict(null);
    setDraftNotice(false);
    void (async () => {
      try {
        const opened = await m7.openEditable(worktreeId, entry.path);
        if (cancelled) return;
        setFile(opened);
        if (opened.text === null || opened.baseSha256 === null) {
          setLoading(false);
          return;
        }
        setBaseline(opened.text);
        setBaseSha(opened.baseSha256);
        setSavedText(opened.text);
        setText(opened.text);
        const draft = await m7.loadEditDraft(worktreeId, entry.path).catch(() => null);
        if (cancelled) return;
        if (draft && draft.text !== opened.text) {
          if (draft.baseSha256 === opened.baseSha256) {
            // 基线一致：草稿就是未保存的那一版。
            setText(draft.text);
            setDraftNotice(true);
          } else {
            // F03：磁盘基线已变——保留两版。界面显示磁盘内容，草稿留在存储里不覆盖，
            // 冲突提示走既有 conflict 流程（重新加载或保留我的版本）。
            setText(opened.text);
            setSavedText(opened.text);
            setConflict({ diskSha256: opened.baseSha256 });
            setDraftNotice(true);
          }
        }
        setLoading(false);
      } catch (caught) {
        if (cancelled) return;
        setError(toAppError(caught));
        setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [worktreeId, entry.path]);

  // 草稿自动保存：静默一会儿再写，卸载 / 关闭前再兜底一次（H1/F03：切换不丢最新输入）。
  const saveDraft = useCallback(async () => {
    const current = latest.current;
    if (!current.baseSha || current.text === savedText) return;
    await m7
      .saveEditDraft({
        worktreeId: current.worktreeId,
        path: current.path,
        baseSha256: current.baseSha,
        text: current.text,
        updatedAtMs: Date.now(),
      })
      .catch(() => undefined);
  }, [savedText]);

  useEffect(() => {
    if (!dirty || baseSha === null) return;
    if (draftTimer.current !== null) window.clearTimeout(draftTimer.current);
    draftTimer.current = window.setTimeout(() => {
      draftTimer.current = null;
      void saveDraft();
    }, DRAFT_DEBOUNCE_MS);
    return () => {
      if (draftTimer.current !== null) {
        window.clearTimeout(draftTimer.current);
        draftTimer.current = null;
      }
      // 卸载（切文件 / 切 worktree / 关面板）时立刻落草稿：不能只取消定时器。
      const current = latest.current;
      if (current.baseSha && current.text !== savedTextRef.current) {
        void m7
          .saveEditDraft({
            worktreeId: current.worktreeId,
            path: current.path,
            baseSha256: current.baseSha,
            text: current.text,
            updatedAtMs: Date.now(),
          })
          .catch(() => undefined);
      }
    };
  }, [text, dirty, baseSha, saveDraft]);

  // 关闭前冲刷：把当前草稿同步写盘（失败则抛出，阻止关闭）。
  useEffect(() => {
    return registerM7Flusher(async () => {
      const current = latest.current;
      if (!current.baseSha || current.text === savedText) return;
      await m7.saveEditDraft({
        worktreeId: current.worktreeId,
        path: current.path,
        baseSha256: current.baseSha,
        text: current.text,
        updatedAtMs: Date.now(),
      });
    });
  }, [savedText]);

  const save = useCallback(async () => {
    const current = latest.current;
    if (!current.baseSha || saving) return;
    setSaving(true);
    setError(null);
    try {
      const result = await m7.saveEditable({
        worktreeId: current.worktreeId,
        path: current.path,
        baseSha256: current.baseSha,
        text: current.text,
      });
      setBaseSha(result.sha256);
      setSavedText(current.text);
      setBaseline(current.text);
      setDraftNotice(false);
      await m7.clearEditDraft(current.worktreeId, current.path).catch(() => undefined);
      onSaved();
    } catch (caught) {
      const failure = toAppError(caught);
      setError(failure);
      if (failure.code === "errors.m7.editFileChangedOnDisk") {
        setConflict({ diskSha256: failure.params?.diskSha256 ?? null });
      }
    } finally {
      setSaving(false);
    }
  }, [onSaved, saving]);

  /** 重新读磁盘：拿到新的基线指纹，同时保留用户正在编辑的文本。 */
  const reload = useCallback(
    async (keepMine: boolean) => {
      try {
        const opened = await m7.openEditable(worktreeId, entry.path);
        setFile(opened);
        if (opened.text === null || opened.baseSha256 === null) {
          setError(null);
          return;
        }
        setBaseline(opened.text);
        setBaseSha(opened.baseSha256);
        setConflict(null);
        setError(null);
        if (!keepMine) {
          setText(opened.text);
          setSavedText(opened.text);
          setDraftNotice(false);
          await m7.clearEditDraft(worktreeId, entry.path).catch(() => undefined);
        } else {
          // 保留我的版本：基线换成磁盘当前版本，用户需要再点一次保存确认覆盖。
          setSavedText(opened.text);
        }
      } catch (caught) {
        setError(toAppError(caught));
      }
    },
    [entry.path, worktreeId],
  );

  const requestClose = useCallback(() => {
    if (dirty) {
      setConfirmClose(true);
      return;
    }
    onClose();
  }, [dirty, onClose]);

  // 保存快捷键：编辑区内 ⌘S / Ctrl+S 保存。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isPrimaryModifier(event) || event.code !== "KeyS") return;
      event.preventDefault();
      void save();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [save]);

  const baselineLines = useMemo(() => {
    if (baseline === "") return [""];
    return splitLines(baseline).slice(0, MAX_RENDER_LINES);
  }, [baseline]);
  const editorLines = useMemo(() => {
    const lines = splitLines(text);
    return lines.length > MAX_RENDER_LINES ? lines.slice(0, MAX_RENDER_LINES) : lines;
  }, [text]);
  const truncated = splitLines(text).length > MAX_RENDER_LINES;

  const editable = file !== null && file.text !== null && file.readonlyReason === null;

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--editor-background)]">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-panel-border px-3 text-[12px]">
        <Codicon name="edit" size={14} />
        <span className="truncate font-semibold">{entry.path}</span>
        {file ? (
          <span className="shrink-0 text-descriptionforeground">
            {t("edit.meta", {
              newline: newlineText(file.newline),
              bytes: file.sizeBytes,
            })}
          </span>
        ) : null}
        {file?.executable ? (
          <span className="shrink-0 text-descriptionforeground">{t("edit.executable")}</span>
        ) : null}
        <span className="ml-auto shrink-0 text-descriptionforeground">
          {dirty ? t("edit.unsaved") : t("edit.saved")}
        </span>
        <button
          type="button"
          className="toolbar-widget h-6 gap-1 px-2 text-[12px]"
          disabled={!editable || saving || !dirty}
          onClick={() => void save()}
        >
          <Codicon name="save" size={14} />
          {saving ? t("edit.saving") : t("edit.save")}
        </button>
        <button
          type="button"
          className="icon-action size-[22px]"
          title={t("edit.close")}
          aria-label={t("edit.close")}
          onClick={requestClose}
        >
          <Codicon name="close" size={14} />
        </button>
      </div>

      {error ? (
        <div role="alert" className="mx-3 mt-2 rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] bg-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_10%,var(--editor-background))] px-3 py-2 text-[12px]">
          {formatAppError(error)}
        </div>
      ) : null}

      {conflict ? (
        <div className="mx-3 mt-2 flex flex-wrap items-center gap-2 rounded border border-[color-mix(in_srgb,var(--gitdecoration-modifiedresourceforeground)_40%,transparent)] bg-[color-mix(in_srgb,var(--gitdecoration-modifiedresourceforeground)_10%,var(--editor-background))] px-3 py-2 text-[12px]">
          <span className="min-w-0 flex-1">{t("edit.conflict")}</span>
          <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void reload(false)}>
            {t("edit.conflictReload")}
          </button>
          <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void reload(true)}>
            {t("edit.conflictKeepMine")}
          </button>
        </div>
      ) : null}

      {draftNotice ? (
        <div className="mx-3 mt-2 flex flex-wrap items-center gap-2 rounded border border-panel-border bg-[var(--side-bar-background)] px-3 py-2 text-[12px]">
          <span className="min-w-0 flex-1">{t("edit.draftRestored")}</span>
          <button
            type="button"
            className="toolbar-widget h-6 px-2"
            onClick={() => {
              setText(baseline);
              setSavedText(baseline);
              setDraftNotice(false);
              void m7.clearEditDraft(worktreeId, entry.path).catch(() => undefined);
            }}
          >
            {t("edit.draftDiscard")}
          </button>
        </div>
      ) : null}

      {loading ? (
        <div className="p-4 text-[12px] text-descriptionforeground">{t("edit.loading")}</div>
      ) : !editable ? (
        <div className="p-4 text-[12px] text-descriptionforeground">
          <p className="mb-1 font-semibold">{t("edit.readonlyTitle")}</p>
          <p>{readonlyText(file?.readonlyReason ?? null)}</p>
        </div>
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-2 divide-x divide-panel-border">
          <div className="flex min-h-0 min-w-0 flex-col">
            <div className="flex h-6 shrink-0 items-center justify-between border-b border-panel-border px-2 text-[11px] text-descriptionforeground">
              <span>{t("edit.baseline")}</span>
              <span>{t("edit.baselineHint")}</span>
            </div>
            <div className="min-h-0 flex-1 overflow-auto">
              <div
                className="min-h-full py-1"
                style={{ fontFamily: codeFontStack(fontFamily), fontSize }}
              >
                {baselineLines.map((line, index) => (
                  <div key={index} className="flex">
                    <span className="w-12 shrink-0 select-none pr-2 text-right text-descriptionforeground">{index + 1}</span>
                    <span className="whitespace-pre">{line}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
          <div className="flex min-h-0 min-w-0 flex-col">
            <div className="flex h-6 shrink-0 items-center justify-between border-b border-panel-border px-2 text-[11px] text-descriptionforeground">
              <span>{t("edit.editor")}</span>
              <span>{truncated ? t("edit.truncated") : null}</span>
            </div>
            <div className="relative min-h-0 flex-1 overflow-hidden">
              <div
                ref={gutterRef}
                className="absolute left-0 top-0 h-full w-12 select-none overflow-hidden bg-[var(--editor-background)] pr-2 text-right text-descriptionforeground"
                style={{ fontFamily: codeFontStack(fontFamily), fontSize }}
                aria-hidden
              >
                <div className="py-1">
                  {editorLines.map((_, index) => (
                    <div key={index} style={{ lineHeight: 1.45 }}>{index + 1}</div>
                  ))}
                </div>
              </div>
              <textarea
                ref={textareaRef}
                className="absolute inset-y-0 left-12 right-0 w-[calc(100%-3rem)] resize-none overflow-auto bg-transparent py-1 pl-2 pr-3 outline-none"
                style={{ fontFamily: codeFontStack(fontFamily), fontSize, lineHeight: 1.45, tabSize: 4 }}
                value={text}
                spellCheck={false}
                wrap="off"
                onChange={(event) => setText(event.target.value)}
                onScroll={(event) => {
                  if (gutterRef.current) gutterRef.current.scrollTop = event.currentTarget.scrollTop;
                }}
                onKeyDown={(event) => {
                  if (event.key === "Tab" && !event.metaKey && !event.ctrlKey) {
                    event.preventDefault();
                    const target = event.currentTarget;
                    const start = target.selectionStart;
                    const end = target.selectionEnd;
                    const next = `${text.slice(0, start)}  ${text.slice(end)}`;
                    setText(next);
                    requestAnimationFrame(() => {
                      target.selectionStart = start + 2;
                      target.selectionEnd = start + 2;
                    });
                  }
                }}
              />
            </div>
          </div>
        </div>
      )}

      {confirmClose ? (
        <div className="absolute inset-0 z-30 flex items-center justify-center bg-black/30">
          <div role="dialog" aria-modal="true" className="w-[420px] rounded-md border border-panel-border bg-[var(--editor-background)] p-4 text-[12.5px] shadow-xl">
            <p className="mb-3">{t("edit.closeConfirm")}</p>
            <div className="flex justify-end gap-2">
              <button type="button" className="toolbar-widget h-7 px-3" onClick={() => setConfirmClose(false)}>
                {t("edit.closeCancel")}
              </button>
              <button
                type="button"
                className="toolbar-widget h-7 px-3"
                onClick={() => {
                  // H1 修正：「放弃」丢弃未保存改动并清掉草稿，不再保存草稿。
                  setConfirmClose(false);
                  const current = latest.current;
                  void m7
                    .clearEditDraft(current.worktreeId, current.path)
                    .catch(() => undefined)
                    .finally(onClose);
                }}
              >
                {t("edit.closeDiscard")}
              </button>
              <button
                type="button"
                className="toolbar-widget h-7 px-3"
                disabled={saving}
                onClick={() => {
                  // H2/F04 修正：保存成功（回到磁盘基线）后关闭面板。
                  void (async () => {
                    await save();
                    setConfirmClose(false);
                    if (!dirtyRef.current) onClose();
                  })();
                }}
              >
                {t("edit.closeSave")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
