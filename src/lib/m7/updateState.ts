import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
/** 后端持久状态决定下载/安装阶段；历史安装回执不能遮住后来可用的新版本。 */
export function updateCandidate(status: AppUpdateStatus | null) {
  const version = status?.available?.version ?? status?.downloadedVersion ?? null;
  return { version, downloaded: version !== null && status?.downloadedVersion === version,
    restartVersion: status?.restartRequired ? status.receipt?.version ?? null : null };
}
