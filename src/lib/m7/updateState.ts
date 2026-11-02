import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
/** 后端持久状态决定下载/安装阶段；历史安装回执不能遮住后来可用的新版本。 */
export function updateCandidate(status: AppUpdateStatus | null) {
  const version = status?.available?.version ?? status?.downloadedVersion ?? status?.pendingInstall?.version ?? null;
  return {
    version,
    downloaded: version !== null && status?.downloadedVersion === version,
    pending: version !== null && status?.pendingInstall?.version === version,
    restartVersion: status?.restartRequired ? status.receipt?.version ?? null : null,
  };
}

export type UpdateLifecycleIntent = "installUpdate" | "restartUpdate";

/**
 * Windows 安装会退出应用：安装前必须由用户明确确认“安装并重启”，取消不得执行。
 * macOS 保持“安装后再单独确认重启”，因此只有 Windows 的 install 需要确认。
 * 这个状态机由 App 的更新入口使用，Node 生命周期回归直接驱动它。
 */
export class UpdateInstallConfirmation {
  private pending: UpdateLifecycleIntent | null = null;
  private readonly windows: boolean;

  constructor(windows: boolean) {
    this.windows = windows;
  }

  request(intent: UpdateLifecycleIntent): { kind: "start"; intent: UpdateLifecycleIntent } | { kind: "confirm"; intent: UpdateLifecycleIntent } {
    if (this.windows && intent === "installUpdate") {
      this.pending = intent;
      return { kind: "confirm", intent };
    }
    return { kind: "start", intent };
  }

  confirm(): UpdateLifecycleIntent | null {
    const intent = this.pending;
    this.pending = null;
    return intent;
  }

  cancel(): void {
    this.pending = null;
  }

  get waiting(): boolean {
    return this.pending !== null;
  }
}
