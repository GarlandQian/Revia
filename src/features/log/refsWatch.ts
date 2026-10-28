/**
 * 引用变化检测（M5 L01、审查第 7 项）。
 *
 * 日志不再自己定期读引用：文件监视（`watch.rs`，Q88）已经覆盖 refs、packed-refs 与
 * HEAD，界面在监视事件、手动刷新或前台核对后读取 `history_refs`，签名变化才重查日志。
 * 这样后台不会持续执行 Git，也不会把命令日志的 1000 条容量填满。
 *
 * 纯模块：`tools/test-log-model.mjs` 直接测签名比较。
 */

import type { HistoryRef } from "@/types/bindings/HistoryRef";

/** 引用快照的规范签名（引用名、指向的提交与当前分支）。 */
export function refsSignature(refs: HistoryRef[]): string {
  return refs
    .map((ref) => `${ref.fullName}@${ref.commit}:${ref.current ? "current" : ""}`)
    .sort()
    .join("|");
}

/** 记住上一次看到的引用签名，判断一次监视事件是否真的改了引用。 */
export class RefsWatch {
  private signature = "";

  /** 观察一份引用快照：有变化返回 true（需要重查日志），否则 false。 */
  observe(refs: HistoryRef[]): boolean {
    const next = refsSignature(refs);
    if (next === this.signature) return false;
    this.signature = next;
    return true;
  }

  /** 换目标时清空：新目标的第一份快照算一次变化。 */
  reset(): void {
    this.signature = "";
  }
}
