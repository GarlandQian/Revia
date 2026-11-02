import { useEffect, useRef, useState } from "react";
import type { GithubAccount } from "@/types/bindings/GithubAccount";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError, localizedError, toAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { m7 } from "@/lib/m7/api";
import { GithubDeviceFlow, type DeviceFlowView } from "@/lib/m7/githubDeviceFlow";

export function GithubDeviceLogin({
  disabled = false,
  onAuthorized,
  onBusyChange,
}: {
  disabled?: boolean;
  onAuthorized: (account: GithubAccount) => void;
  onBusyChange: (busy: boolean) => void;
}) {
  const [view, setView] = useState<DeviceFlowView>({ phase: "idle" });
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [ended, setEnded] = useState<"expired" | "denied" | null>(null);
  const callbacks = useRef({ onAuthorized, onBusyChange });
  callbacks.current = { onAuthorized, onBusyChange };
  const [flow] = useState(() => new GithubDeviceFlow(m7, {
    changed(next) {
      setView(next);
      callbacks.current.onBusyChange(next.phase !== "idle");
    },
    authorized(account) {
      callbacks.current.onAuthorized(account);
      window.dispatchEvent(new Event("revia-github-account-changed"));
    },
    failed: (caught) => setError(toAppError(caught)),
    ended: setEnded,
  }));

  useEffect(() => () => {
    flow.cancel(false);
    callbacks.current.onBusyChange(false);
  }, [flow]);

  const copyCode = async (code: string) => {
    try {
      await navigator.clipboard.writeText(code);
      setError(null);
    } catch {
      setError(localizedError("environment", "commit.copyFailed"));
    }
  };

  return (
    <div className="space-y-2 rounded border border-panel-border p-3 text-[12.5px]">
      <p className="font-semibold">{t("github.deviceTitle")}</p>
      {error ? <p role="alert" className="whitespace-pre-wrap text-[var(--editorwarning-foreground)]">{formatAppError(error)}</p> : null}
      {ended ? <p role="alert">{ended === "expired" ? t("github.deviceExpired") : t("github.deviceDenied")}</p> : null}
      {view.phase === "waiting" ? (
        <div className="space-y-2">
          <p>{t("github.deviceCode")}</p>
          <p className="select-all font-mono text-[18px] tracking-widest">{view.device.userCode}</p>
          <p className="text-descriptionforeground">https://github.com/login/device</p>
          <div className="flex flex-wrap gap-2">
            <button type="button" className="toolbar-widget h-7 px-3" onClick={() => void copyCode(view.device.userCode)}>{t("github.deviceCopy")}</button>
            <button type="button" className="toolbar-widget h-7 px-3" onClick={() => {
              setError(null);
              void flow.openBrowser();
            }}>{t("github.deviceOpen")}</button>
            <button type="button" className="toolbar-widget h-7 px-3" onClick={() => flow.cancel()}>{t("github.deviceCancel")}</button>
          </div>
        </div>
      ) : (
        <div className="flex gap-2">
          <button type="button" className="toolbar-widget h-7 px-3" disabled={disabled || view.phase === "starting"} onClick={() => {
            setError(null);
            setEnded(null);
            void flow.start();
          }}>{view.phase === "starting" ? t("github.deviceStarting") : t("github.deviceStart")}</button>
          {view.phase === "starting" ? <button type="button" className="toolbar-widget h-7 px-3" onClick={() => flow.cancel()}>{t("github.deviceCancel")}</button> : null}
        </div>
      )}
    </div>
  );
}
