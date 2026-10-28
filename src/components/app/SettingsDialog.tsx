/**
 * 设置对话框（M4 F08、返修 E/Q139）：左上角搜索、左侧分类可展开、右侧顶部
 * 显示“分类 › 页名”，外观照 VS Code。
 *
 * 分类与设置项以 UI 方向的“设置”一节为准，只实现标为 M4 的项；M5 及以后
 * 的项不做，也不放入口。修改立即保存；输入框在失焦或回车时提交，避免每次
 * 按键都写设置。所有设置都存在 Rust 的 settings.rs 里，前端不用 localStorage。
 */

import type { NotifyKinds } from "@/lib/ipc";
import { useEffect, useMemo, useRef, useState } from "react";
import { messages } from "@/lib/i18n/zh-CN";
import { themesOfKind } from "@/lib/theme";
import { Codicon } from "@/components/ui/codicon";

export interface SettingsValues {
  themeLight: string;
  themeDark: string;
  codeFontFamily: string;
  fontSize: number;
  gitPath: string;
  mergeTool: string;
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
  /** 当前会话实际使用的 Git 程序（只读展示）。 */
  detectedGitPath: string | null;
}

export interface SettingsPatch {
  themeLight?: string;
  themeDark?: string;
  codeFontFamily?: string;
  diffFontSize?: number;
  gitPath?: string;
  mergeTool?: string;
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
}

type Category = "appearance" | "commit" | "diff" | "notifications" | "git" | "about";
type Page =
  | "theme"
  | "font"
  | "commitMessage"
  | "commitWarnings"
  | "diffDisplay"
  | "notifications"
  | "gitTools"
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

const CATEGORIES: CategoryDef[] = [
  {
    id: "appearance",
    title: messages.settings.appearance,
    pages: [
      { id: "theme", title: messages.settings.pages.theme, keywords: messages.settings.keywords.theme },
      { id: "font", title: messages.settings.pages.font, keywords: messages.settings.keywords.font },
    ],
  },
  {
    id: "commit",
    title: messages.settings.commit,
    pages: [
      {
        id: "commitMessage",
        title: messages.settings.pages.commitMessage,
        keywords: messages.settings.keywords.commitMessage,
      },
      {
        id: "commitWarnings",
        title: messages.settings.pages.commitWarnings,
        keywords: messages.settings.keywords.commitWarnings,
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
        keywords: messages.settings.keywords.diffDisplay,
      },
    ],
  },
  {
    id: "notifications",
    title: messages.settings.notifications,
    pages: [
      { id: "notifications", title: messages.settings.pages.notifications, keywords: messages.settings.keywords.notifications },
    ],
  },
  {
    id: "git",
    title: messages.settings.git,
    pages: [
      {
        id: "gitTools",
        title: messages.settings.pages.gitTools,
        keywords: messages.settings.keywords.gitTools,
      },
    ],
  },
  {
    id: "about",
    title: messages.settings.about,
    pages: [{ id: "about", title: messages.settings.about, keywords: messages.settings.keywords.about }],
  },
];

const CATEGORY_OF_PAGE: Record<Page, Category> = {
  theme: "appearance",
  font: "appearance",
  commitMessage: "commit",
  commitWarnings: "commit",
  diffDisplay: "diff",
  notifications: "notifications",
  gitTools: "git",
  about: "about",
};

export function SettingsDialog({
  open,
  values,
  onClose,
  onChange,
  onReloadEnvironment,
}: {
  open: boolean;
  values: SettingsValues;
  onClose: () => void;
  onChange: (patch: SettingsPatch) => Promise<string | null>;
  onReloadEnvironment: () => Promise<string | null>;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [page, setPage] = useState<Page>("theme");
  const [expanded, setExpanded] = useState<Set<Category>>(new Set(["appearance", "commit", "diff"]));
  const [query, setQuery] = useState("");
  const [font, setFont] = useState(values.codeFontFamily);
  const [size, setSize] = useState(String(values.fontSize));
  const [zoom, setZoom] = useState(String(values.uiZoom));
  const [gitPath, setGitPath] = useState(values.gitPath);
  const [subjectLimit, setSubjectLimit] = useState(String(values.commitSubjectLimit));
  const [bodyLimit, setBodyLimit] = useState(String(values.commitBodyLineLimit));
  const [largeFileMb, setLargeFileMb] = useState(String(Math.round(values.commitWarnLargeFileBytes / (1024 * 1024))));
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setFont(values.codeFontFamily);
    setSize(String(values.fontSize));
    setZoom(String(values.uiZoom));
    setGitPath(values.gitPath);
    setSubjectLimit(String(values.commitSubjectLimit));
    setBodyLimit(String(values.commitBodyLineLimit));
    setLargeFileMb(String(Math.round(values.commitWarnLargeFileBytes / (1024 * 1024))));
    setError(null);
    setNotice(null);
    setQuery("");
  }, [
    open,
    values.codeFontFamily,
    values.fontSize,
    values.uiZoom,
    values.gitPath,
    values.commitSubjectLimit,
    values.commitBodyLineLimit,
    values.commitWarnLargeFileBytes,
  ]);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || !open) return;
    dialog.showModal();
    return () => dialog.close();
  }, [open]);

  const trimmedQuery = query.trim();
  const matches = useMemo(() => {
    if (!trimmedQuery) return null;
    const text = trimmedQuery.toLowerCase();
    const result: PageDef[] = [];
    for (const category of CATEGORIES) {
      for (const candidate of category.pages) {
        const haystack = [category.title, candidate.title, ...candidate.keywords].join(" ").toLowerCase();
        if (haystack.includes(text)) result.push(candidate);
      }
    }
    return result;
  }, [trimmedQuery]);

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

  const activePage = CATEGORIES.find((category) => category.id === CATEGORY_OF_PAGE[page]);
  const activePageTitle = activePage?.pages.find((candidate) => candidate.id === page)?.title ?? "";
  const activeCategoryTitle = activePage?.title ?? "";

  const openPage = (next: Page) => {
    setPage(next);
    setExpanded((current) => new Set(current).add(CATEGORY_OF_PAGE[next]));
  };

  const visibleCategories = trimmedQuery && matches
    ? CATEGORIES.filter((category) => category.pages.some((candidate) => matches.includes(candidate)))
    : CATEGORIES;

  return (
    <dialog
      ref={ref}
      aria-label={messages.settings.title}
      className="app-dialog w-[min(900px,calc(100vw-48px))] overflow-hidden p-0 text-[13px]"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <div className="flex items-center gap-2 border-b border-panel-border px-4 py-2.5">
        <h2 className="flex-1 text-[14px] font-semibold">{messages.settings.title}</h2>
        <button type="button" className="icon-action" aria-label={messages.recovery.close} title={messages.recovery.close} onClick={onClose}>
          <Codicon name="close" size={16} />
        </button>
      </div>
      <div className="flex h-[min(560px,calc(100vh-140px))]">
        <nav className="flex w-[220px] shrink-0 flex-col border-r border-panel-border bg-[var(--sidebar-background)]" aria-label={messages.settings.title}>
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
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            {visibleCategories.map((category) => {
              const isOpen = !!trimmedQuery || expanded.has(category.id);
              return (
                <div key={category.id}>
                  <button
                    type="button"
                    className={`flex h-7 w-full items-center gap-1 rounded-[3px] px-1 text-left text-[12.5px] outline-none hover:bg-toolbar-hoverbackground focus-visible:bg-toolbar-hoverbackground ${
                      CATEGORY_OF_PAGE[page] === category.id ? "font-semibold" : ""
                    }`}
                    aria-expanded={isOpen}
                    onClick={() => {
                      setExpanded((current) => {
                        const next = new Set(current);
                        if (next.has(category.id)) next.delete(category.id);
                        else next.add(category.id);
                        return next;
                      });
                      if (category.pages.length === 1) openPage(category.pages[0].id);
                    }}
                  >
                    <Codicon name={isOpen ? "chevron-down" : "chevron-right"} size={12} />
                    <span className="min-w-0 flex-1 truncate">{category.title}</span>
                  </button>
                  {isOpen
                    ? category.pages.map((candidate) => (
                        <button
                          key={candidate.id}
                          type="button"
                          className={`flex h-7 w-full items-center rounded-[3px] pr-2 pl-6 text-left text-[12.5px] outline-none hover:bg-toolbar-hoverbackground focus-visible:bg-toolbar-hoverbackground ${
                            page === candidate.id ? "bg-[var(--list-inactiveselectionbackground)]" : ""
                          }`}
                          aria-pressed={page === candidate.id}
                          onClick={() => openPage(candidate.id)}
                        >
                          <span className="min-w-0 truncate">{candidate.title}</span>
                        </button>
                      ))
                    : null}
                </div>
              );
            })}
            {trimmedQuery && matches && matches.length === 0 ? (
              <p className="px-2 py-2 text-[12px] text-descriptionforeground">{messages.settings.searchEmpty}</p>
            ) : null}
          </div>
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-8 shrink-0 items-center gap-1.5 border-b border-panel-border px-4 text-[12px] text-descriptionforeground">
            <span>{activeCategoryTitle}</span>
            <Codicon name="chevron-right" size={12} />
            <span className="text-foreground">{activePageTitle}</span>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-5">
            {error ? (
              <p role="alert" className="mb-3 rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] bg-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_10%,var(--editor-background))] px-3 py-2 text-[12px]">
                {error}
              </p>
            ) : null}
            {notice ? (
              <p role="status" className="mb-3 text-[12px] text-descriptionforeground">
                {notice}
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
                        {messages.diff.highlightLevels[level]}
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

            {page === "notifications" ? (
              <div className="space-y-3">
                <p className="text-[12px] text-descriptionforeground">{messages.settings.notifyHint}</p>
                {(["stage", "discard", "commit", "recovery"] as const).map((kind) => (
                  <Toggle
                    key={kind}
                    label={messages.settings.notifyKinds[kind]}
                    checked={values.notify[kind]}
                    onChange={(value) => void commit({ notify: { ...values.notify, [kind]: value } })}
                  />
                ))}
              </div>
            ) : null}

            {page === "gitTools" ? (
              <div className="space-y-5">
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
                        setNotice(failure ? null : messages.settings.reloaded);
                      });
                    }}
                  >
                    {messages.settings.reload}
                  </button>
                  <p className="mt-1.5 text-[11.5px] text-descriptionforeground">{messages.settings.reloadHint}</p>
                </div>
              </div>
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
