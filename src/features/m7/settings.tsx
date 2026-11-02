/**
 * M7 设置页（主题、快捷键、GitHub、更新）。
 *
 * 这些页面挂在既有设置对话框里，仍走 `onChange` 的补丁保存，
 * 不在前端写 localStorage；主题与账号数据从 Rust 读取。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { AppUpdateMode } from "@/types/bindings/AppUpdateMode";
import type { GithubStatus } from "@/types/bindings/GithubStatus";
import type { ImportedTheme } from "@/types/bindings/ImportedTheme";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import { Codicon } from "@/components/ui/codicon";
import { formatAppError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { updateReasonText } from "@/lib/i18n/text";
import { m7 } from "@/lib/m7/api";
import { GithubDeviceLogin } from "./GithubDeviceLogin";
import {
  acceleratorOf,
  captureAccelerator,
  conflictFor,
  formatAccelerator,
  shortcutDefs,
  shortcutLabel,
  withShortcut,
  replaceShortcut,
} from "@/lib/shortcuts";

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[minmax(140px,220px)_1fr] items-start gap-3">
      <div>
        <div className="text-[12.5px] font-semibold">{label}</div>
        {hint ? <div className="text-[11.5px] text-descriptionforeground">{hint}</div> : null}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

// ---------------- 颜色主题 ----------------

export function M7ThemeSettings({
  themeLight,
  themeDark,
  onChange,
  onThemesChanged,
}: {
  themeLight: string;
  themeDark: string;
  onChange: (patch: { themeLight?: string; themeDark?: string }) => Promise<boolean>;
  onThemesChanged: () => Promise<void> | void;
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
        await m7.removeImportedTheme(id);
        await reload();
        await onThemesChanged();
        setError(null);
      } catch (caught) {
        setError(toAppError(caught));
      }
    },
    [onThemesChanged, reload],
  );

  const currentLight = themeLight.startsWith("imported-");
  const currentDark = themeDark.startsWith("imported-");

  return (
    <div className="space-y-4">
      <p className="text-[12px] text-descriptionforeground">{t("settings.m7.theme.hint")}</p>
      {error ? (
        <p role="alert" className="rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1 text-[12px]">
          {formatAppError(error)}
        </p>
      ) : null}
      <Row label={t("settings.m7.theme.light")}>
        <select className="settings-select" value={themeLight} onChange={(event) => void onChange({ themeLight: event.target.value })}>
          {themes
            .filter((theme) => theme.kind === "light")
            .map((theme) => (
              <option key={theme.id} value={theme.id}>
                {theme.label}
              </option>
            ))}
        </select>
        {currentLight ? <p className="mt-1 text-[11.5px] text-descriptionforeground">{t("settings.m7.theme.importedActive")}</p> : null}
      </Row>
      <Row label={t("settings.m7.theme.dark")}>
        <select className="settings-select" value={themeDark} onChange={(event) => void onChange({ themeDark: event.target.value })}>
          {themes
            .filter((theme) => theme.kind === "dark")
            .map((theme) => (
              <option key={theme.id} value={theme.id}>
                {theme.label}
              </option>
            ))}
        </select>
        {currentDark ? <p className="mt-1 text-[11.5px] text-descriptionforeground">{t("settings.m7.theme.importedActive")}</p> : null}
      </Row>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <span className="text-[12.5px] font-semibold">{t("settings.m7.theme.imported")}</span>
          <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void importTheme()}>
            {t("settings.m7.theme.import")}
          </button>
        </div>
        {themes.length === 0 ? (
          <p className="text-[11.5px] text-descriptionforeground">{t("settings.m7.theme.none")}</p>
        ) : (
          <ul className="space-y-1">
            {themes.map((theme) => (
              <li key={theme.id} className="flex items-center gap-2 rounded border border-panel-border px-2 py-1 text-[12px]">
                <span className="min-w-0 flex-1 truncate">
                  {theme.label} · {theme.sourceName}
                </span>
                <span className="shrink-0 text-descriptionforeground">
                  {theme.kind === "light" ? t("settings.m7.theme.kindLight") : t("settings.m7.theme.kindDark")}
                </span>
                <button
                  type="button"
                  className="icon-action size-5"
                  aria-label={t("common.delete")}
                  onClick={() => void remove(theme.id)}
                >
                  <Codicon name="trash" size={12} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

// ---------------- 快捷键 ----------------

export function M7ShortcutSettings({
  shortcuts,
  onChange,
}: {
  shortcuts: Record<string, string>;
  onChange: (next: Record<string, string>) => Promise<boolean>;
}) {
  const [capturing, setCapturing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const captureRef = useRef<string | null>(null);
  captureRef.current = capturing;

  // F17：冲突不再直接拒绝——先询问“确认替换”，用户确认后两个动作一起落盘。
  const [pendingReplace, setPendingReplace] = useState<{ action: string; accelerator: string; conflict: string } | null>(null);

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
        // 记录待确认替换；用户点击确认才改绑定。
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

  return (
    <div className="space-y-3">
      <p className="text-[12px] text-descriptionforeground">{t("settings.m7.shortcuts.hint")}</p>
      {error ? <p role="alert" className="text-[12px] text-[color:var(--gitdecoration-conflictingresourceforeground)]">{error}</p> : null}
      {pendingReplace ? (
        <div role="alertdialog" aria-modal="false" className="flex items-center gap-2 rounded border border-panel-border px-2 py-1.5 text-[12px]">
          <span className="min-w-0 flex-1">
            {t("settings.m7.shortcuts.confirmReplace", {
              action: shortcutLabel(pendingReplace.action as never),
              accelerator: formatAccelerator(pendingReplace.accelerator),
              conflict: shortcutLabel(pendingReplace.conflict as never),
            })}
          </span>
          <button
            type="button"
            className="toolbar-widget h-6 px-2 font-semibold"
            onClick={() => {
              const next = replaceShortcut(shortcuts, pendingReplace.action as never, pendingReplace.accelerator);
              void onChange(next);
              setPendingReplace(null);
              setError(null);
            }}
          >
            {t("settings.m7.shortcuts.replaceConfirm")}
          </button>
          <button type="button" className="toolbar-widget h-6 px-2" onClick={() => setPendingReplace(null)}>
            {t("common.cancel")}
          </button>
        </div>
      ) : null}
      <table className="w-full text-[12px]">
        <tbody>
          {shortcutDefs.map((def) => {
            const active = shortcuts[def.id] ?? def.defaultAccelerator;
            return (
              <tr key={def.id} className="border-b border-panel-border last:border-0">
                <td className="py-1.5 pr-2">
                  {shortcutLabel(def.id)}
                  {def.scope === "diff" ? <span className="ml-1 text-descriptionforeground">{t("settings.m7.shortcuts.diffOnly")}</span> : null}
                </td>
                <td className="w-[180px] py-1.5 text-right">
                  <button
                    type="button"
                    className="toolbar-widget h-6 min-w-[92px] px-2 font-mono"
                    onClick={() => setCapturing(def.id)}
                  >
                    {capturing === def.id ? t("settings.m7.shortcuts.press") : formatAccelerator(active)}
                  </button>
                </td>
                <td className="w-[92px] py-1.5 text-right">
                  <button
                    type="button"
                    className="toolbar-widget h-6 px-2"
                    disabled={shortcuts[def.id] === undefined}
                    onClick={() => {
                      const conflict = conflictFor(def.defaultAccelerator, def.id, shortcuts);
                      if (conflict) setPendingReplace({ action: def.id, accelerator: def.defaultAccelerator, conflict });
                      else void onChange(withShortcut(shortcuts, def.id, def.defaultAccelerator));
                    }}
                  >
                    {t("settings.m7.shortcuts.reset")}
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="text-[11.5px] text-descriptionforeground">
        {t("settings.m7.shortcuts.current", {
          list: shortcutDefs.map((def) => `${shortcutLabel(def.id)}=${formatAccelerator(acceleratorOf(def.id))}`).join("，"),
        })}
      </p>
      <button type="button" className="toolbar-widget h-6 px-2" onClick={() => void onChange({})}>
        {t("settings.m7.shortcuts.resetAll")}
      </button>
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
      <p className="text-[12px] text-descriptionforeground">{t("settings.m7.github.hint")}</p>
      {error ? (
        <p role="alert" className="rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1 text-[12px]">
          {formatAppError(error)}
        </p>
      ) : null}
      {status && status.accounts.length > 0 ? (
        <ul className="space-y-1">
          {status.accounts.map((account) => (
            <li key={account.login} className="flex items-center gap-2 rounded border border-panel-border px-2 py-1 text-[12px]">
              <span className="min-w-0 flex-1 truncate">{account.login}</span>
              {status.repoAccount === account.login ? <span className="text-descriptionforeground">{t("settings.m7.github.bound")}</span> : null}
              {worktreeId && status.repoAccount !== account.login ? (
                <button
                  type="button"
                  className="toolbar-widget h-6 px-2"
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
                className="toolbar-widget h-6 px-2"
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
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-[12px] text-descriptionforeground">{t("settings.m7.github.none")}</p>
      )}
      {status?.deviceFlowConfigured ? (
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
      ) : null}
      <div className="space-y-2">
        <span className="text-[12.5px] font-semibold">{t("settings.m7.github.token")}</span>
        <input
          className="settings-input"
          type="password"
          value={token}
          spellCheck={false}
          placeholder={t("settings.m7.github.tokenPlaceholder")}
          onChange={(event) => setToken(event.target.value)}
        />
        <button type="button" className="toolbar-widget h-6 px-2" disabled={busy || deviceBusy || token.trim().length === 0} onClick={() => void login()}>
          {t("settings.m7.github.login")}
        </button>
        <p className="text-[11.5px] text-descriptionforeground">{t("settings.m7.github.tokenHint")}</p>
      </div>
      {status && !status.deviceFlowConfigured ? (
        <p className="text-[11.5px] text-descriptionforeground">{t("settings.m7.github.deviceUnconfigured")}</p>
      ) : null}
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
}: {
  mode: AppUpdateMode;
  onChangeMode: (mode: AppUpdateMode) => Promise<boolean>;
  /** F22：预发布渠道开关（用户主动开启；默认稳定版）。 */
  allowPrerelease: boolean;
  onChangeAllowPrerelease: (value: boolean) => Promise<boolean>;
  status: AppUpdateStatus | null;
  onStatusChange: (status: AppUpdateStatus) => void;
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
      <p className="text-[12px] text-descriptionforeground">{t("settings.m7.update.hint")}</p>
      {error ? (
        <p role="alert" className="rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] px-2 py-1 text-[12px]">
          {formatAppError(error)}
        </p>
      ) : null}
      <Row label={t("settings.m7.update.mode")}>
        <select className="settings-select" value={mode} onChange={(event) => void onChangeMode(event.target.value as AppUpdateMode)}>
          {UPDATE_MODES.map((candidate) => (
            <option key={candidate} value={candidate}>
              {updateModeLabel(candidate)}
            </option>
          ))}
        </select>
      </Row>
      <Row label={t("settings.m7.update.prerelease")}>
        <label className="flex items-center gap-2 text-[12px]">
          <input
            type="checkbox"
            className="accent-[var(--focusborder)]"
            checked={allowPrerelease}
            onChange={(event) => void onChangeAllowPrerelease(event.target.checked)}
          />
          {t("settings.m7.update.prereleaseHint")}
        </label>
      </Row>
      <Row label={t("settings.m7.update.version")}>
        <div className="space-y-1 text-[12px]">
          <p>{status?.currentVersion ?? "—"}</p>
          {status && !status.configured ? (
            <p className="text-descriptionforeground">{status.unconfiguredReason ? updateReasonText(status.unconfiguredReason) : t("settings.m7.update.unconfigured")}</p>
          ) : null}
          {status?.portable ? <p className="text-descriptionforeground">{t("update.portableNotice")}</p> : null}
          {status?.lastCheckedAtMs ? (
            <p className="text-descriptionforeground">{t("settings.m7.update.lastChecked", { time: new Date(status.lastCheckedAtMs).toLocaleString() })}</p>
          ) : null}
          <button type="button" className="toolbar-widget h-6 px-2" disabled={busy} onClick={() => void check()}>
            {busy ? t("settings.m7.update.checking") : t("settings.m7.update.check")}
          </button>
        </div>
      </Row>
      {status?.available ? (
        <Row label={t("settings.m7.update.available")}>
          <div className="space-y-1 text-[12px]">
            <p className="font-semibold">{status.available.version}</p>
            {status.available.notes ? <pre className="max-h-32 overflow-auto whitespace-pre-wrap text-[11.5px]">{status.available.notes}</pre> : null}
          </div>
        </Row>
      ) : null}
      {status?.receipt ? (
        <Row label={t("settings.m7.update.receipt")}>
          <p className="text-[12px]">
            {t("settings.m7.update.receiptValue", {
              version: status.receipt.version,
              time: new Date(status.receipt.installedAtMs).toLocaleString(),
            })}
          </p>
        </Row>
      ) : null}
    </div>
  );
}
