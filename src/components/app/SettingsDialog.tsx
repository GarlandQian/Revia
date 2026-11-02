import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError } from "@/lib/error";
/**
 * 设置对话框（M4 F08、返修 E/Q139）：左上角搜索、左侧分类可展开、右侧顶部
 * 显示“分类 › 页名”，外观照 VS Code。
 *
 * 分类与设置项以 UI 方向的“设置”一节为准，只实现标为 M4 的项；M5 及以后
 * 的项不做，也不放入口。修改立即保存；输入框在失焦或回车时提交，避免每次
 * 按键都写设置。所有设置都存在 Rust 的 settings.rs 里，前端不用 localStorage。
 */

import type {
  FetchTagMode,
  LogColumns,
  LogPreviewPosition,
  NotifyKinds,
  UpdateMode,
} from "@/lib/ipc";
import { useEffect, useMemo, useRef, useState } from "react";
import { messages, t } from "@/lib/i18n/index";
import { highlightLevelText, notifyKindText } from "@/lib/i18n/text";

/** 搜索关键词在资源里以 `|` 分隔，避免运行期拼接翻译键（Q171）。 */
function keywordList(value: string): string[] {
  return value.split("|");
}
import { themesOfKind } from "@/lib/theme";
import { Codicon } from "@/components/ui/codicon";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { M7GithubSettings, M7ShortcutSettings, M7ThemeSettings, M7UpdateSettings } from "@/features/m7/settings";
import type { AppUpdateMode } from "@/types/bindings/AppUpdateMode";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import { useModalDialog } from "@/features/m7/glass/nativeGlass";
import { useGlassLens } from "@/features/m7/glass/glassLens";

export interface SettingsValues {
  themeLight: string;
  themeDark: string;
  codeFontFamily: string;
  fontSize: number;
  gitPath: string;
  mergeTool: string;
  diffTool: string;
  language: "auto" | "zh-CN" | "en";
  cherryPickKeepEmpty: boolean;
  cherryPickIncludeSource: boolean;
  sidebarWidth: number;
  uiZoom: number;
  treeGroupByDirectory: boolean;
  diffContextLines: number;
  diffFoldUnchanged: boolean;
  diffSyncScroll: boolean;
  diffHighlight: "word" | "line" | "char" | "none";
  diffShowWhitespace: boolean;
  diffSoftWrap: boolean;
  diffJumpToNextFile: boolean;
  diffIgnoreWhitespace: number;
  changelistMode: boolean;
  commitSubjectLimit: number;
  commitBlankLineRequired: boolean;
  commitBodyLineLimit: number;
  commitShowRuler: boolean;
  commitWarnCrlf: boolean;
  commitWarnDetached: boolean;
  commitWarnLargeFileBytes: number;
  commitWarnInvalidFilename: boolean;
  notify: NotifyKinds;
  mergeAutoApply: boolean;
  logColumns: LogColumns;
  fileHistoryShowNames: boolean;
  stashCompareLocal: boolean;
  logLongEdges: boolean;
  logFirstParent: boolean;
  logPreviewPosition: LogPreviewPosition;
  updateMode: UpdateMode;
  fetchTagMode: FetchTagMode;
  fetchAutomatically: boolean;
  checkRemote: boolean;
  autoUpdateOnPushRejection: boolean;
  pushDialogOnCommit: boolean;
  pushDialogProtectedOnly: boolean;
  /** 受保护分支（每行一个）。 */
  protectedBranches: string;
  /** 当前会话实际使用的 Git 程序（只读展示）。 */
  detectedGitPath: string | null;
  appUpdateMode: AppUpdateMode;
  allowPrerelease: boolean;
  shortcuts: Record<string, string>;
}

export interface SettingsPatch {
  themeLight?: string;
  themeDark?: string;
  codeFontFamily?: string;
  diffFontSize?: number;
  gitPath?: string;
  mergeTool?: string;
  diffTool?: string;
  language?: string;
  cherryPickKeepEmpty?: boolean;
  cherryPickIncludeSource?: boolean;
  sidebarWidth?: number;
  viewMode?: "split" | "unified";
  uiZoom?: number;
  treeGroupByDirectory?: boolean;
  diffContextLines?: number;
  diffFoldUnchanged?: boolean;
  diffSyncScroll?: boolean;
  diffHighlight?: "word" | "line" | "char" | "none";
  diffShowWhitespace?: boolean;
  diffSoftWrap?: boolean;
  diffJumpToNextFile?: boolean;
  diffIgnoreWhitespace?: number;
  changelistMode?: boolean;
  commitSubjectLimit?: number;
  commitBlankLineRequired?: boolean;
  commitBodyLineLimit?: number;
  commitShowRuler?: boolean;
  commitWarnCrlf?: boolean;
  commitWarnDetached?: boolean;
  commitWarnLargeFileBytes?: number;
  commitWarnInvalidFilename?: boolean;
  notify?: NotifyKinds;
  mergeAutoApply?: boolean;
  logColumns?: LogColumns;
  fileHistoryShowNames?: boolean;
  stashCompareLocal?: boolean;
  logLongEdges?: boolean;
  logFirstParent?: boolean;
  logPreviewPosition?: LogPreviewPosition;
  updateMode?: UpdateMode;
  fetchTagMode?: FetchTagMode;
  fetchAutomatically?: boolean;
  checkRemote?: boolean;
  autoUpdateOnPushRejection?: boolean;
  pushDialogOnCommit?: boolean;
  pushDialogProtectedOnly?: boolean;
  protectedBranches?: string[];
  appUpdateMode?: AppUpdateMode;
  allowPrerelease?: boolean;
  shortcuts?: Record<string, string>;
}

type Category = "appearance" | "commit" | "diff" | "log" | "notifications" | "git" | "extensions" | "about";
type Page =
  | "theme"
  | "language"
  | "font"
  | "commitMessage"
  | "commitWarnings"
  | "diffDisplay"
  | "logSettings"
  | "fileHistory"
  | "notifications"
  | "gitTools"
  | "updatePush"
  | "themeColors"
  | "shortcuts"
  | "githubAccounts"
  | "appUpdate"
  | "about";

interface PageDef {
  id: Page;
  title: string;
  /** 搜索用的关键词（设置项文字）。 */
  keywords: readonly string[];
}

interface CategoryDef {
  id: Category;
  title: string;
  pages: PageDef[];
}

function buildCategories(): CategoryDef[] {
  return [
  {
    id: "appearance",
    title: messages.settings.appearance,
    pages: [
      { id: "theme", title: messages.settings.pages.theme, keywords: keywordList(messages.settings.keywords.theme) },
      { id: "language", title: messages.settings.pages.language, keywords: keywordList(messages.settings.keywords.language) },
      { id: "font", title: messages.settings.pages.font, keywords: keywordList(messages.settings.keywords.font) },
    ],
  },
  {
    id: "commit",
    title: messages.settings.commit,
    pages: [
      {
        id: "commitMessage",
        title: messages.settings.pages.commitMessage,
        keywords: keywordList(messages.settings.keywords.commitMessage),
      },
      {
        id: "commitWarnings",
        title: messages.settings.pages.commitWarnings,
        keywords: keywordList(messages.settings.keywords.commitWarnings),
      },
    ],
  },
  {
    id: "diff",
    title: messages.settings.diff,
    pages: [
      {
        id: "diffDisplay",
        title: messages.settings.pages.diffDisplay,
        keywords: keywordList(messages.settings.keywords.diffDisplay),
      },
    ],
  },
  {
    id: "log",
    title: messages.log.title,
    pages: [
      { id: "logSettings", title: messages.settings.pages.log, keywords: keywordList(messages.settings.keywords.log) },
      { id: "fileHistory", title: messages.settings.pages.fileHistory, keywords: keywordList(messages.settings.keywords.fileHistory) },
    ],
  },
  {
    id: "notifications",
    title: messages.settings.notifications,
    pages: [
      { id: "notifications", title: messages.settings.pages.notifications, keywords: keywordList(messages.settings.keywords.notifications) },
    ],
  },
  {
    id: "git",
    title: messages.settings.git,
    pages: [
      {
        id: "gitTools",
        title: messages.settings.pages.gitTools,
        keywords: keywordList(messages.settings.keywords.gitTools),
      },
      {
        id: "updatePush",
        title: messages.settings.pages.updatePush,
        keywords: keywordList(messages.settings.keywords.updatePush),
      },
    ],
  },
  {
    id: "extensions",
    title: t("settings.m7.title"),
    pages: [
      {
        id: "themeColors",
        title: t("settings.m7.theme.title"),
        keywords: keywordList(t("settings.m7.theme.keywords")),
      },
      {
        id: "shortcuts",
        title: t("settings.m7.shortcuts.title"),
        keywords: keywordList(t("settings.m7.shortcuts.keywords")),
      },
      {
        id: "githubAccounts",
        title: t("settings.m7.github.title"),
        keywords: keywordList(t("settings.m7.github.keywords")),
      },
      {
        id: "appUpdate",
        title: t("settings.m7.update.title"),
        keywords: keywordList(t("settings.m7.update.keywords")),
      },
    ],
  },
  {
    id: "about",
    title: messages.settings.about,
    pages: [{ id: "about", title: messages.settings.about, keywords: keywordList(messages.settings.keywords.about) }],
  },
  ];
}

/** 导航行的图标（Q202，参照 Loop 侧栏）。 */
const PAGE_ICONS: Record<Page, string> = {
  theme: "symbol-color",
  language: "globe",
  font: "text-size",
  commitMessage: "comment-draft",
  commitWarnings: "warning",
  diffDisplay: "diff",
  logSettings: "list-unordered",
  fileHistory: "history",
  notifications: "bell",
  gitTools: "tools",
  updatePush: "repo-push",
  themeColors: "paintcan",
  shortcuts: "keyboard",
  githubAccounts: "github",
  appUpdate: "cloud-download",
  about: "info",
};

const CATEGORY_OF_PAGE: Record<Page, Category> = {
  theme: "appearance",
  language: "appearance",
  font: "appearance",
  commitMessage: "commit",
  commitWarnings: "commit",
  diffDisplay: "diff",
  logSettings: "log",
  fileHistory: "log",
  notifications: "notifications",
  gitTools: "git",
  updatePush: "git",
  themeColors: "extensions",
  shortcuts: "extensions",
  githubAccounts: "extensions",
  appUpdate: "extensions",
  about: "about",
};

export function SettingsDialog({
  open,
  values,
  onClose,
  onChange,
  onReloadEnvironment,
  worktreeId = null,
  updateStatus = null,
  onUpdateStatus,
  onThemesChanged,
  onAccountChanged,
}: {
  open: boolean;
  values: SettingsValues;
  onClose: () => void;
  onChange: (patch: SettingsPatch) => Promise<AppErrorPayload | null>;
  onReloadEnvironment: () => Promise<AppErrorPayload | null>;
  /** M7：GitHub 账号页要按当前仓库读取绑定关系。 */
  worktreeId?: string | null;
  updateStatus?: AppUpdateStatus | null;
  onUpdateStatus?: (status: AppUpdateStatus) => void;
  onThemesChanged?: () => Promise<void> | void;
  onAccountChanged?: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const navRef = useRef<HTMLElement>(null);
  const pageListRef = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState<Page>("theme");
  // 切换设置页时的玻璃镜片（Q202）。
  useGlassLens({ host: navRef, track: pageListRef, selected: page });
  const [query, setQuery] = useState("");
  const [font, setFont] = useState(values.codeFontFamily);
  const [size, setSize] = useState(String(values.fontSize));
  const [zoom, setZoom] = useState(String(values.uiZoom));
  const [gitPath, setGitPath] = useState(values.gitPath);
  const [mergeTool, setMergeTool] = useState(values.mergeTool);
  const [diffTool, setDiffTool] = useState(values.diffTool);
  const [subjectLimit, setSubjectLimit] = useState(String(values.commitSubjectLimit));
  const [bodyLimit, setBodyLimit] = useState(String(values.commitBodyLineLimit));
  const [largeFileMb, setLargeFileMb] = useState(String(Math.round(values.commitWarnLargeFileBytes / (1024 * 1024))));
  const [protectedBranches, setProtectedBranches] = useState(values.protectedBranches);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [notice, setNotice] = useState(false);

  useEffect(() => {
    if (!open) return;
    setFont(values.codeFontFamily);
    setSize(String(values.fontSize));
    setZoom(String(values.uiZoom));
    setGitPath(values.gitPath);
    setMergeTool(values.mergeTool);
    setDiffTool(values.diffTool);
    setSubjectLimit(String(values.commitSubjectLimit));
    setBodyLimit(String(values.commitBodyLineLimit));
    setLargeFileMb(String(Math.round(values.commitWarnLargeFileBytes / (1024 * 1024))));
    setProtectedBranches(values.protectedBranches);
    setError(null);
    setNotice(false);
    setQuery("");
  }, [
    open,
    values.codeFontFamily,
    values.fontSize,
    values.uiZoom,
    values.gitPath,
    values.mergeTool,
    values.diffTool,
    values.commitSubjectLimit,
    values.commitBodyLineLimit,
    values.commitWarnLargeFileBytes,
    values.protectedBranches,
  ]);

  // 系统材质下设置页铺满整窗，放在整窗玻璃上。
  useModalDialog(ref, open, "page");

  const categories = buildCategories();
  const trimmedQuery = query.trim();
  const matches = useMemo(() => {
    if (!trimmedQuery) return null;
    const text = trimmedQuery.toLowerCase();
    const result: PageDef[] = [];
    for (const category of categories) {
      for (const candidate of category.pages) {
        const haystack = [category.title, candidate.title, ...candidate.keywords].join(" ").toLowerCase();
        if (haystack.includes(text)) result.push(candidate);
      }
    }
    return result;
  }, [categories, trimmedQuery]);

  if (!open) return null;

  const commit = async (patch: SettingsPatch) => {
    const failure = await onChange(patch);
    setError(failure);
    return failure === null;
  };

  const commitNumber = async (raw: string, current: number, apply: (value: number) => SettingsPatch, reset: (value: string) => void, min = 0, max = 1000) => {
    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) {
      reset(String(current));
      return;
    }
    const clamped = Math.min(max, Math.max(min, parsed));
    if (clamped !== current) await commit(apply(clamped));
    else reset(String(current));
  };

  const activePage = categories.find((category) => category.id === CATEGORY_OF_PAGE[page]);
  const activePageTitle = activePage?.pages.find((candidate) => candidate.id === page)?.title ?? "";
  const activeCategoryTitle = activePage?.title ?? "";

  // 导航平铺（Q202）：多页的分类只留一行小标题，单页分类直接是一行；搜索时只留命中的页。
  const visibleCategories = categories
    .map((category) => ({
      ...category,
      pages: trimmedQuery && matches ? category.pages.filter((candidate) => matches.includes(candidate)) : category.pages,
    }))
    .filter((category) => category.pages.length > 0);

  return (
    <dialog
      ref={ref}
      aria-label={messages.settings.title}
      className="settings-dialog app-dialog glass-isolate w-[min(900px,calc(100vw-48px))] overflow-hidden p-0 text-[13px]"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <GlassLayer variant="popover" />
      <div className="settings-body flex h-[min(560px,calc(100vh-140px))]">
        <nav ref={navRef} className="settings-nav glass-lens-host flex w-[220px] shrink-0 flex-col border-r border-panel-border bg-[var(--glass-side-tint)]" aria-label={messages.settings.title}>
          <div className="settings-nav-head px-3 pt-3 pb-1" data-tauri-drag-region>
            <h2 className="text-[14px] font-semibold" data-tauri-drag-region>{messages.settings.title}</h2>
          </div>
          <div className="relative px-2 py-2">
            <Codicon name="search" size={13} className="pointer-events-none absolute top-1/2 left-4 -translate-y-1/2 text-descriptionforeground" />
            <input
              className="settings-input w-full pl-7"
              value={query}
              placeholder={messages.settings.search}
              aria-label={messages.settings.search}
              spellCheck={false}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <div ref={pageListRef} className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            {visibleCategories.map((category) => (
              <div key={category.id} className="settings-nav-group">
                {category.pages.length > 1 ? (
                  <div className="settings-nav-caption truncate px-2 pt-2 pb-1 text-[11px] text-descriptionforeground">{category.title}</div>
                ) : null}
                {category.pages.map((candidate) => (
                  <button
                    key={candidate.id}
                    type="button"
                    className={`settings-nav-item flex h-7 w-full items-center gap-2 rounded-[3px] px-2 text-left text-[12.5px] outline-none hover:bg-toolbar-hoverbackground focus-visible:bg-toolbar-hoverbackground ${
                      page === candidate.id ? "bg-[var(--list-inactiveselectionbackground)]" : ""
                    }`}
                    aria-pressed={page === candidate.id}
                    data-lens-key={candidate.id}
                    onClick={() => setPage(candidate.id)}
                  >
                    <Codicon name={PAGE_ICONS[candidate.id]} size={15} className="shrink-0" />
                    <span className="min-w-0 truncate">{candidate.title}</span>
                  </button>
                ))}
              </div>
            ))}
            {trimmedQuery && matches && matches.length === 0 ? (
              <p className="px-2 py-2 text-[12px] text-descriptionforeground">{messages.settings.searchEmpty}</p>
            ) : null}
          </div>
        </nav>
        <div className="settings-main flex min-w-0 flex-1 flex-col">
          <div className="settings-header flex shrink-0 items-start gap-3 border-b border-panel-border px-5 pt-3 pb-2.5" data-tauri-drag-region>
            <div className="min-w-0 flex-1" data-tauri-drag-region>
              {activeCategoryTitle !== activePageTitle ? (
                <p className="settings-crumb truncate text-[11.5px] text-descriptionforeground" data-tauri-drag-region>{activeCategoryTitle}</p>
              ) : null}
              <h3 className="settings-title truncate text-[15px] font-semibold" data-tauri-drag-region>{activePageTitle}</h3>
            </div>
            <button type="button" className="icon-action shrink-0" aria-label={messages.recovery.close} title={messages.recovery.close} onClick={onClose}>
              <Codicon name="close" size={16} />
            </button>
          </div>
          <div className="settings-scroll min-h-0 flex-1 overflow-y-auto p-5">
            {error ? (
              <p role="alert" className="mb-3 rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] bg-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_10%,var(--editor-background))] px-3 py-2 text-[12px]">
                {formatAppError(error)}
              </p>
            ) : null}
            {notice ? (
              <p role="status" className="mb-3 text-[12px] text-descriptionforeground">
                {messages.settings.reloaded}
              </p>
            ) : null}

            {page === "theme" ? (
              <div className="space-y-5">
                <p className="text-[12px] text-descriptionforeground">{messages.settings.themeHint}</p>
                <Field label={messages.settings.lightTheme}>
                  <select
                    className="settings-select"
                    value={values.themeLight}
                    onChange={(event) => void commit({ themeLight: event.target.value })}
                  >
                    {themesOfKind("light").map((theme) => (
                      <option key={theme.id} value={theme.id}>
                        {theme.label}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label={messages.settings.darkTheme}>
                  <select
                    className="settings-select"
                    value={values.themeDark}
                    onChange={(event) => void commit({ themeDark: event.target.value })}
                  >
                    {themesOfKind("dark").map((theme) => (
                      <option key={theme.id} value={theme.id}>
                        {theme.label}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
            ) : null}

            {page === "language" ? (
              <div className="space-y-5">
                <Field label={messages.settings.language} hint={messages.settings.languageHint}>
                  <select
                    className="settings-select"
                    value={values.language}
                    onChange={(event) => void commit({ language: event.target.value === "auto" ? "" : event.target.value })}
                  >
                    <option value="auto">{messages.settings.languageAuto}</option>
                    <option value="zh-CN">{messages.settings.languageChinese}</option>
                    <option value="en">{messages.settings.languageEnglish}</option>
                  </select>
                </Field>
              </div>
            ) : null}

            {page === "font" ? (
              <div className="space-y-5">
                <Field label={messages.settings.codeFont} hint={messages.settings.codeFontHint}>
                  <input
                    className="settings-input"
                    value={font}
                    placeholder="JetBrains Mono"
                    spellCheck={false}
                    onChange={(event) => setFont(event.target.value)}
                    onBlur={() => {
                      if (font !== values.codeFontFamily) void commit({ codeFontFamily: font });
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && font !== values.codeFontFamily) void commit({ codeFontFamily: font });
                    }}
                  />
                </Field>
                <Field label={messages.settings.fontSize} hint={messages.settings.fontSizeHint}>
                  <input
                    className="settings-input w-24"
                    type="number"
                    min={9}
                    max={24}
                    value={size}
                    onChange={(event) => setSize(event.target.value)}
                    onBlur={() => void commitNumber(size, values.fontSize, (value) => ({ diffFontSize: value }), setSize, 9, 24)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void commitNumber(size, values.fontSize, (value) => ({ diffFontSize: value }), setSize, 9, 24);
                    }}
                  />
                </Field>
                <Field label={messages.settings.uiZoom} hint={messages.settings.uiZoomHint}>
                  <div className="flex items-center gap-2">
                    <input
                      className="settings-input w-24"
                      type="number"
                      min={80}
                      max={150}
                      step={10}
                      value={zoom}
                      onChange={(event) => setZoom(event.target.value)}
                      onBlur={() => void commitNumber(zoom, values.uiZoom, (value) => ({ uiZoom: value }), setZoom, 80, 150)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") void commitNumber(zoom, values.uiZoom, (value) => ({ uiZoom: value }), setZoom, 80, 150);
                      }}
                    />
                    <span className="text-[12px] text-descriptionforeground">%</span>
                    <button
                      type="button"
                      className="rounded-[3px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-0.5 text-[11.5px] hover:bg-toolbar-hoverbackground"
                      onClick={() => {
                        setZoom("100");
                        void commit({ uiZoom: 100 });
                      }}
                    >
                      {messages.settings.uiZoomReset}
                    </button>
                  </div>
                </Field>
              </div>
            ) : null}

            {page === "commitMessage" ? (
              <div className="space-y-5">
                <Toggle
                  label={messages.settings.cherryPickKeepEmpty}
                  hint={messages.settings.cherryPickKeepEmptyHint}
                  checked={values.cherryPickKeepEmpty}
                  onChange={(value) => void commit({ cherryPickKeepEmpty: value })}
                />
                <Toggle
                  label={messages.settings.cherryPickIncludeSource}
                  hint={messages.settings.cherryPickIncludeSourceHint}
                  checked={values.cherryPickIncludeSource}
                  onChange={(value) => void commit({ cherryPickIncludeSource: value })}
                />
                <Toggle label={t("changelists.mode")} checked={values.changelistMode} onChange={(value) => void commit({ changelistMode: value })} />
                <Field label={messages.settings.commitSubjectLimit} hint={messages.settings.commitSubjectLimitHint}>
                  <input
                    className="settings-input w-24"
                    type="number"
                    min={0}
                    max={200}
                    value={subjectLimit}
                    onChange={(event) => setSubjectLimit(event.target.value)}
                    onBlur={() =>
                      void commitNumber(subjectLimit, values.commitSubjectLimit, (value) => ({ commitSubjectLimit: value }), setSubjectLimit, 0, 200)
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        void commitNumber(subjectLimit, values.commitSubjectLimit, (value) => ({ commitSubjectLimit: value }), setSubjectLimit, 0, 200);
                      }
                    }}
                  />
                </Field>
                <Field label={messages.settings.commitBodyLineLimit} hint={messages.settings.commitBodyLineLimitHint}>
                  <input
                    className="settings-input w-24"
                    type="number"
                    min={0}
                    max={200}
                    value={bodyLimit}
                    onChange={(event) => setBodyLimit(event.target.value)}
                    onBlur={() =>
                      void commitNumber(bodyLimit, values.commitBodyLineLimit, (value) => ({ commitBodyLineLimit: value }), setBodyLimit, 0, 200)
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        void commitNumber(bodyLimit, values.commitBodyLineLimit, (value) => ({ commitBodyLineLimit: value }), setBodyLimit, 0, 200);
                      }
                    }}
                  />
                </Field>
                <Toggle
                  label={messages.settings.commitBlankLineRequired}
                  hint={messages.settings.commitBlankLineRequiredHint}
                  checked={values.commitBlankLineRequired}
                  onChange={(value) => void commit({ commitBlankLineRequired: value })}
                />
                <Toggle
                  label={messages.settings.commitShowRuler}
                  hint={messages.settings.commitShowRulerHint}
                  checked={values.commitShowRuler}
                  onChange={(value) => void commit({ commitShowRuler: value })}
                />
              </div>
            ) : null}

            {page === "commitWarnings" ? (
              <div className="space-y-3">
                <p className="text-[12px] text-descriptionforeground">{messages.settings.commitWarningsHint}</p>
                <Toggle
                  label={messages.settings.commitWarnCrlf}
                  checked={values.commitWarnCrlf}
                  onChange={(value) => void commit({ commitWarnCrlf: value })}
                />
                <Toggle
                  label={messages.settings.commitWarnDetached}
                  checked={values.commitWarnDetached}
                  onChange={(value) => void commit({ commitWarnDetached: value })}
                />
                <Toggle
                  label={messages.settings.commitWarnInvalidFilename}
                  checked={values.commitWarnInvalidFilename}
                  onChange={(value) => void commit({ commitWarnInvalidFilename: value })}
                />
                <Field label={messages.settings.commitWarnLargeFile} hint={messages.settings.commitWarnLargeFileHint}>
                  <div className="flex items-center gap-2">
                    <input
                      className="settings-input w-24"
                      type="number"
                      min={0}
                      max={10240}
                      value={largeFileMb}
                      onChange={(event) => setLargeFileMb(event.target.value)}
                      onBlur={() =>
                        void commitNumber(
                          largeFileMb,
                          Math.round(values.commitWarnLargeFileBytes / (1024 * 1024)),
                          (value) => ({ commitWarnLargeFileBytes: value * 1024 * 1024 }),
                          setLargeFileMb,
                          0,
                          10240,
                        )
                      }
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          void commitNumber(
                            largeFileMb,
                            Math.round(values.commitWarnLargeFileBytes / (1024 * 1024)),
                            (value) => ({ commitWarnLargeFileBytes: value * 1024 * 1024 }),
                            setLargeFileMb,
                            0,
                            10240,
                          );
                        }
                      }}
                    />
                    <span className="text-[12px] text-descriptionforeground">MB</span>
                  </div>
                </Field>
              </div>
            ) : null}

            {page === "diffDisplay" ? (
              <div className="space-y-3">
                <Field label={messages.settings.diffContextLines} hint={messages.settings.diffContextLinesHint}>
                  <input
                    className="settings-input w-24"
                    type="number"
                    min={0}
                    max={64}
                    value={values.diffContextLines}
                    onChange={(event) => {
                      const parsed = Number.parseInt(event.target.value, 10);
                      if (Number.isFinite(parsed)) void commit({ diffContextLines: Math.min(64, Math.max(0, parsed)) });
                    }}
                  />
                </Field>
                <Toggle
                  label={messages.diff.foldUnchanged}
                  checked={values.diffFoldUnchanged}
                  onChange={(value) => void commit({ diffFoldUnchanged: value })}
                />
                <Toggle
                  label={messages.diff.syncScroll}
                  checked={values.diffSyncScroll}
                  onChange={(value) => void commit({ diffSyncScroll: value })}
                />
                <Toggle
                  label={messages.diff.showWhitespace}
                  checked={values.diffShowWhitespace}
                  onChange={(value) => void commit({ diffShowWhitespace: value })}
                />
                <Toggle
                  label={messages.diff.softWrap}
                  checked={values.diffSoftWrap}
                  onChange={(value) => void commit({ diffSoftWrap: value })}
                />
                <Toggle
                  label={messages.diff.jumpToNextFile}
                  checked={values.diffJumpToNextFile}
                  onChange={(value) => void commit({ diffJumpToNextFile: value })}
                />
                <Field label={messages.diff.highlightLabel}>
                  <select
                    className="settings-select"
                    value={values.diffHighlight}
                    onChange={(event) => void commit({ diffHighlight: event.target.value as SettingsValues["diffHighlight"] })}
                  >
                    {(["word", "line", "char", "none"] as const).map((level) => (
                      <option key={level} value={level}>
                        {highlightLevelText(level)}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label={messages.diff.ignoreWhitespace} hint={messages.diff.ignoreWhitespaceReadonly}>
                  <select
                    className="settings-select"
                    value={values.diffIgnoreWhitespace}
                    onChange={(event) => void commit({ diffIgnoreWhitespace: Number(event.target.value) })}
                  >
                    <option value={0}>{messages.diff.ignoreWhitespaceLevels.off}</option>
                    <option value={1}>{messages.diff.ignoreWhitespaceLevels.eol}</option>
                    <option value={2}>{messages.diff.ignoreWhitespaceLevels.all}</option>
                    <option value={3}>{messages.diff.ignoreWhitespaceLevels.allBlank}</option>
                  </select>
                </Field>
              </div>
            ) : null}

            {page === "fileHistory" ? (
              <div className="space-y-5">
                <Field
                  label={messages.settings.fileHistoryShowNames}
                  hint={messages.settings.fileHistoryShowNamesHint}
                >
                  <label className="flex items-center gap-2 text-[12.5px]">
                    <input
                      type="checkbox"
                      checked={values.fileHistoryShowNames}
                      onChange={(event) => void commit({ fileHistoryShowNames: event.target.checked })}
                    />
                    {messages.settings.fileHistoryShowNames}
                  </label>
                </Field>
              </div>
            ) : null}

            {page === "logSettings" ? (
              <div className="space-y-5">
                <Field label={messages.settings.logColumns} hint={messages.settings.logColumnsHint}>
                  <div className="space-y-1.5">
                    {([
                      ["author", messages.settings.logColumnAuthor],
                      ["date", messages.settings.logColumnDate],
                      ["hash", messages.settings.logColumnHash],
                    ] as const).map(([key, label]) => (
                      <label key={key} className="flex items-center gap-2 text-[12.5px]">
                        <input
                          type="checkbox"
                          checked={values.logColumns[key]}
                          onChange={(event) =>
                            void commit({ logColumns: { ...values.logColumns, [key]: event.target.checked } })
                          }
                        />
                        {label}
                      </label>
                    ))}
                  </div>
                </Field>
                <Field label={messages.settings.logLongEdges} hint={messages.settings.logLongEdgesHint}>
                  <label className="flex items-center gap-2 text-[12.5px]">
                    <input
                      type="checkbox"
                      checked={values.logLongEdges}
                      onChange={(event) => void commit({ logLongEdges: event.target.checked })}
                    />
                    {messages.settings.logLongEdges}
                  </label>
                </Field>
                <Field label={messages.settings.logFirstParent} hint={messages.settings.logFirstParentHint}>
                  <label className="flex items-center gap-2 text-[12.5px]">
                    <input
                      type="checkbox"
                      checked={values.logFirstParent}
                      onChange={(event) => void commit({ logFirstParent: event.target.checked })}
                    />
                    {messages.settings.logFirstParent}
                  </label>
                </Field>
                <Field label={messages.settings.logPreviewPosition}>
                  <select
                    className="settings-select"
                    value={values.logPreviewPosition}
                    onChange={(event) => void commit({ logPreviewPosition: event.target.value as LogPreviewPosition })}
                  >
                    <option value="right">{messages.settings.logPreviewRight}</option>
                    <option value="bottom">{messages.settings.logPreviewBottom}</option>
                  </select>
                </Field>
                <p className="text-[11.5px] text-descriptionforeground">{messages.settings.logHint}</p>
              </div>
            ) : null}

            {page === "updatePush" ? (
              <div className="space-y-5">
                <Field label={messages.settings.updateMode} hint={messages.settings.updateModeHint}>
                  <select
                    className="settings-select"
                    value={values.updateMode}
                    onChange={(event) => void commit({ updateMode: event.target.value as UpdateMode })}
                  >
                    <option value="merge">{messages.settings.updateModeMerge}</option>
                    <option value="rebase">{messages.settings.updateModeRebase}</option>
                    <option value="branchDefault">{messages.settings.updateModeBranchDefault}</option>
                  </select>
                </Field>
                <Field label={messages.settings.fetchTagMode}>
                  <select
                    className="settings-select"
                    value={values.fetchTagMode}
                    onChange={(event) => void commit({ fetchTagMode: event.target.value as FetchTagMode })}
                  >
                    <option value="auto">{messages.settings.fetchTagModeAuto}</option>
                    <option value="sync">{messages.settings.fetchTagModeSync}</option>
                    <option value="always">{messages.settings.fetchTagModeAlways}</option>
                    <option value="never">{messages.settings.fetchTagModeNever}</option>
                  </select>
                </Field>
                <Field label={messages.settings.fetchAutomatically} hint={messages.settings.fetchAutomaticallyHint}>
                  <label className="flex items-center gap-2 text-[12.5px]">
                    <input
                      type="checkbox"
                      checked={values.fetchAutomatically}
                      onChange={(event) => void commit({ fetchAutomatically: event.target.checked })}
                    />
                    {messages.settings.fetchAutomatically}
                  </label>
                </Field>
                <Field label={messages.settings.checkRemote} hint={messages.settings.checkRemoteHint}>
                  <label className="flex items-center gap-2 text-[12.5px]">
                    <input
                      type="checkbox"
                      checked={values.checkRemote}
                      onChange={(event) => void commit({ checkRemote: event.target.checked })}
                    />
                    {messages.settings.checkRemote}
                  </label>
                </Field>
                <Field label={messages.settings.autoUpdateOnPushRejection} hint={messages.settings.autoUpdateOnPushRejectionHint}>
                  <label className="flex items-center gap-2 text-[12.5px]">
                    <input
                      type="checkbox"
                      checked={values.autoUpdateOnPushRejection}
                      onChange={(event) => void commit({ autoUpdateOnPushRejection: event.target.checked })}
                    />
                    {messages.settings.autoUpdateOnPushRejection}
                  </label>
                </Field>
                <Field label={messages.settings.pushDialogOnCommit}>
                  <div className="space-y-1.5">
                    <label className="flex items-center gap-2 text-[12.5px]">
                      <input
                        type="checkbox"
                        checked={values.pushDialogOnCommit}
                        onChange={(event) => void commit({ pushDialogOnCommit: event.target.checked })}
                      />
                      {messages.settings.pushDialogOnCommit}
                    </label>
                    <label className="flex items-center gap-2 text-[12.5px]">
                      <input
                        type="checkbox"
                        checked={values.pushDialogProtectedOnly}
                        onChange={(event) => void commit({ pushDialogProtectedOnly: event.target.checked })}
                      />
                      {messages.settings.pushDialogProtectedOnly}
                    </label>
                  </div>
                </Field>
                <Field label={messages.settings.protectedBranches} hint={messages.settings.protectedBranchesHint}>
                  <textarea
                    className="settings-input min-h-[72px] w-full font-mono"
                    value={protectedBranches}
                    spellCheck={false}
                    onChange={(event) => setProtectedBranches(event.target.value)}
                    onBlur={() =>
                      void commit({
                        protectedBranches: protectedBranches
                          .split(/\r?\n/)
                          .map((line) => line.trim())
                          .filter((line) => line.length > 0),
                      })
                    }
                  />
                </Field>
              </div>
            ) : null}

            {page === "notifications" ? (
              <div className="space-y-3">
                <p className="text-[12px] text-descriptionforeground">{messages.settings.notifyHint}</p>
                {(["stage", "discard", "commit", "recovery"] as const).map((kind) => (
                  <Toggle
                    key={kind}
                    label={notifyKindText(kind)}
                    checked={values.notify[kind]}
                    onChange={(value) => void commit({ notify: { ...values.notify, [kind]: value } })}
                  />
                ))}
              </div>
            ) : null}

            {page === "gitTools" ? (
              <div className="space-y-5">
                <Toggle
                  label={messages.stash.compareLocal}
                  hint={messages.settings.stashComparisonHint}
                  checked={values.stashCompareLocal}
                  onChange={(value) => void commit({ stashCompareLocal: value })}
                />
                <Field
                  label={messages.settings.gitPath}
                  hint={
                    values.detectedGitPath
                      ? `${messages.settings.detectedGit}${values.detectedGitPath}`
                      : messages.settings.gitPathHint
                  }
                >
                  <input
                    className="settings-input font-mono text-[12px]"
                    value={gitPath}
                    placeholder={messages.settings.gitPathPlaceholder}
                    spellCheck={false}
                    onChange={(event) => setGitPath(event.target.value)}
                    onBlur={() => {
                      if (gitPath !== values.gitPath) void commit({ gitPath });
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && gitPath !== values.gitPath) void commit({ gitPath });
                    }}
                  />
                </Field>
                <Field label={messages.settings.mergeTool} hint={messages.settings.mergeToolHint}>
                  <input
                    className="settings-input font-mono text-[12px]"
                    value={mergeTool}
                    placeholder={messages.settings.toolPlaceholder}
                    spellCheck={false}
                    onChange={(event) => setMergeTool(event.target.value)}
                    onBlur={() => {
                      if (mergeTool !== values.mergeTool) void commit({ mergeTool });
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && mergeTool !== values.mergeTool) void commit({ mergeTool });
                    }}
                  />
                </Field>
                <Field label={messages.settings.diffTool} hint={messages.settings.diffToolHint}>
                  <input
                    className="settings-input font-mono text-[12px]"
                    value={diffTool}
                    placeholder={messages.settings.toolPlaceholder}
                    spellCheck={false}
                    onChange={(event) => setDiffTool(event.target.value)}
                    onBlur={() => {
                      if (diffTool !== values.diffTool) void commit({ diffTool });
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && diffTool !== values.diffTool) void commit({ diffTool });
                    }}
                  />
                </Field>
                <Toggle
                  label={messages.settings.mergeAutoApply}
                  hint={messages.settings.mergeAutoApplyHint}
                  checked={values.mergeAutoApply}
                  onChange={(value) => void commit({ mergeAutoApply: value })}
                />
                <div>
                  <button
                    type="button"
                    className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground"
                    onClick={() => {
                      void onReloadEnvironment().then((failure) => {
                        setError(failure);
                        setNotice(!failure);
                      });
                    }}
                  >
                    {messages.settings.reload}
                  </button>
                  <p className="mt-1.5 text-[11.5px] text-descriptionforeground">{messages.settings.reloadHint}</p>
                </div>
              </div>
            ) : null}

            {page === "themeColors" ? (
              <M7ThemeSettings
                themeLight={values.themeLight}
                themeDark={values.themeDark}
                onChange={(patch) => commit(patch)}
                onThemesChanged={() => onThemesChanged?.()}
              />
            ) : null}

            {page === "shortcuts" ? (
              <M7ShortcutSettings
                shortcuts={values.shortcuts ?? {}}
                onChange={(next) => commit({ shortcuts: next })}
              />
            ) : null}

            {page === "githubAccounts" ? (
              <M7GithubSettings worktreeId={worktreeId} onChangeAccount={() => onAccountChanged?.()} />
            ) : null}

            {page === "appUpdate" ? (
              <M7UpdateSettings
                mode={values.appUpdateMode ?? "notify"}
                onChangeMode={(mode) => commit({ appUpdateMode: mode })}
                allowPrerelease={values.allowPrerelease ?? false}
                onChangeAllowPrerelease={(value) => commit({ allowPrerelease: value })}
                status={updateStatus}
                onStatusChange={(status) => onUpdateStatus?.(status)}
              />
            ) : null}

            {page === "about" ? (
              <div className="space-y-3 text-[12.5px] leading-relaxed">
                <p className="text-[15px] font-semibold">Revia</p>
                <p className="text-descriptionforeground">{messages.settings.aboutText}</p>
                <div>
                  <p className="font-semibold">{messages.settings.licenses}</p>
                  <ul className="mt-1 list-disc space-y-1 pl-5 text-descriptionforeground">
                    <li>{messages.settings.licenseCodicons}</li>
                    <li>{messages.settings.licenseMaterial}</li>
                    <li>{messages.settings.licenseThemes}</li>
                    <li>{messages.settings.licenseReactBits}</li>
                  </ul>
                </div>
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </dialog>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-[12.5px] font-semibold">{label}</span>
      {children}
      {hint ? <span className="mt-1 block text-[11.5px] text-descriptionforeground">{hint}</span> : null}
    </label>
  );
}

/** 勾选框：自绘照效果图（返修 B12），标签与说明分两行。 */
function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2 py-0.5">
      <input type="checkbox" className="vs-checkbox mt-0.5" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <span className="min-w-0">
        <span className="block text-[12.5px]">{label}</span>
        {hint ? <span className="block text-[11.5px] text-descriptionforeground">{hint}</span> : null}
      </span>
    </label>
  );
}
