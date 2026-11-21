/**
 * M7 设置页（自定义代码颜色主题、快捷键、GitHub 账号、应用更新）。
 * M13 重构为统一的卡片行、现代 Switch 开关、键帽快捷键捕获与 revia-btn 按钮。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { AppUpdateMode } from "@/types/bindings/AppUpdateMode";
import type { GithubStatus } from "@/types/bindings/GithubStatus";
import type { ImportedTheme } from "@/types/bindings/ImportedTheme";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import { AppIcon } from "@/components/ui/app-icon";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { updateReasonText } from "@/lib/i18n/text";
import { m7 } from "@/lib/m7/api";
import { GithubDeviceLogin } from "./GithubDeviceLogin";
import {
  captureAccelerator,
  conflictFor,
  formatAccelerator,
  shortcutDefs,
  shortcutLabel,
  withShortcut,
  replaceShortcut,
} from "@/lib/shortcuts";

// ---------------- 导入的 VS Code 代码颜色主题 ----------------

export function M7ThemeSettings({
  onThemesChanged,
}: {
  onThemesChanged: (updated?: { themeLight?: string; themeDark?: string }) => Promise<void> | void;
}) {
  const [themes, setThemes] = useState<ImportedTheme[]>([]);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  const reload = useCallback(async () => {
    try {
      setThemes(await m7.listImportedThemes());
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const importTheme = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      title: t("settings.m7.theme.import"),
      filters: [{ name: t("settings.m7.theme.filter"), extensions: ["json", "jsonc", "code-theme"] }],
    });
    if (typeof picked !== "string") return;
    try {
      await m7.importTheme(picked, null, null);
      await reload();
      await onThemesChanged();
      setError(null);
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [onThemesChanged, reload]);

  const remove = useCallback(
    async (id: string) => {
      try {
        const updated = await m7.removeImportedTheme(id);
        await reload();
        await onThemesChanged({ themeLight: updated.themeLight, themeDark: updated.themeDark });
        setError(null);
      } catch (caught) {
        setError(toAppError(caught));
      }
    },
    [onThemesChanged, reload],
  );

  return (
    <div className="setting-card-list">
      <div className="setting-row">
        <div className="min-w-0">
          <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{t("settings.m7.theme.imported")}</div>
          <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("settings.m7.theme.hint")}</div>
        </div>
        <button type="button" className="revia-btn revia-btn-secondary shrink-0" onClick={() => void importTheme()}>
          <AppIcon name="add" size={13} />
          <span>{t("settings.m7.theme.import")}</span>
        </button>
      </div>
      {error ? (
        <div className="setting-row-stack">
          <p role="alert" className="rounded-[6px] border border-[var(--danger)]/35 bg-[var(--danger-soft)] px-2.5 py-1.5 text-[12px] text-[var(--danger)]">
            {formatAppError(error)}
          </p>
        </div>
      ) : null}
      {themes.length === 0 ? (
        <div className="setting-row">
          <span className="text-[12px] text-[var(--text-muted)]">{t("settings.m7.theme.none")}</span>
        </div>
      ) : (
        themes.map((theme) => (
          <div key={theme.id} className="setting-row">
            <div className="min-w-0 flex-1">
              <div className="truncate text-[12.5px] font-medium text-[var(--text-primary)]">{theme.label}</div>
              <div className="truncate text-[11.5px] text-[var(--text-muted)]">{theme.sourceName}</div>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <span className="count-pill">
                {theme.kind === "light" ? t("settings.m7.theme.kindLight") : t("settings.m7.theme.kindDark")}
              </span>
              <button
                type="button"
                className="mini-icon-btn danger"
                aria-label={t("common.delete")}
                title={t("common.delete")}
                onClick={() => void remove(theme.id)}
              >
                <AppIcon name="trash" size={13} />
              </button>
            </div>
          </div>
        ))
      )}
    </div>
  );
}

// ---------------- 快捷键 ----------------

export function M7ShortcutSettings({
  shortcuts,
  onChange,
  onCapturingChange,
}: {
  shortcuts: Record<string, string>;
  onChange: (next: Record<string, string>) => Promise<boolean>;
  onCapturingChange?: (capturing: boolean) => void;
}) {
  const [capturing, setCapturing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const captureRef = useRef<string | null>(null);
  captureRef.current = capturing;

  const [pendingReplace, setPendingReplace] = useState<{ action: string; accelerator: string; conflict: string } | null>(null);

  const onCapturingChangeRef = useRef(onCapturingChange);
  onCapturingChangeRef.current = onCapturingChange;

  useEffect(() => {
    onCapturingChangeRef.current?.(capturing !== null);
    return () => onCapturingChangeRef.current?.(false);
  }, [capturing]);

  useEffect(() => {
    if (capturing === null) return;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setCapturing(null);
        setError(null);
        return;
      }
      const captured = captureAccelerator(event);
      if (!captured) return;
      const conflict = conflictFor(captured, capturing as never, shortcuts);
      if (conflict) {
        setPendingReplace({ action: capturing, accelerator: captured, conflict });
        setCapturing(null);
        return;
      }
      setError(null);
      setCapturing(null);
      void onChange(withShortcut(shortcuts, capturing as never, captured));
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, onChange, shortcuts]);

  const hasCustom = Object.keys(shortcuts).length > 0;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-[12px] text-[var(--text-secondary)]">{t("settings.m7.shortcuts.hint")}</p>
        <button
          type="button"
          className="revia-btn revia-btn-secondary shrink-0"
          disabled={!hasCustom}
          onClick={() => void onChange({})}
        >
          {t("settings.m7.shortcuts.resetAll")}
        </button>
      </div>

      {error ? (
        <p role="alert" className="rounded-[6px] border border-[var(--danger)]/35 bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger)]">
          {error}
        </p>
      ) : null}

      {pendingReplace ? (
        <div
          role="alertdialog"
          aria-modal="false"
          className="flex items-center gap-2.5 rounded-[8px] border border-[var(--warn)]/35 bg-[var(--surface-subtle)] px-3.5 py-2.5 text-[12px]"
        >
          <span className="min-w-0 flex-1 text-[var(--text-primary)]">
            {t("settings.m7.shortcuts.confirmReplace", {
              action: shortcutLabel(pendingReplace.action as never),
              accelerator: formatAccelerator(pendingReplace.accelerator),
              conflict: shortcutLabel(pendingReplace.conflict as never),
            })}
          </span>
          <button
            type="button"
            className="revia-btn revia-btn-primary h-7 px-2.5"
            onClick={() => {
              const next = replaceShortcut(shortcuts, pendingReplace.action as never, pendingReplace.accelerator);
              void onChange(next);
              setPendingReplace(null);
              setError(null);
            }}
          >
            {t("settings.m7.shortcuts.replaceConfirm")}
          </button>
          <button type="button" className="revia-btn revia-btn-secondary h-7 px-2.5" onClick={() => setPendingReplace(null)}>
            {t("common.cancel")}
          </button>
        </div>
      ) : null}

      <div className="setting-card-list">
        {shortcutDefs.map((def) => {
          const active = shortcuts[def.id] ?? def.defaultAccelerator;
          const isCustom = shortcuts[def.id] !== undefined;
          return (
            <div key={def.id} className="setting-row py-2.5">
              <div className="min-w-0 flex-1">
                <span className="text-[12.5px] font-medium text-[var(--text-primary)]">{shortcutLabel(def.id)}</span>
                {def.scope === "diff" ? (
                  <span className="ml-2 text-[11px] text-[var(--text-muted)]">{t("settings.m7.shortcuts.diffOnly")}</span>
                ) : null}
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  className="kbd-cap"
                  data-capturing={capturing === def.id}
                  onClick={() => setCapturing(def.id)}
                >
                  {capturing === def.id ? t("settings.m7.shortcuts.press") : formatAccelerator(active)}
                </button>
                <button
                  type="button"
                  className="mini-icon-btn"
                  title={t("settings.m7.shortcuts.reset")}
                  aria-label={t("settings.m7.shortcuts.reset")}
                  disabled={!isCustom}
                  onClick={() => {
                    const conflict = conflictFor(def.defaultAccelerator, def.id, shortcuts);
                    if (conflict) setPendingReplace({ action: def.id, accelerator: def.defaultAccelerator, conflict });
                    else void onChange(withShortcut(shortcuts, def.id, def.defaultAccelerator));
                  }}
                >
                  <AppIcon name="discard" size={12} />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <p className="text-[11px] leading-relaxed text-[var(--text-muted)]">
        {t("settings.m7.shortcuts.current", {
          list: shortcutDefs.map((def) => `${shortcutLabel(def.id)}=${formatAccelerator(shortcuts[def.id] ?? def.defaultAccelerator)}`).join("，"),
        })}
      </p>
    </div>
  );
}

// ---------------- GitHub 账号 ----------------

export function M7GithubSettings({
  worktreeId,
  onChangeAccount,
}: {
  worktreeId: string | null;
  onChangeAccount: () => void;
}) {
  const [status, setStatus] = useState<GithubStatus | null>(null);
  const [token, setToken] = useState("");
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [busy, setBusy] = useState(false);
  const [deviceBusy, setDeviceBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      setStatus(await m7.githubStatus(worktreeId));
    } catch (caught) {
      setError(toAppError(caught));
    }
  }, [worktreeId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const login = useCallback(async () => {
    setBusy(true);
    try {
      await m7.githubLoginToken(token.trim());
      setToken("");
      window.dispatchEvent(new Event("revia-github-account-changed"));
      await reload();
      onChangeAccount();
      setError(null);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(false);
    }
  }, [onChangeAccount, reload, token]);

  return (
    <div className="space-y-4">
      <p className="text-[12px] text-[var(--text-secondary)]">{t("settings.m7.github.hint")}</p>
      {error ? (
        <p role="alert" className="rounded-[6px] border border-[var(--danger)]/35 bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger)]">
          {formatAppError(error)}
        </p>
      ) : null}

      <div className="setting-card-list">
        {status && status.accounts.length > 0 ? (
          status.accounts.map((account) => (
            <div key={account.login} className="setting-row">
              <div className="flex min-w-0 items-center gap-2.5">
                <span className="grid size-7 shrink-0 place-items-center rounded-full bg-[var(--accent-soft)] text-[11.5px] font-semibold text-[var(--accent)]">
                  {account.login.slice(0, 2).toUpperCase()}
                </span>
                <div className="min-w-0">
                  <div className="truncate text-[12.5px] font-medium text-[var(--text-primary)]">{account.login}</div>
                  {status.repoAccount === account.login ? (
                    <div className="text-[11px] text-[var(--reviewed)]">{t("settings.m7.github.bound")}</div>
                  ) : null}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                {worktreeId && status.repoAccount !== account.login ? (
                  <button
                    type="button"
                    className="revia-btn revia-btn-secondary h-7 px-2.5"
                    onClick={() =>
                      void m7
                        .githubBindRepo(worktreeId, account.login)
                        .then(() => {
                          window.dispatchEvent(new Event("revia-github-account-changed"));
                          onChangeAccount();
                          return reload();
                        })
                        .catch((caught) => setError(toAppError(caught)))
                    }
                  >
                    {t("settings.m7.github.bind")}
                  </button>
                ) : null}
                <button
                  type="button"
                  className="revia-btn revia-btn-secondary h-7 px-2.5 text-[var(--danger)]"
                  onClick={() =>
                    void m7
                      .githubLogout(account.login)
                      .then(async () => {
                        window.dispatchEvent(new Event("revia-github-account-changed"));
                        await reload();
                        onChangeAccount();
                      })
                      .catch((caught) => setError(toAppError(caught)))
                  }
                >
                  {t("settings.m7.github.logout")}
                </button>
              </div>
            </div>
          ))
        ) : (
          <div className="setting-row">
            <span className="text-[12px] text-[var(--text-muted)]">{t("settings.m7.github.none")}</span>
          </div>
        )}
      </div>

      {status?.deviceFlowConfigured ? (
        <div className="setting-card">
          <GithubDeviceLogin
            key={worktreeId ?? "no-worktree"}
            disabled={busy}
            onBusyChange={setDeviceBusy}
            onAuthorized={() => {
              setError(null);
              void reload();
              onChangeAccount();
            }}
          />
        </div>
      ) : null}

      <div className="setting-card space-y-2.5">
        <div className="text-[12.5px] font-semibold text-[var(--text-primary)]">{t("settings.m7.github.token")}</div>
        <div className="flex items-center gap-2">
          <input
            className="settings-input min-w-0 flex-1"
            type="password"
            value={token}
            spellCheck={false}
            placeholder={t("settings.m7.github.tokenPlaceholder")}
            onChange={(event) => setToken(event.target.value)}
          />
          <button
            type="button"
            className="revia-btn revia-btn-primary shrink-0"
            disabled={busy || deviceBusy || token.trim().length === 0}
            onClick={() => void login()}
          >
            {t("settings.m7.github.login")}
          </button>
        </div>
        <p className="text-[11.5px] text-[var(--text-muted)]">{t("settings.m7.github.tokenHint")}</p>
        {status && !status.deviceFlowConfigured ? (
          <p className="text-[11.5px] text-[var(--text-muted)]">{t("settings.m7.github.deviceUnconfigured")}</p>
        ) : null}
      </div>
    </div>
  );
}

// ---------------- 应用更新 ----------------

const UPDATE_MODES: AppUpdateMode[] = ["manual", "notify"];

function updateModeLabel(mode: AppUpdateMode): string {
  switch (mode) {
    case "manual":
      return t("settings.m7.update.modeManual");
    case "notify":
      return t("settings.m7.update.modeNotify");
    default:
      return mode;
  }
}

export function M7UpdateSettings({
  mode,
  onChangeMode,
  allowPrerelease,
  onChangeAllowPrerelease,
  status,
  onStatusChange,
  onOpenUpdatePanel,
}: {
  mode: AppUpdateMode;
  onChangeMode: (mode: AppUpdateMode) => Promise<boolean>;
  allowPrerelease: boolean;
  onChangeAllowPrerelease: (value: boolean) => Promise<boolean>;
  status: AppUpdateStatus | null;
  onStatusChange: (status: AppUpdateStatus) => void;
  onOpenUpdatePanel?: () => void;
}) {
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [busy, setBusy] = useState(false);

  const check = useCallback(async () => {
    setBusy(true);
    try {
      onStatusChange(await m7.updateCheck(true));
      setError(null);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(false);
    }
  }, [onStatusChange]);

  return (
    <div className="space-y-4">
      {error ? (
        <p role="alert" className="rounded-[6px] border border-[var(--danger)]/35 bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger)]">
          {formatAppError(error)}
        </p>
      ) : null}

      <div className="setting-card-list">
        <div className="setting-row">
          <div className="min-w-0">
            <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{t("settings.m7.update.version")}</div>
            <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">
              {status?.currentVersion ?? "—"}
              {status?.lastCheckedAtMs ? ` · ${t("settings.m7.update.lastChecked", { time: new Date(status.lastCheckedAtMs).toLocaleString() })}` : ""}
            </div>
            {status && !status.configured ? (
              <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">
                {status.unconfiguredReason ? updateReasonText(status.unconfiguredReason) : t("settings.m7.update.unconfigured")}
              </div>
            ) : null}
            {status?.portable ? (
              <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("update.portableNotice")}</div>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {onOpenUpdatePanel ? (
              <button
                type="button"
                className={status?.available || status?.restartRequired ? "revia-btn revia-btn-primary shrink-0" : "revia-btn revia-btn-secondary shrink-0"}
                onClick={onOpenUpdatePanel}
              >
                <AppIcon name="cloud-download" size={13} />
                <span>{t("update.bannerOpen")}</span>
              </button>
            ) : null}
            <button type="button" className="revia-btn revia-btn-secondary shrink-0" disabled={busy} onClick={() => void check()}>
              <AppIcon name={busy ? "loading" : "refresh"} size={13} spin={busy} />
              <span>{busy ? t("settings.m7.update.checking") : t("settings.m7.update.check")}</span>
            </button>
          </div>
        </div>

        <div className="setting-row">
          <div className="min-w-0">
            <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{t("settings.m7.update.mode")}</div>
            <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("settings.m7.update.hint")}</div>
          </div>
          <select className="settings-select w-[190px]" value={mode} onChange={(event) => void onChangeMode(event.target.value as AppUpdateMode)}>
            {UPDATE_MODES.map((candidate) => (
              <option key={candidate} value={candidate}>
                {updateModeLabel(candidate)}
              </option>
            ))}
          </select>
        </div>

        <div className="setting-row">
          <div className="min-w-0">
            <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{t("settings.m7.update.prerelease")}</div>
            <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("settings.m7.update.prereleaseHint")}</div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={allowPrerelease}
            aria-label={t("settings.m7.update.prerelease")}
            className="setting-switch"
            onClick={() => void onChangeAllowPrerelease(!allowPrerelease)}
          />
        </div>

        {status?.available ? (
          <div className="setting-row-stack">
            <div className="text-[12.5px] font-semibold text-[var(--accent)]">
              {t("settings.m7.update.available")}: {status.available.version}
            </div>
            {status.available.notes ? (
              <pre className="max-h-32 overflow-auto rounded-[6px] border border-[var(--border-subtle)] bg-[var(--surface-bg)] p-2.5 text-[11.5px] whitespace-pre-wrap text-[var(--text-secondary)]">
                {status.available.notes}
              </pre>
            ) : null}
          </div>
        ) : null}

        {status?.receipt ? (
          <div className="setting-row">
            <div className="text-[12px] text-[var(--text-secondary)]">
              {t("settings.m7.update.receipt")}:{" "}
              {t("settings.m7.update.receiptValue", {
                version: status.receipt.version,
                time: new Date(status.receipt.installedAtMs).toLocaleString(),
              })}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
