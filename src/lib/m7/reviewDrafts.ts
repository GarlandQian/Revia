import { m7 } from "@/lib/m7/api";
import { registerM7Flusher } from "@/lib/m7/flush";
import { ReviewDraftSessions } from "@/lib/m7/reviewDraftSessions";
export const reviewDraftSessions = new ReviewDraftSessions({ save: m7.reviewDraftSave, remove: m7.reviewDraftDelete });
// 会话在面板隐藏后仍需冲刷；与App共享正常退出前的既有登记点。
registerM7Flusher(() => reviewDraftSessions.flush());
