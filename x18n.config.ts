import { defineConfig } from "@x18n/core";

// Revia 界面文案（M6 L01/L02，Q163–Q171）：
// - 源语言 zh-CN，目标 en；
// - 保留既有语义分组与 camelCase 末级键（如 toolbar.chooseRepo）；
// - 资源为 JSON（locales/zh-CN.json 与 locales/en.json 由 x18n CLI 维护，不手改）；
// - 调试 / 测量日志不翻译（相关文件用最小范围 x18n-ignore 标注）；
// - 术语表见 docs/milestones/m6/i18n-proposal.md（Q170：Snapshot Before Commit）。
export default defineConfig({
  sourceLocale: "zh-CN",
  locales: ["zh-CN", "en"],
  resources: [
    {
      pattern: "locales/{locale}.json",
      format: "json",
    },
  ],
  usage: {
    include: ["src/**/*.{ts,tsx}"],
    calls: [{ callee: "t", kind: "call", key: { arg: 0 } }, { callee: "localizedError", kind: "call", key: { arg: 1 } }, { callee: "at", kind: "call", key: { arg: 1 } }],
    regex: [
      {
        extensions: ["ts", "tsx"],
        patterns: ["messages\\.([A-Za-z0-9_.]+)"],
      },
    ],
    // Rust 错误按 IPC 载荷里的完整错误码查表；错误码不在前端写死。
    keysInUse: ["errors.**"],
  },
  extract: {
    ignore: [],
    // 诊断日志只在测量会话写入，不属于界面文案。
    ignoreCallees: ["debugLog", "reportCritical", "console.log", "console.warn", "console.error"],
  },
  glossary: [
    { term: "Revia", doNotTranslate: true },
    { term: "Git", doNotTranslate: true },
    { term: "Rebased", doNotTranslate: true },
    { term: "VS Code", doNotTranslate: true },
    { term: "x18n", doNotTranslate: true },
    { term: "HEAD", doNotTranslate: true },
    { term: "worktree", doNotTranslate: true },
    { term: "stash", doNotTranslate: true },
    { term: "rebase", doNotTranslate: true },
    { term: "cherry-pick", doNotTranslate: true },
    { term: "revert", doNotTranslate: true },
    { term: "reset", doNotTranslate: true },
    { term: "blame", doNotTranslate: true },
    { term: "diff", doNotTranslate: true },
    { term: "hook", doNotTranslate: true },
    { term: "fetch", doNotTranslate: true },
    { term: "push", doNotTranslate: true },
    {
      term: "Snapshot Before Commit",
      translations: { "zh-CN": "提交前快照" },
      note: "与 Git 的 pre-commit hook 和用户 stash 区分（Q170）",
    },
  ],
  checks: {
    "unused-key": "warning",
    glossary: "warning",
  },
});
