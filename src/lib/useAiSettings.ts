//! App 级 AI 设置控制器归属（M11）。
//!
//! 设置对话框会随打开/关闭挂载和卸载；控制器、串行写入队列与页面会话必须
//! 比某个对话框实例活得更久，否则关闭时排队的写入会失去所有者，重开后新
//! 输入也可能与旧保存交错。这里用 App 的 ref 保存唯一实例。

import { useRef } from "react";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { AiSettings } from "@/lib/aiSettings";
import { ipc, type SettingsPatch } from "@/lib/ipc";

export type SaveSettings = (patch: SettingsPatch) => Promise<AppErrorPayload | null>;

export function useAiSettings(save: SaveSettings): AiSettings {
  const saveRef = useRef(save);
  saveRef.current = save;
  const ref = useRef<AiSettings | null>(null);
  if (!ref.current) {
    ref.current = new AiSettings({
      transport: {
        status: () => ipc.aiStatus(),
        save: (patch) => saveRef.current(patch),
        setKey: (key, baseUrl) => ipc.aiSetKey(key, baseUrl),
        clearKey: (baseUrl) => ipc.aiClearKey(baseUrl),
        listModels: (baseUrl) => ipc.aiListModels(baseUrl),
      },
    });
  }
  return ref.current;
}
