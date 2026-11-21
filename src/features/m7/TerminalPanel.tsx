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
import { AppIcon } from "@/components/ui/app-icon";
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

/** 稳定每个页签的ref；父组件刷新不会重建xterm或抢走其他输入的焦点。 */
function TerminalViewport({ sessionId, active, attach }: {
  sessionId: number;
  active: boolean;
  attach: (sessionId: number, container: HTMLDivElement | null) => void;
}) {
  const containerRef = useCallback((node: HTMLDivElement | null) => attach(sessionId, node), [attach, sessionId]);
  return <div className={`absolute inset-0 px-1 pt-1 ${active ? "" : "hidden"}`} ref={containerRef} />;
}

export interface TerminalPanelProps {
  worktreeId: string;
  worktreePath: string;
  fontSize: number;
  fontFamily: string | null;
  registry: TerminalSessions;
  busy: boolean;
  visible?: boolean;
}

function terminalTheme(): Record<string, string> {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) => {
    const value = style.getPropertyValue(name).trim();
    return value.length > 0 ? value : fallback;
  };
  return {
    background: read("--surface-bg", "#1a1c20"),
    foreground: read("--text-primary", "#f1f3f6"),
    cursor: read("--accent", "#3b82f6"),
    selectionBackground: read("--accent-soft", "rgba(59, 130, 246, 0.25)"),
  };
}

export function TerminalPanel({
  worktreeId,
  worktreePath,
  fontSize,
  fontFamily,
  registry,
  busy,
  visible = true,
}: TerminalPanelProps) {
  const revision = useSyncExternalStore(registry.subscribe, registry.snapshot);
  const tabs = registry.list(worktreeId);
  const error = registry.errorFor(worktreeId);
  const starting = registry.isStarting(worktreeId);
  const [active, setActive] = useState<number | null>(null);
  const [themeRev, setThemeRev] = useState(0);
  const instances = useRef(new Map<number, Instance>());
  const fontOptions = useRef({ fontFamily, fontSize });
  fontOptions.current = { fontFamily, fontSize };
  const ownerRef = useRef<string | null>(worktreeId); ownerRef.current = worktreeId;
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
        fontFamily: codeFontStack(fontOptions.current.fontFamily),
        fontSize: fontOptions.current.fontSize,
        cursorBlink: true,
        theme: terminalTheme(),
        allowTransparency: false,
        scrollback: 2000,
        allowProposedApi: false,
      });
      const fit = new FitAddon();
      terminal.loadAddon(fit);
      terminal.open(container);
      terminal.onResize(({ rows, cols }) => {
        void registry.resize(sessionId, rows, cols).catch(() => { /* 原目标保留结构化错误。 */ });
      });
      const observer = new ResizeObserver(() => {
        try {
          fit.fit();
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
        if (instances.current.get(sessionId)?.terminal !== terminal) return;
        try {
          fit.fit();
        } catch {
          // 忽略布局竞态。
        }
      });
    },
    [registry],
  );

  // 监听外观模式 / 主题切换，立即刷新已打开终端的颜色（M13）。
  useEffect(() => {
    if (typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => setThemeRev((value) => value + 1));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-kind", "data-theme"] });
    return () => observer.disconnect();
  }, []);

  // 只有真正激活页签且面板可见时才聚焦；输出、刷新及其他输入不会重新夺取焦点。
  useEffect(() => {
    if (active === null || !visible) return;
    const frame = requestAnimationFrame(() => {
      const instance = instances.current.get(active);
      if (!instance) return;
      try {
        instance.fit.fit();
        instance.terminal.focus();
      } catch {
        // 页签在布局前被关闭。
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [active, visible, worktreeId]);

  // 字号、外观模式或主题变化时更新所有实例。
  useEffect(() => {
    for (const instance of instances.current.values()) {
      instance.terminal.options.disableStdin = busy || registry.isClosing;
      instance.terminal.options.fontSize = fontSize;
      instance.terminal.options.fontFamily = codeFontStack(fontFamily);
      instance.terminal.options.theme = terminalTheme();
      if (visible) {
        try {
          instance.fit.fit();
        } catch {
          // 忽略。
        }
      }
    }
  }, [fontSize, fontFamily, busy, registry, revision, themeRev, visible]);

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

  const activeTab = useMemo(() => tabs.find((tab) => tab.sessionId === active) ?? null, [tabs, active]);

  return (
    <div className="terminal-panel flex min-h-0 flex-1 flex-col bg-[var(--surface-bg)]">
      <div className="flex h-7 shrink-0 items-center gap-1.5 border-b border-[var(--border-subtle)] bg-[var(--surface-subtle)] px-2.5 text-[12px]">
        <span className="max-w-[36%] truncate text-[11.5px] text-[var(--text-muted)]" title={worktreePath}>
          {worktreePath}
        </span>
        <div className="ml-1 flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {tabs.map((tab) => (
            <div
              key={tab.sessionId}
              className={`group flex h-5 shrink-0 items-center gap-1 rounded-[5px] px-2 text-[11.5px] ${
                tab.sessionId === active
                  ? "bg-[var(--surface-bg)] text-[var(--text-primary)] border border-[var(--border-subtle)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
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
                <AppIcon name="close" size={11} />
              </button>
              {confirmCloseSession === tab.sessionId ? (
                <span className="ml-1 flex items-center gap-1 text-[10.5px] text-[var(--warn)]">
                  {t("terminal.closeRunningConfirm")}
                  <button type="button" className="revia-btn revia-btn-secondary h-4 px-1" onClick={() => { setConfirmCloseSession(null); void closeTab(tab.sessionId, true); }}>
                    {t("terminal.closeRunningYes")}
                  </button>
                  <button type="button" className="revia-btn revia-btn-secondary h-4 px-1" onClick={() => setConfirmCloseSession(null)}>
                    {t("common.cancel")}
                  </button>
                </span>
              ) : null}
            </div>
          ))}
        </div>
        <button
          type="button"
          className="mini-icon-btn"
          title={t("terminal.new")}
          aria-label={t("terminal.new")}
          disabled={starting || busy || registry.isClosing}
          onClick={() => void openSession(activeTab?.cwd ?? null)}
        >
          <AppIcon name="add" size={13} />
        </button>
      </div>
      {busy ? <p role="status" className="px-3 py-1 text-[12px] text-[var(--text-secondary)]">{t("errors.m7.terminalWorktreeBusy")}</p> : null}
      {error ? (
        <div role="alert" className="mx-2 mt-1 rounded border border-[color-mix(in_srgb,var(--danger)_32%,transparent)] px-2 py-1 text-[12px]">
          {formatAppError(error)}
        </div>
      ) : null}
      <div className="relative min-h-0 flex-1">
        {tabs.map((tab) => (
          <TerminalViewport
            key={tab.sessionId}
            sessionId={tab.sessionId}
            active={tab.sessionId === active}
            attach={attach}
          />
        ))}
        {tabs.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-[12px] text-[var(--text-secondary)]">
            <span>{t("terminal.empty")}</span>
            <button type="button" className="revia-btn revia-btn-secondary h-6 px-2" disabled={starting || busy || registry.isClosing} onClick={() => void openSession(null)}>
              {t("terminal.new")}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
