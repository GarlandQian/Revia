/**
 * 内置终端（M7 任务 04、Q178 / Q196）。
 *
 * Rust 侧创建 PTY（登录 shell），这里只做三件事：
 * 把用户按键按字节发给 PTY、把输出写进 xterm、把尺寸变化回传。
 * 不在前端拼命令、不保存输出；重启只恢复页签与起始目录并新开 shell。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { TerminalSessionInfo } from "@/types/bindings/TerminalSessionInfo";
import type { TerminalTabRecord } from "@/types/bindings/TerminalTabRecord";
import { Codicon } from "@/components/ui/codicon";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7, onM7Events } from "@/lib/m7/api";
import { registerM7Flusher } from "@/lib/m7/flush";
import { codeFontStack } from "@/lib/theme";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";

interface Tab {
  sessionId: number;
  worktreeId: string;
  title: string;
  cwd: string;
  running: boolean;
  exitCode: number | null;
}

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
  /** 终端输出静默后触发外部核对（工作区可能被命令改过）。 */
  onActivity: () => void;
  /** F05/F13：本面板管理的会话中是否存在运行中的进程（更新安装 / 关闭协调用）。 */
  onRunningChange?: (running: boolean) => void;
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
  onActivity,
  onRunningChange,
  onClose,
}: TerminalPanelProps) {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [starting, setStarting] = useState(false);
  const instances = useRef(new Map<number, Instance>());
  const tabsRef = useRef<Tab[]>([]);
  tabsRef.current = tabs;
  const otherTabs = useRef<TerminalTabRecord[]>([]);
  const activityTimer = useRef<number | null>(null);
  const dragRef = useRef<{ startY: number; startHeight: number } | null>(null);

  const persistTabs = useCallback(async () => {
    const records: TerminalTabRecord[] = [
      ...otherTabs.current,
      ...tabsRef.current.map((tab, index) => ({
        worktreeId: tab.worktreeId,
        cwd: tab.cwd,
        title: tab.title,
        order: index,
      })),
    ];
    await m7.terminalSaveTabs(records).catch(() => undefined);
  }, []);

  const openSession = useCallback(
    async (cwd: string | null) => {
      setStarting(true);
      try {
        const info: TerminalSessionInfo = await m7.terminalOpen({
          worktreeId,
          cwd,
          rows: 24,
          cols: 80,
        });
        const tab: Tab = {
          sessionId: info.sessionId,
          worktreeId: info.worktreeId,
          title: info.title,
          cwd: info.cwd,
          running: info.running,
          exitCode: info.exitCode ?? null,
        };
        setTabs((current) => [...current, tab]);
        setActive(info.sessionId);
        setError(null);
        return tab;
      } catch (caught) {
        setError(toAppError(caught));
        return null;
      } finally {
        setStarting(false);
      }
    },
    [worktreeId],
  );

  // F05：挂载 / 切 worktree 时，先认领后端仍在运行的属于本 worktree 的会话，
  // 再按持久化页签补新 shell。不重复 openSession 造成不可达进程。
  const claimOrOpen = useCallback(async (recordCwd: string | null) => {
    const live = await m7.terminalList().catch(() => [] as TerminalSessionInfo[]);
    const mine = live.filter((info) => info.worktreeId === worktreeId);
    const claimed = mine.find((info) => !tabsRef.current.some((tab) => tab.sessionId === info.sessionId));
    if (claimed) {
      const tab: Tab = {
        sessionId: claimed.sessionId,
        worktreeId: claimed.worktreeId,
        title: claimed.title,
        cwd: claimed.cwd,
        running: claimed.running,
        exitCode: claimed.exitCode ?? null,
      };
      setTabs((current) => [...current, tab]);
      setActive(tab.sessionId);
      return tab;
    }
    return openSession(recordCwd);
  }, [openSession, worktreeId]);

  // 首次打开：恢复本 worktree 的页签（新 shell，不恢复旧进程）；运行中的进程被重新认领。
  useEffect(() => {
    let cancelled = false;
    // F05：换 worktree 先清掉旧页签状态，避免 A 的页签显示在 B 的标题下。
    // 只销毁前端 xterm 视图，后端 PTY 会话保留（重新打开时由 claimOrOpen 认领）。
    setTabs([]);
    setActive(null);
    instances.current.forEach((instance) => {
      instance.observer?.disconnect();
      instance.terminal.dispose();
    });
    instances.current.clear();
    void (async () => {
      const stored = await m7.terminalTabs().catch(() => [] as TerminalTabRecord[]);
      if (cancelled) return;
      otherTabs.current = stored.filter((record) => record.worktreeId !== worktreeId);
      const mine = stored
        .filter((record) => record.worktreeId === worktreeId)
        .sort((left, right) => left.order - right.order);
      if (mine.length === 0) {
        await claimOrOpen(null);
        return;
      }
      for (const record of mine.slice(0, 6)) {
        if (cancelled) return;
        await claimOrOpen(record.cwd);
      }
    })();
    return () => {
      cancelled = true;
    };
    // 只按 worktree 恢复一次；页签变化由后续操作保存。
  }, [worktreeId, claimOrOpen]);

  useEffect(() => {
    void persistTabs();
  }, [tabs, persistTabs]);

  // F05/F13：把“是否有运行中的会话”同步给外层（更新安装 / 关闭协调读取）。
  const anyRunning = tabs.some((tab) => tab.running);
  useEffect(() => {
    onRunningChange?.(anyRunning);
  }, [anyRunning, onRunningChange]);

  // F05：写入期间显示所属 worktree 忙碌（写锁被 Git 写操作持有时禁用输入）。

  // 关闭前保存页签（不保存输出，也不恢复进程）。
  useEffect(() => registerM7Flusher(persistTabs), [persistTabs]);

  // 后端输出与退出事件。
  useEffect(() => {
    return onM7Events({
      onTerminalOutput: (event) => {
        const instance = instances.current.get(event.sessionId);
        if (instance) instance.terminal.write(event.data);
        if (activityTimer.current !== null) window.clearTimeout(activityTimer.current);
        activityTimer.current = window.setTimeout(() => {
          activityTimer.current = null;
          onActivity();
        }, 900);
      },
      onTerminalExit: (event) => {
        setTabs((current) =>
          current.map((tab) =>
            tab.sessionId === event.sessionId
              ? { ...tab, running: false, exitCode: event.exitCode ?? null }
              : tab,
          ),
        );
        const instance = instances.current.get(event.sessionId);
        instance?.terminal.write(
          `\r\n\u001b[90m[${t("terminal.exited", { code: event.exitCode ?? 0 })}]\u001b[0m\r\n`,
        );
        onActivity();
      },
    });
  }, [onActivity]);

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
          void m7
            .terminalResize(sessionId, terminal.rows, terminal.cols)
            .catch(() => undefined);
        } catch {
          // 容器尚未布局完成时忽略。
        }
      });
      observer.observe(container);
      terminal.onData((data) => {
        void m7.terminalWrite(sessionId, data).catch(() => undefined);
      });
      void m7
        .terminalSnapshot(sessionId)
        .then((snapshot) => {
          if (snapshot.length > 0) terminal.write(snapshot);
        })
        .catch(() => undefined);
      instances.current.set(sessionId, { terminal, fit, container, observer });
      requestAnimationFrame(() => {
        try {
          fit.fit();
          void m7.terminalResize(sessionId, terminal.rows, terminal.cols).catch(() => undefined);
          terminal.focus();
        } catch {
          // 忽略布局竞态。
        }
      });
    },
    [fontFamily, fontSize],
  );

  // 字号或主题变化时更新所有实例。
  useEffect(() => {
    for (const instance of instances.current.values()) {
      instance.terminal.options.fontSize = fontSize;
      instance.terminal.options.fontFamily = codeFontStack(fontFamily);
      instance.terminal.options.theme = terminalTheme();
      try {
        instance.fit.fit();
      } catch {
        // 忽略。
      }
    }
  }, [fontSize, fontFamily]);

  // F06/H9：关闭运行中进程前先确认；确认后由用户动作触发真正关闭。
  const [confirmCloseSession, setConfirmCloseSession] = useState<number | null>(null);

  const closeTab = useCallback(
    async (sessionId: number) => {
      const tab = tabsRef.current.find((item) => item.sessionId === sessionId);
      if (tab?.running && confirmCloseSession !== sessionId) {
        setConfirmCloseSession(sessionId);
        return;
      }
      setConfirmCloseSession(null);
      const instance = instances.current.get(sessionId);
      if (instance) {
        instance.observer?.disconnect();
        instance.terminal.dispose();
        instances.current.delete(sessionId);
      }
      await m7.terminalClose(sessionId).catch(() => undefined);
      setTabs((current) => {
        const next = current.filter((tab) => tab.sessionId !== sessionId);
        if (active === sessionId) setActive(next.length > 0 ? next[next.length - 1].sessionId : null);
        return next;
      });
    },
    [active, confirmCloseSession],
  );

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
                  <button type="button" className="toolbar-widget h-4 px-1" onClick={() => { setConfirmCloseSession(null); void closeTab(tab.sessionId); }}>
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
          disabled={starting}
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
            <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void openSession(null)}>
              {t("terminal.new")}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
