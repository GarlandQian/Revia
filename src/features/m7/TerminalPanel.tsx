/**
 * 内置终端（M7 任务 04、Q178 / Q196）。
 *
 * Rust 侧创建 PTY（登录 shell），这里只做三件事：
 * 把用户按键按字节发给 PTY、把输出写进 xterm、把尺寸变化回传。
 * 不在前端拼命令、不保存输出；重启只恢复页签与起始目录并新开 shell。
 */

import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { Codicon } from "@/components/ui/codicon";
import { formatAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7 } from "@/lib/m7/api";
import { codeFontStack } from "@/lib/theme";
import { TerminalSessions } from "@/lib/m7/terminalSessions";

interface Instance {
  terminal: Terminal;
  fit: FitAddon;
  container: HTMLDivElement;
  observer: ResizeObserver | null;
}

export interface TerminalPanelProps {
  worktreeId: string;
  worktreePath: string;
  fontSize: number;
  fontFamily: string | null;
  height: number;
  onHeightChange: (height: number) => void;
  registry: TerminalSessions;
  busy: boolean;
  onClose: () => void;
}

function terminalTheme(): Record<string, string> {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) => {
    const value = style.getPropertyValue(name).trim();
    return value.length > 0 ? value : fallback;
  };
  return {
    background: read("--editor-background", "#1e1e1e"),
    foreground: read("--ui-foreground", "#dddddd"),
    cursor: read("--ui-foreground", "#dddddd"),
    selectionBackground: read("--editor-selectionbackground", "#264f78"),
  };
}

export function TerminalPanel({
  worktreeId,
  worktreePath,
  fontSize,
  fontFamily,
  height,
  onHeightChange,
  registry,
  busy,
  onClose,
}: TerminalPanelProps) {
  const revision = useSyncExternalStore(registry.subscribe, registry.snapshot);
  const tabs = registry.list(worktreeId);
  const error = registry.errorFor(worktreeId);
  const starting = registry.isStarting(worktreeId);
  const [active, setActive] = useState<number | null>(null);
  const instances = useRef(new Map<number, Instance>());
  const ownerRef = useRef<string | null>(worktreeId); ownerRef.current = worktreeId;
  const dragRef = useRef<{ startY: number; startHeight: number } | null>(null);
  useEffect(() => {
    ownerRef.current = worktreeId;
    setActive(null);
    void registry.restore(worktreeId).catch(() => { /* 原目标错误已由全局registry展示。 */ });
    return () => { if (ownerRef.current === worktreeId) ownerRef.current = null; };
  }, [registry, worktreeId]);
  useEffect(() => {
    if (!tabs.some((tab) => tab.sessionId === active)) setActive(tabs[0]?.sessionId ?? null);
  }, [active, tabs]);
  const openSession = useCallback(async (cwd: string | null) => {
    const info = await registry.open(worktreeId, cwd);
    if (info && ownerRef.current === worktreeId) setActive(info.sessionId);
  }, [registry, worktreeId]);
  useEffect(() => registry.onOutput((event) => {
    instances.current.get(event.sessionId)?.terminal.write(event.data);
  }), [registry]);
  useEffect(() => registry.onExit((event) => {
    instances.current.get(event.sessionId)?.terminal.write(
      `\r\n\u001b[90m[${t("terminal.exited", { code: event.exitCode ?? 0 })}]\u001b[0m\r\n`,
    );
  }), [registry]);

  const attach = useCallback(
    (sessionId: number, container: HTMLDivElement | null) => {
      const existing = instances.current.get(sessionId);
      if (!container) {
        if (existing) {
          existing.observer?.disconnect();
          existing.terminal.dispose();
          instances.current.delete(sessionId);
        }
        return;
      }
      if (existing && existing.container === container) return;
      const terminal = new Terminal({
        fontFamily: codeFontStack(fontFamily),
        fontSize,
        cursorBlink: true,
        theme: terminalTheme(),
        scrollback: 2000,
        allowProposedApi: false,
      });
      const fit = new FitAddon();
      terminal.loadAddon(fit);
      terminal.open(container);
      const observer = new ResizeObserver(() => {
        try {
          fit.fit();
          void registry
            .resize(sessionId, terminal.rows, terminal.cols)
            .catch(() => undefined);
        } catch {
          // 容器尚未布局完成时忽略。
        }
      });
      observer.observe(container);
      terminal.onData((data) => {
        void registry.input(sessionId, data).catch(() => { /* 原目标已有错误。 */ });
      });
      void m7
        .terminalSnapshot(sessionId)
        .then((snapshot) => {
          if (instances.current.get(sessionId)?.terminal === terminal && snapshot.length > 0) terminal.write(snapshot);
        })
        .catch((caught) => registry.reportSessionFailure(sessionId, caught));
      instances.current.set(sessionId, { terminal, fit, container, observer });
      requestAnimationFrame(() => {
        try {
          fit.fit();
          void registry.resize(sessionId, terminal.rows, terminal.cols).catch(() => { /* 原目标已有错误。 */ });
          terminal.focus();
        } catch {
          // 忽略布局竞态。
        }
      });
    },
    [fontFamily, fontSize, registry],
  );

  // 字号或主题变化时更新所有实例。
  useEffect(() => {
    for (const instance of instances.current.values()) {
      instance.terminal.options.disableStdin = busy || registry.isClosing;
      instance.terminal.options.fontSize = fontSize;
      instance.terminal.options.fontFamily = codeFontStack(fontFamily);
      instance.terminal.options.theme = terminalTheme();
      try {
        instance.fit.fit();
      } catch {
        // 忽略。
      }
    }
  }, [fontSize, fontFamily, busy, registry, revision]);

  // F06/H9：关闭运行中进程前先确认；确认后由用户动作触发真正关闭。
  const [confirmCloseSession, setConfirmCloseSession] = useState<number | null>(null);

  const closeTab = useCallback(async (sessionId: number, confirmed = false) => {
    const tab = registry.list(worktreeId).find((item) => item.sessionId === sessionId);
    if (!tab) return;
    if (tab.running && !confirmed) { setConfirmCloseSession(sessionId); return; }
    setConfirmCloseSession(null);
    // 后端确实关闭后才移除页签和视图；失败保留原进程与错误。
    await registry.close(sessionId, confirmed).catch(() => { /* 原目标已有错误。 */ });
  }, [registry, worktreeId]);

  const startDrag = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      dragRef.current = { startY: event.clientY, startHeight: height };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [height],
  );

  const moveDrag = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      if (!drag) return;
      const next = Math.min(Math.max(drag.startHeight + (drag.startY - event.clientY), 120), window.innerHeight * 0.7);
      onHeightChange(Math.round(next));
    },
    [onHeightChange],
  );

  const activeTab = useMemo(() => tabs.find((tab) => tab.sessionId === active) ?? null, [tabs, active]);

  return (
    <div className="flex min-h-0 shrink-0 flex-col border-t border-panel-border bg-[var(--editor-background)]" style={{ height }}>
      <div
        className="h-1 shrink-0 cursor-row-resize bg-transparent hover:bg-[var(--focus-border)]"
        onPointerDown={startDrag}
        onPointerMove={moveDrag}
        onPointerUp={(event) => {
          dragRef.current = null;
          event.currentTarget.releasePointerCapture(event.pointerId);
        }}
      />
      <div className="flex h-8 shrink-0 items-center gap-1 border-b border-panel-border px-2 text-[12px]">
        <Codicon name="terminal" size={14} />
        <span className="shrink-0 font-semibold">{t("terminal.title")}</span>
        <span className="max-w-[40%] truncate text-descriptionforeground" title={worktreePath}>
          {worktreePath}
        </span>
        <div className="ml-2 flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
          {tabs.map((tab) => (
            <div
              key={tab.sessionId}
              className={`group flex h-6 shrink-0 items-center gap-1 rounded px-2 ${
                tab.sessionId === active ? "bg-[var(--list-activeSelectionBackground)] text-[var(--list-activeSelectionForeground)]" : "text-descriptionforeground hover:bg-[var(--list-hoverBackground)]"
              }`}
            >
              <button
                type="button"
                className="max-w-[160px] truncate"
                title={tab.cwd}
                onClick={() => setActive(tab.sessionId)}
              >
                {tab.title}
                {tab.running ? "" : ` · ${t("terminal.ended")}`}
              </button>
              <button
                type="button"
                className="opacity-60 hover:opacity-100"
                title={t("terminal.closeTab")}
                aria-label={t("terminal.closeTab")}
                onClick={() => void closeTab(tab.sessionId)}
              >
                <Codicon name="close" size={12} />
              </button>
              {confirmCloseSession === tab.sessionId ? (
                <span className="ml-1 flex items-center gap-1 text-[10.5px] text-[var(--editorwarning-foreground)]">
                  {t("terminal.closeRunningConfirm")}
                  <button type="button" className="toolbar-widget h-4 px-1" onClick={() => { setConfirmCloseSession(null); void closeTab(tab.sessionId, true); }}>
                    {t("terminal.closeRunningYes")}
                  </button>
                  <button type="button" className="toolbar-widget h-4 px-1" onClick={() => setConfirmCloseSession(null)}>
                    {t("common.cancel")}
                  </button>
                </span>
              ) : null}
            </div>
          ))}
        </div>
        <button
          type="button"
          className="icon-action size-[22px]"
          title={t("terminal.new")}
          aria-label={t("terminal.new")}
          disabled={starting || busy || registry.isClosing}
          onClick={() => void openSession(activeTab?.cwd ?? null)}
        >
          <Codicon name="add" size={14} />
        </button>
        <button
          type="button"
          className="icon-action size-[22px]"
          title={t("terminal.closePanel")}
          aria-label={t("terminal.closePanel")}
          onClick={onClose}
        >
          <Codicon name="close" size={14} />
        </button>
      </div>
      {busy ? <p role="status" className="px-3 py-1 text-[12px] text-descriptionforeground">{t("errors.m7.terminalWorktreeBusy")}</p> : null}
      {error ? (
        <div role="alert" className="mx-2 mt-1 rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1 text-[12px]">
          {formatAppError(error)}
        </div>
      ) : null}
      <div className="relative min-h-0 flex-1">
        {tabs.map((tab) => (
          <div
            key={tab.sessionId}
            className={`absolute inset-0 px-1 pt-1 ${tab.sessionId === active ? "" : "hidden"}`}
            ref={(node) => attach(tab.sessionId, node)}
          />
        ))}
        {tabs.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-[12px] text-descriptionforeground">
            <span>{t("terminal.empty")}</span>
            <button type="button" className="toolbar-widget h-6 px-2" disabled={starting || busy || registry.isClosing} onClick={() => void openSession(null)}>
              {t("terminal.new")}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
