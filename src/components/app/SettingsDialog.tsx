/**
 * 设置模态弹窗（M13 Q237、Q242）：
 * - 居中宽敞两栏结构：左侧搜索 + 精简分组 Lucide 图标导航（合并原 17 项碎片页为 11 个清晰页面）；
 * - 右侧全站统一采用分组卡片（`.setting-card-list`）、左右对齐行（`.setting-row`）与现代 Switch 开关；
 * - 所有修改立即通过 Rust `settings.rs` 持久化，前端不写 localStorage。
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { AppUpdateMode } from "@/types/bindings/AppUpdateMode";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import { formatAppError } from "@/lib/error";
import {
  ipc,
  type AiLanguage,
  type AppearanceMode,
  type FetchTagMode,
  type LogColumns,
  type LogPreviewPosition,
  type NotifyKinds,
  type SettingsPatch,
  type UpdateMode,
} from "@/lib/ipc";
import type { AiSettings } from "@/lib/aiSettings";
import { messages, t } from "@/lib/i18n/index";
import { highlightLevelText, notifyKindText } from "@/lib/i18n/text";
import { themesOfKind } from "@/lib/theme";
import { AppIcon } from "@/components/ui/app-icon";
import { M7GithubSettings, M7ShortcutSettings, M7ThemeSettings, M7UpdateSettings } from "@/features/m7/settings";
import { useModalDialog } from "@/hooks/useDialogOpen";

function keywordList(value: string): string[] {
  return value.split("|");
}

const THEME_SWATCHES: Record<string, readonly string[]> = {
  "one-light": ["#FAFAFA", "#A626A4", "#4078F2", "#50A14F"],
  "light-2026": ["#FFFFFF", "#CF222E", "#8250DF", "#0A3069"],
  "one-dark-pro": ["#282C34", "#C678DD", "#61AFEF", "#98C379"],
  "dark-2026": ["#1F1F1F", "#569CD6", "#DCDCAA", "#CE9178"],
};

export interface SettingsValues {
  appearanceMode: AppearanceMode;
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
  protectedBranches: string;
  detectedGitPath: string | null;
  appUpdateMode: AppUpdateMode;
  allowPrerelease: boolean;
  shortcuts: Record<string, string>;
}

export type { SettingsPatch } from "@/lib/ipc";

type GroupId = "general" | "workflow" | "integrations";
type Page =
  | "appearance"
  | "language"
  | "shortcuts"
  | "notifications"
  | "commit"
  | "diff"
  | "log"
  | "git"
  | "ai"
  | "github"
  | "updates"
  | "about";

interface PageDef {
  id: Page;
  title: string;
  icon: string;
  keywords: readonly string[];
}

interface NavGroup {
  id: GroupId;
  title: string;
  pages: PageDef[];
}

function buildNavGroups(): NavGroup[] {
  return [
    {
      id: "general",
      title: messages.settings.appearance,
      pages: [
        {
          id: "appearance",
          title: messages.settings.pages.theme,
          icon: "symbol-color",
          keywords: [
            ...keywordList(messages.settings.keywords.theme),
            ...keywordList(messages.settings.keywords.font),
            ...keywordList(t("settings.m7.theme.keywords")),
          ],
        },
        {
          id: "language",
          title: messages.settings.pages.language,
          icon: "globe",
          keywords: keywordList(messages.settings.keywords.language),
        },
        {
          id: "shortcuts",
          title: t("settings.m7.shortcuts.title"),
          icon: "keyboard",
          keywords: keywordList(t("settings.m7.shortcuts.keywords")),
        },
        {
          id: "notifications",
          title: messages.settings.pages.notifications,
          icon: "bell",
          keywords: keywordList(messages.settings.keywords.notifications),
        },
      ],
    },
    {
      id: "workflow",
      title: messages.settings.git,
      pages: [
        {
          id: "commit",
          title: messages.settings.commit,
          icon: "comment-draft",
          keywords: [
            ...keywordList(messages.settings.keywords.commitMessage),
            ...keywordList(messages.settings.keywords.commitWarnings),
          ],
        },
        {
          id: "diff",
          title: messages.settings.pages.diffDisplay,
          icon: "diff",
          keywords: keywordList(messages.settings.keywords.diffDisplay),
        },
        {
          id: "log",
          title: messages.log.title,
          icon: "list-unordered",
          keywords: [
            ...keywordList(messages.settings.keywords.log),
            ...keywordList(messages.settings.keywords.fileHistory),
          ],
        },
        {
          id: "git",
          title: messages.settings.pages.gitTools,
          icon: "tools",
          keywords: [
            ...keywordList(messages.settings.keywords.gitTools),
            ...keywordList(messages.settings.keywords.updatePush),
          ],
        },
      ],
    },
    {
      id: "integrations",
      title: t("settings.m7.title"),
      pages: [
        {
          id: "ai",
          title: messages.settings.pages.ai,
          icon: "sparkle",
          keywords: keywordList(messages.settings.keywords.ai),
        },
        {
          id: "github",
          title: t("settings.m7.github.title"),
          icon: "github",
          keywords: keywordList(t("settings.m7.github.keywords")),
        },
        {
          id: "updates",
          title: t("settings.m7.update.title"),
          icon: "cloud-download",
          keywords: keywordList(t("settings.m7.update.keywords")),
        },
        {
          id: "about",
          title: messages.settings.about,
          icon: "info",
          keywords: keywordList(messages.settings.keywords.about),
        },
      ],
    },
  ];
}

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
  onOpenDiagnostics,
  onOpenUpdatePanel,
  aiSettings,
}: {
  open: boolean;
  values: SettingsValues;
  onClose: () => void;
  onChange: (patch: SettingsPatch) => Promise<AppErrorPayload | null>;
  onReloadEnvironment: () => Promise<AppErrorPayload | null>;
  worktreeId?: string | null;
  updateStatus?: AppUpdateStatus | null;
  onUpdateStatus?: (status: AppUpdateStatus) => void;
  onThemesChanged?: (updated?: { themeLight?: string; themeDark?: string }) => Promise<void> | void;
  onAccountChanged?: () => void;
  onOpenDiagnostics?: () => void;
  onOpenUpdatePanel?: () => void;
  aiSettings: AiSettings;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const capturingShortcutRef = useRef(false);
  const [page, setPage] = useState<Page>("appearance");
  const [query, setQuery] = useState("");
  const [font, setFont] = useState(values.codeFontFamily);
  const [size, setSize] = useState(String(values.fontSize));
  const [zoom, setZoom] = useState(String(values.uiZoom));
  const [contextLines, setContextLines] = useState(String(values.diffContextLines));
  const [gitPath, setGitPath] = useState(values.gitPath);
  const [mergeTool, setMergeTool] = useState(values.mergeTool);
  const [diffTool, setDiffTool] = useState(values.diffTool);
  const [subjectLimit, setSubjectLimit] = useState(String(values.commitSubjectLimit));
  const [bodyLimit, setBodyLimit] = useState(String(values.commitBodyLineLimit));
  const [largeFileMb, setLargeFileMb] = useState(String(Math.round(values.commitWarnLargeFileBytes / (1024 * 1024))));
  const [protectedBranches, setProtectedBranches] = useState(values.protectedBranches);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [notice, setNotice] = useState(false);

  const [aiKey, setAiKey] = useState("");
  const aiKeyRef = useRef("");
  const aiKeyEpochRef = useRef(0);
  const updateAiKey = (value: string) => {
    aiKeyRef.current = value;
    aiKeyEpochRef.current += 1;
    setAiKey(value);
  };
  const [aiRevision, setAiRevision] = useState(0);
  useEffect(() => aiSettings.subscribe(() => setAiRevision((value) => value + 1)), [aiSettings]);
  const aiView = useMemo(() => aiSettings.view(), [aiSettings, aiRevision]);

  const [measure, setMeasure] = useState<{ measure: boolean; commit: string } | null>(null);
  useEffect(() => {
    let active = true;
    void ipc.measureState()
      .then((state) => { if (active) setMeasure(state); })
      .catch(() => undefined);
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!open) {
      capturingShortcutRef.current = false;
      return;
    }
    setError(null);
    setNotice(false);
    setQuery("");
  }, [open]);

  useEffect(() => { if (open) setFont(values.codeFontFamily); }, [open, values.codeFontFamily]);
  useEffect(() => { if (open) setSize(String(values.fontSize)); }, [open, values.fontSize]);
  useEffect(() => { if (open) setZoom(String(values.uiZoom)); }, [open, values.uiZoom]);
  useEffect(() => { if (open) setContextLines(String(values.diffContextLines)); }, [open, values.diffContextLines]);
  useEffect(() => { if (open) setGitPath(values.gitPath); }, [open, values.gitPath]);
  useEffect(() => { if (open) setMergeTool(values.mergeTool); }, [open, values.mergeTool]);
  useEffect(() => { if (open) setDiffTool(values.diffTool); }, [open, values.diffTool]);
  useEffect(() => { if (open) setSubjectLimit(String(values.commitSubjectLimit)); }, [open, values.commitSubjectLimit]);
  useEffect(() => { if (open) setBodyLimit(String(values.commitBodyLineLimit)); }, [open, values.commitBodyLineLimit]);
  useEffect(() => { if (open) setLargeFileMb(String(Math.round(values.commitWarnLargeFileBytes / (1024 * 1024)))); }, [open, values.commitWarnLargeFileBytes]);
  useEffect(() => { if (open) setProtectedBranches(values.protectedBranches); }, [open, values.protectedBranches]);

  useModalDialog(ref, open);

  const navGroups = buildNavGroups();
  const trimmedQuery = query.trim();
  const visibleGroups = useMemo(() => {
    if (!trimmedQuery) return navGroups;
    const text = trimmedQuery.toLowerCase();
    return navGroups
      .map((group) => ({
        ...group,
        pages: group.pages.filter((candidate) =>
          [group.title, candidate.title, ...candidate.keywords].join(" ").toLowerCase().includes(text),
        ),
      }))
      .filter((group) => group.pages.length > 0);
  }, [navGroups, trimmedQuery]);

  useEffect(() => {
    if (!(open && page === "ai")) return;
    updateAiKey("");
    aiSettings.open();
    return () => aiSettings.close();
  }, [open, page, aiSettings]);

  if (!open) return null;

  const runAiKeyChange = (action: () => Promise<boolean>) => {
    const submitted = aiKey;
    const session = aiSettings.sessionToken();
    const keyEpoch = aiKeyEpochRef.current;
    void action().then((ok) => {
      if (
        ok
        && aiSettings.sessionToken() === session
        && aiKeyEpochRef.current === keyEpoch
        && aiKeyRef.current === submitted
      ) updateAiKey("");
    });
  };

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

  const activeGroup = navGroups.find((group) => group.pages.some((candidate) => candidate.id === page));
  const activePageDef = activeGroup?.pages.find((candidate) => candidate.id === page);
  const lightThemes = themesOfKind("light");
  const darkThemes = themesOfKind("dark");
  const resolvedLightTheme = lightThemes.some((theme) => theme.id === values.themeLight)
    ? values.themeLight
    : lightThemes[0]?.id ?? values.themeLight;
  const resolvedDarkTheme = darkThemes.some((theme) => theme.id === values.themeDark)
    ? values.themeDark
    : darkThemes[0]?.id ?? values.themeDark;
  const lightSwatches = THEME_SWATCHES[resolvedLightTheme] ?? THEME_SWATCHES["one-light"];
  const darkSwatches = THEME_SWATCHES[resolvedDarkTheme] ?? THEME_SWATCHES["one-dark-pro"];

  return (
    <dialog
      ref={ref}
      aria-label={messages.settings.title}
      className="settings-dialog app-dialog text-[13px]"
      onClick={(event) => {
        if (event.target === ref.current) onClose();
      }}
      onCancel={(event) => {
        event.preventDefault();
        if (!capturingShortcutRef.current) onClose();
      }}
    >
      <div className="flex h-full">
        {/* 左侧搜索 + 分组图标导航 */}
        <nav className="settings-nav flex w-[226px] shrink-0 flex-col" aria-label={messages.settings.title}>
          <div className="px-3.5 pt-3.5 pb-1">
            <h2 className="text-[14px] font-semibold text-[var(--text-primary)]">{messages.settings.title}</h2>
          </div>
          <div className="relative px-2.5 py-2">
            <AppIcon name="search" size={13} className="pointer-events-none absolute top-1/2 left-5 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              className="settings-input w-full pl-7"
              value={query}
              placeholder={messages.settings.search}
              aria-label={messages.settings.search}
              spellCheck={false}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
            {visibleGroups.map((group) => (
              <div key={group.id} className="mb-1.5">
                <div className="truncate px-2.5 pt-2 pb-1 text-[11px] font-semibold text-[var(--text-muted)]">{group.title}</div>
                {group.pages.map((candidate) => (
                  <button
                    key={candidate.id}
                    type="button"
                    data-settings-page={candidate.id}
                    className="settings-nav-item w-full"
                    aria-current={page === candidate.id ? "page" : undefined}
                    onClick={() => setPage(candidate.id)}
                  >
                    <AppIcon name={candidate.icon} size={14} className="shrink-0" />
                    <span className="min-w-0 truncate">{candidate.title}</span>
                  </button>
                ))}
              </div>
            ))}
            {trimmedQuery && visibleGroups.length === 0 ? (
              <p className="px-2.5 py-2 text-[12px] text-[var(--text-muted)]">{messages.settings.searchEmpty}</p>
            ) : null}
          </div>
        </nav>

        {/* 右侧设置内容区 */}
        <div className="flex min-w-0 flex-1 flex-col bg-[var(--surface-bg)]">
          <div className="flex h-[54px] shrink-0 items-center justify-between gap-3 border-b border-[var(--border-subtle)] px-6">
            <div className="min-w-0 flex-1">
              {activeGroup && activePageDef && activeGroup.title !== activePageDef.title ? (
                <p className="truncate text-[11px] text-[var(--text-muted)]">{activeGroup.title}</p>
              ) : null}
              <h3 className="truncate text-[15px] font-semibold text-[var(--text-primary)]">{activePageDef?.title ?? ""}</h3>
            </div>
            <button type="button" className="icon-action shrink-0" aria-label={messages.recovery.close} title={messages.recovery.close} onClick={onClose}>
              <AppIcon name="close" size={15} />
            </button>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
            {error ? (
              <p role="alert" className="mb-4 rounded-[6px] border border-[var(--danger)]/35 bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger)]">
                {formatAppError(error)}
              </p>
            ) : null}
            {notice ? (
              <p role="status" className="mb-4 rounded-[6px] border border-[var(--border-subtle)] bg-[var(--surface-subtle)] px-3 py-2 text-[12px] text-[var(--text-secondary)]">
                {messages.settings.reloaded}
              </p>
            ) : null}

            {/* 1. 外观、代码颜色主题与字体缩放 */}
            {page === "appearance" ? (
              <div className="space-y-4">
                <div className="setting-card space-y-3">
                  <div>
                    <p className="text-[13px] font-semibold text-[var(--text-primary)]">{messages.settings.appearanceMode}</p>
                    <p className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.appearanceModeHint}</p>
                  </div>
                  <div className="mode-grid">
                    {(["system", "light", "dark"] as AppearanceMode[]).map((mode) => {
                      const label =
                        mode === "system"
                          ? messages.settings.appearanceSystem
                          : mode === "light"
                            ? messages.settings.appearanceLight
                            : messages.settings.appearanceDark;
                      return (
                        <button
                          key={mode}
                          type="button"
                          className="mode-option"
                          aria-pressed={values.appearanceMode === mode}
                          onClick={() => void commit({ appearanceMode: mode })}
                        >
                          <div className="mode-preview" aria-hidden>
                            {mode === "system" ? (
                              <>
                                <div className="flex-1 bg-[#F5F6F8] border-r border-[#E4E7EC]" />
                                <div className="flex-1 bg-[#1A1C20]" />
                              </>
                            ) : mode === "light" ? (
                              <div className="flex flex-1 bg-[#FFFFFF]">
                                <div className="w-[30%] bg-[#F5F6F8] border-r border-[#E4E7EC]" />
                                <div className="flex-1 p-2"><div className="h-1.5 w-3/5 rounded bg-[#2563EB]" /></div>
                              </div>
                            ) : (
                              <div className="flex flex-1 bg-[#1A1C20]">
                                <div className="w-[30%] bg-[#141518] border-r border-[#262931]" />
                                <div className="flex-1 p-2"><div className="h-1.5 w-3/5 rounded bg-[#3B82F6]" /></div>
                              </div>
                            )}
                          </div>
                          <span className="text-[12px] font-medium text-[var(--text-primary)]">{label}</span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.codeThemesTitle}</p>
                    <p className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.themeHint}</p>
                  </div>
                  <SettingRow label={messages.settings.lightTheme}>
                    <div className="flex items-center gap-2">
                      <span className="swatch-strip" aria-hidden>
                        {lightSwatches.map((hex) => <span key={hex} className="swatch-dot" style={{ background: hex }} />)}
                      </span>
                      <select
                        className="settings-select w-[200px]"
                        value={resolvedLightTheme}
                        onChange={(event) => void commit({ themeLight: event.target.value })}
                      >
                        {lightThemes.map((theme) => (
                          <option key={theme.id} value={theme.id}>{theme.label}</option>
                        ))}
                      </select>
                    </div>
                  </SettingRow>
                  <SettingRow label={messages.settings.darkTheme}>
                    <div className="flex items-center gap-2">
                      <span className="swatch-strip" aria-hidden>
                        {darkSwatches.map((hex) => <span key={hex} className="swatch-dot" style={{ background: hex }} />)}
                      </span>
                      <select
                        className="settings-select w-[200px]"
                        value={resolvedDarkTheme}
                        onChange={(event) => void commit({ themeDark: event.target.value })}
                      >
                        {darkThemes.map((theme) => (
                          <option key={theme.id} value={theme.id}>{theme.label}</option>
                        ))}
                      </select>
                    </div>
                  </SettingRow>
                </div>

                <M7ThemeSettings onThemesChanged={(updated) => onThemesChanged?.(updated)} />

                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.pages.font}</p>
                  </div>
                  <SettingRow label={messages.settings.codeFont} hint={messages.settings.codeFontHint}>
                    <input
                      className="settings-input w-[210px] font-mono text-[12px]"
                      value={font}
                      placeholder="JetBrains Mono"
                      spellCheck={false}
                      onChange={(event) => setFont(event.target.value)}
                      onBlur={() => { if (font !== values.codeFontFamily) void commit({ codeFontFamily: font }); }}
                      onKeyDown={(event) => { if (event.key === "Enter" && font !== values.codeFontFamily) void commit({ codeFontFamily: font }); }}
                    />
                  </SettingRow>
                  <SettingRow label={messages.settings.fontSize} hint={messages.settings.fontSizeHint}>
                    <div className="flex items-center gap-1.5">
                      <input
                        className="settings-input w-20 text-center font-mono"
                        type="number"
                        min={9}
                        max={24}
                        value={size}
                        onChange={(event) => setSize(event.target.value)}
                        onBlur={() => void commitNumber(size, values.fontSize, (value) => ({ diffFontSize: value }), setSize, 9, 24)}
                        onKeyDown={(event) => { if (event.key === "Enter") void commitNumber(size, values.fontSize, (value) => ({ diffFontSize: value }), setSize, 9, 24); }}
                      />
                      <span className="text-[12px] text-[var(--text-muted)]">px</span>
                    </div>
                  </SettingRow>
                  <SettingRow label={messages.settings.uiZoom} hint={messages.settings.uiZoomHint}>
                    <div className="flex items-center gap-2">
                      <input
                        className="settings-input w-20 text-center font-mono"
                        type="number"
                        min={80}
                        max={150}
                        step={10}
                        value={zoom}
                        onChange={(event) => setZoom(event.target.value)}
                        onBlur={() => void commitNumber(zoom, values.uiZoom, (value) => ({ uiZoom: value }), setZoom, 80, 150)}
                        onKeyDown={(event) => { if (event.key === "Enter") void commitNumber(zoom, values.uiZoom, (value) => ({ uiZoom: value }), setZoom, 80, 150); }}
                      />
                      <span className="text-[12px] text-[var(--text-muted)]">%</span>
                      <button
                        type="button"
                        className="revia-btn revia-btn-secondary h-7 px-2.5"
                        onClick={() => {
                          setZoom("100");
                          void commit({ uiZoom: 100 });
                        }}
                      >
                        {messages.settings.uiZoomReset}
                      </button>
                    </div>
                  </SettingRow>
                </div>
              </div>
            ) : null}

            {/* 2. 界面语言 */}
            {page === "language" ? (
              <div className="setting-card-list">
                <SettingRow label={messages.settings.language} hint={messages.settings.languageHint}>
                  <select
                    className="settings-select w-[200px]"
                    value={values.language}
                    onChange={(event) => void commit({ language: event.target.value === "auto" ? "" : event.target.value })}
                  >
                    <option value="auto">{messages.settings.languageAuto}</option>
                    <option value="zh-CN">{messages.settings.languageChinese}</option>
                    <option value="en">{messages.settings.languageEnglish}</option>
                  </select>
                </SettingRow>
              </div>
            ) : null}

            {/* 3. 快捷键 */}
            {page === "shortcuts" ? (
              <M7ShortcutSettings
                shortcuts={values.shortcuts ?? {}}
                onChange={(next) => commit({ shortcuts: next })}
                onCapturingChange={(active) => { capturingShortcutRef.current = active; }}
              />
            ) : null}

            {/* 4. 通知 */}
            {page === "notifications" ? (
              <div className="space-y-3">
                <p className="text-[12px] text-[var(--text-secondary)]">{messages.settings.notifyHint}</p>
                <div className="setting-card-list">
                  {(["stage", "discard", "commit", "recovery"] as const).map((kind) => (
                    <SwitchRow
                      key={kind}
                      label={notifyKindText(kind)}
                      checked={values.notify[kind]}
                      onChange={(value) => void commit({ notify: { ...values.notify, [kind]: value } })}
                    />
                  ))}
                </div>
              </div>
            ) : null}

            {/* 5. 提交（工作模式 + 提交信息检查 + 提交前警告） */}
            {page === "commit" ? (
              <div className="space-y-4">
                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.pages.commitMessage}</p>
                  </div>
                  <SwitchRow
                    label={t("changelists.mode")}
                    checked={values.changelistMode}
                    onChange={(value) => void commit({ changelistMode: value })}
                  />
                  <SwitchRow
                    label={messages.settings.cherryPickKeepEmpty}
                    hint={messages.settings.cherryPickKeepEmptyHint}
                    checked={values.cherryPickKeepEmpty}
                    onChange={(value) => void commit({ cherryPickKeepEmpty: value })}
                  />
                  <SwitchRow
                    label={messages.settings.cherryPickIncludeSource}
                    hint={messages.settings.cherryPickIncludeSourceHint}
                    checked={values.cherryPickIncludeSource}
                    onChange={(value) => void commit({ cherryPickIncludeSource: value })}
                  />
                </div>

                <div className="setting-card-list">
                  <SettingRow label={messages.settings.commitSubjectLimit} hint={messages.settings.commitSubjectLimitHint}>
                    <input
                      className="settings-input w-20 text-center font-mono"
                      type="number"
                      min={0}
                      max={200}
                      value={subjectLimit}
                      onChange={(event) => setSubjectLimit(event.target.value)}
                      onBlur={() => void commitNumber(subjectLimit, values.commitSubjectLimit, (value) => ({ commitSubjectLimit: value }), setSubjectLimit, 0, 200)}
                      onKeyDown={(event) => { if (event.key === "Enter") void commitNumber(subjectLimit, values.commitSubjectLimit, (value) => ({ commitSubjectLimit: value }), setSubjectLimit, 0, 200); }}
                    />
                  </SettingRow>
                  <SettingRow label={messages.settings.commitBodyLineLimit} hint={messages.settings.commitBodyLineLimitHint}>
                    <input
                      className="settings-input w-20 text-center font-mono"
                      type="number"
                      min={0}
                      max={200}
                      value={bodyLimit}
                      onChange={(event) => setBodyLimit(event.target.value)}
                      onBlur={() => void commitNumber(bodyLimit, values.commitBodyLineLimit, (value) => ({ commitBodyLineLimit: value }), setBodyLimit, 0, 200)}
                      onKeyDown={(event) => { if (event.key === "Enter") void commitNumber(bodyLimit, values.commitBodyLineLimit, (value) => ({ commitBodyLineLimit: value }), setBodyLimit, 0, 200); }}
                    />
                  </SettingRow>
                  <SwitchRow
                    label={messages.settings.commitBlankLineRequired}
                    hint={messages.settings.commitBlankLineRequiredHint}
                    checked={values.commitBlankLineRequired}
                    onChange={(value) => void commit({ commitBlankLineRequired: value })}
                  />
                  <SwitchRow
                    label={messages.settings.commitShowRuler}
                    hint={messages.settings.commitShowRulerHint}
                    checked={values.commitShowRuler}
                    onChange={(value) => void commit({ commitShowRuler: value })}
                  />
                </div>

                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.pages.commitWarnings}</p>
                    <p className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.commitWarningsHint}</p>
                  </div>
                  <SwitchRow
                    label={messages.settings.commitWarnCrlf}
                    checked={values.commitWarnCrlf}
                    onChange={(value) => void commit({ commitWarnCrlf: value })}
                  />
                  <SwitchRow
                    label={messages.settings.commitWarnDetached}
                    checked={values.commitWarnDetached}
                    onChange={(value) => void commit({ commitWarnDetached: value })}
                  />
                  <SwitchRow
                    label={messages.settings.commitWarnInvalidFilename}
                    checked={values.commitWarnInvalidFilename}
                    onChange={(value) => void commit({ commitWarnInvalidFilename: value })}
                  />
                  <SettingRow label={messages.settings.commitWarnLargeFile} hint={messages.settings.commitWarnLargeFileHint}>
                    <div className="flex items-center gap-1.5">
                      <input
                        className="settings-input w-24 text-center font-mono"
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
                      <span className="text-[12px] text-[var(--text-muted)]">MB</span>
                    </div>
                  </SettingRow>
                </div>
              </div>
            ) : null}

            {/* 6. Diff 与合并 */}
            {page === "diff" ? (
              <div className="space-y-4">
                <div className="setting-card-list">
                  <SettingRow label={messages.settings.diffContextLines} hint={messages.settings.diffContextLinesHint}>
                    <input
                      className="settings-input w-20 text-center font-mono"
                      type="number"
                      min={0}
                      max={64}
                      value={contextLines}
                      onChange={(event) => setContextLines(event.target.value)}
                      onBlur={() => void commitNumber(contextLines, values.diffContextLines, (value) => ({ diffContextLines: value }), setContextLines, 0, 64)}
                      onKeyDown={(event) => { if (event.key === "Enter") void commitNumber(contextLines, values.diffContextLines, (value) => ({ diffContextLines: value }), setContextLines, 0, 64); }}
                    />
                  </SettingRow>
                  <SettingRow label={messages.diff.highlightLabel}>
                    <select
                      className="settings-select w-[190px]"
                      value={values.diffHighlight}
                      onChange={(event) => void commit({ diffHighlight: event.target.value as SettingsValues["diffHighlight"] })}
                    >
                      {(["word", "line", "char", "none"] as const).map((level) => (
                        <option key={level} value={level}>{highlightLevelText(level)}</option>
                      ))}
                    </select>
                  </SettingRow>
                  <SettingRow label={messages.diff.ignoreWhitespace} hint={messages.diff.ignoreWhitespaceReadonly}>
                    <select
                      className="settings-select w-[190px]"
                      value={values.diffIgnoreWhitespace}
                      onChange={(event) => void commit({ diffIgnoreWhitespace: Number(event.target.value) })}
                    >
                      <option value={0}>{messages.diff.ignoreWhitespaceLevels.off}</option>
                      <option value={1}>{messages.diff.ignoreWhitespaceLevels.eol}</option>
                      <option value={2}>{messages.diff.ignoreWhitespaceLevels.all}</option>
                      <option value={3}>{messages.diff.ignoreWhitespaceLevels.allBlank}</option>
                    </select>
                  </SettingRow>
                </div>

                <div className="setting-card-list">
                  <SwitchRow label={messages.diff.foldUnchanged} checked={values.diffFoldUnchanged} onChange={(value) => void commit({ diffFoldUnchanged: value })} />
                  <SwitchRow label={messages.diff.syncScroll} checked={values.diffSyncScroll} onChange={(value) => void commit({ diffSyncScroll: value })} />
                  <SwitchRow label={messages.diff.showWhitespace} checked={values.diffShowWhitespace} onChange={(value) => void commit({ diffShowWhitespace: value })} />
                  <SwitchRow label={messages.diff.softWrap} checked={values.diffSoftWrap} onChange={(value) => void commit({ diffSoftWrap: value })} />
                  <SwitchRow label={messages.diff.jumpToNextFile} checked={values.diffJumpToNextFile} onChange={(value) => void commit({ diffJumpToNextFile: value })} />
                  <SwitchRow
                    label={messages.settings.mergeAutoApply}
                    hint={messages.settings.mergeAutoApplyHint}
                    checked={values.mergeAutoApply}
                    onChange={(value) => void commit({ mergeAutoApply: value })}
                  />
                </div>
              </div>
            ) : null}

            {/* 7. 提交日志与文件历史 */}
            {page === "log" ? (
              <div className="space-y-4">
                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.logColumns}</p>
                    <p className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.logColumnsHint}</p>
                  </div>
                  {([
                    ["author", messages.settings.logColumnAuthor],
                    ["date", messages.settings.logColumnDate],
                    ["hash", messages.settings.logColumnHash],
                  ] as const).map(([key, label]) => (
                    <SwitchRow
                      key={key}
                      label={label}
                      checked={values.logColumns[key]}
                      onChange={(value) => void commit({ logColumns: { ...values.logColumns, [key]: value } })}
                    />
                  ))}
                </div>

                <div className="setting-card-list">
                  <SwitchRow
                    label={messages.settings.logLongEdges}
                    hint={messages.settings.logLongEdgesHint}
                    checked={values.logLongEdges}
                    onChange={(value) => void commit({ logLongEdges: value })}
                  />
                  <SwitchRow
                    label={messages.settings.logFirstParent}
                    hint={messages.settings.logFirstParentHint}
                    checked={values.logFirstParent}
                    onChange={(value) => void commit({ logFirstParent: value })}
                  />
                  <SettingRow label={messages.settings.logPreviewPosition}>
                    <select
                      className="settings-select w-[190px]"
                      value={values.logPreviewPosition}
                      onChange={(event) => void commit({ logPreviewPosition: event.target.value as LogPreviewPosition })}
                    >
                      <option value="right">{messages.settings.logPreviewRight}</option>
                      <option value="bottom">{messages.settings.logPreviewBottom}</option>
                    </select>
                  </SettingRow>
                  <SwitchRow
                    label={messages.settings.fileHistoryShowNames}
                    hint={messages.settings.fileHistoryShowNamesHint}
                    checked={values.fileHistoryShowNames}
                    onChange={(value) => void commit({ fileHistoryShowNames: value })}
                  />
                </div>
                <p className="text-[11.5px] text-[var(--text-muted)]">{messages.settings.logHint}</p>
              </div>
            ) : null}

            {/* 8. Git、外部工具与更新/推送 */}
            {page === "git" ? (
              <div className="space-y-4">
                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.pages.updatePush}</p>
                  </div>
                  <SettingRow label={messages.settings.updateMode} hint={messages.settings.updateModeHint}>
                    <select
                      className="settings-select w-[190px]"
                      value={values.updateMode}
                      onChange={(event) => void commit({ updateMode: event.target.value as UpdateMode })}
                    >
                      <option value="merge">{messages.settings.updateModeMerge}</option>
                      <option value="rebase">{messages.settings.updateModeRebase}</option>
                      <option value="branchDefault">{messages.settings.updateModeBranchDefault}</option>
                    </select>
                  </SettingRow>
                  <SettingRow label={messages.settings.fetchTagMode}>
                    <select
                      className="settings-select w-[190px]"
                      value={values.fetchTagMode}
                      onChange={(event) => void commit({ fetchTagMode: event.target.value as FetchTagMode })}
                    >
                      <option value="auto">{messages.settings.fetchTagModeAuto}</option>
                      <option value="sync">{messages.settings.fetchTagModeSync}</option>
                      <option value="always">{messages.settings.fetchTagModeAlways}</option>
                      <option value="never">{messages.settings.fetchTagModeNever}</option>
                    </select>
                  </SettingRow>
                  <SwitchRow
                    label={messages.settings.fetchAutomatically}
                    hint={messages.settings.fetchAutomaticallyHint}
                    checked={values.fetchAutomatically}
                    onChange={(value) => void commit({ fetchAutomatically: value })}
                  />
                  <SwitchRow
                    label={messages.settings.checkRemote}
                    hint={messages.settings.checkRemoteHint}
                    checked={values.checkRemote}
                    onChange={(value) => void commit({ checkRemote: value })}
                  />
                  <SwitchRow
                    label={messages.settings.autoUpdateOnPushRejection}
                    hint={messages.settings.autoUpdateOnPushRejectionHint}
                    checked={values.autoUpdateOnPushRejection}
                    onChange={(value) => void commit({ autoUpdateOnPushRejection: value })}
                  />
                  <SwitchRow
                    label={messages.settings.pushDialogOnCommit}
                    checked={values.pushDialogOnCommit}
                    onChange={(value) => void commit({ pushDialogOnCommit: value })}
                  />
                  <SwitchRow
                    label={messages.settings.pushDialogProtectedOnly}
                    checked={values.pushDialogProtectedOnly}
                    onChange={(value) => void commit({ pushDialogProtectedOnly: value })}
                  />
                  <div className="setting-row-stack">
                    <div>
                      <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{messages.settings.protectedBranches}</div>
                      <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.protectedBranchesHint}</div>
                    </div>
                    <textarea
                      className="settings-input min-h-[72px] w-full py-2 font-mono text-[12px]"
                      value={protectedBranches}
                      spellCheck={false}
                      onChange={(event) => setProtectedBranches(event.target.value)}
                      onBlur={() => {
                        const normalized = protectedBranches
                          .split(/\r?\n/)
                          .map((line) => line.trim())
                          .filter((line) => line.length > 0);
                        if (normalized.join("\n") !== values.protectedBranches) {
                          void commit({ protectedBranches: normalized });
                        }
                      }}
                    />
                  </div>
                </div>

                <div className="setting-card-list">
                  <div className="setting-card-head">
                    <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{messages.settings.pages.gitTools}</p>
                  </div>
                  <SwitchRow
                    label={messages.stash.compareLocal}
                    hint={messages.settings.stashComparisonHint}
                    checked={values.stashCompareLocal}
                    onChange={(value) => void commit({ stashCompareLocal: value })}
                  />
                  <div className="setting-row-stack">
                    <div>
                      <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{messages.settings.gitPath}</div>
                      <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">
                        {values.detectedGitPath ? `${messages.settings.detectedGit}${values.detectedGitPath}` : messages.settings.gitPathHint}
                      </div>
                    </div>
                    <input
                      className="settings-input font-mono text-[12px]"
                      value={gitPath}
                      placeholder={messages.settings.gitPathPlaceholder}
                      spellCheck={false}
                      onChange={(event) => setGitPath(event.target.value)}
                      onBlur={() => { if (gitPath !== values.gitPath) void commit({ gitPath }); }}
                      onKeyDown={(event) => { if (event.key === "Enter" && gitPath !== values.gitPath) void commit({ gitPath }); }}
                    />
                  </div>
                  <div className="setting-row-stack">
                    <div>
                      <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{messages.settings.mergeTool}</div>
                      <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.mergeToolHint}</div>
                    </div>
                    <input
                      className="settings-input font-mono text-[12px]"
                      value={mergeTool}
                      placeholder={messages.settings.toolPlaceholder}
                      spellCheck={false}
                      onChange={(event) => setMergeTool(event.target.value)}
                      onBlur={() => { if (mergeTool !== values.mergeTool) void commit({ mergeTool }); }}
                      onKeyDown={(event) => { if (event.key === "Enter" && mergeTool !== values.mergeTool) void commit({ mergeTool }); }}
                    />
                  </div>
                  <div className="setting-row-stack">
                    <div>
                      <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{messages.settings.diffTool}</div>
                      <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{messages.settings.diffToolHint}</div>
                    </div>
                    <input
                      className="settings-input font-mono text-[12px]"
                      value={diffTool}
                      placeholder={messages.settings.toolPlaceholder}
                      spellCheck={false}
                      onChange={(event) => setDiffTool(event.target.value)}
                      onBlur={() => { if (diffTool !== values.diffTool) void commit({ diffTool }); }}
                      onKeyDown={(event) => { if (event.key === "Enter" && diffTool !== values.diffTool) void commit({ diffTool }); }}
                    />
                  </div>
                  <SettingRow label={messages.settings.reload} hint={messages.settings.reloadHint}>
                    <button
                      type="button"
                      className="revia-btn revia-btn-secondary shrink-0"
                      onClick={() => {
                        void onReloadEnvironment().then((failure) => {
                          setError(failure);
                          setNotice(!failure);
                        });
                      }}
                    >
                      <AppIcon name="refresh" size={13} />
                      <span>{messages.settings.reload}</span>
                    </button>
                  </SettingRow>
                </div>
              </div>
            ) : null}

            {/* 9. AI 生成提交信息 */}
            {page === "ai" ? (
              <div className="space-y-4">
                <p className="text-[12px] leading-relaxed text-[var(--text-secondary)]">{t("settings.ai.description")}</p>

                <div className="setting-card-list">
                  <div className="setting-row-stack">
                    <div>
                      <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{t("settings.ai.baseUrl")}</div>
                      <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("settings.ai.baseUrlHint")}</div>
                    </div>
                    <input
                      data-ai-field="baseUrl"
                      className="settings-input font-mono text-[12px]"
                      value={aiView.draft.baseUrl}
                      spellCheck={false}
                      onChange={(event) => aiSettings.edit("baseUrl", event.target.value)}
                      onBlur={() => { void aiSettings.save("baseUrl"); }}
                    />
                  </div>

                  <div className="setting-row-stack">
                    <div>
                      <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{t("settings.ai.model")}</div>
                      <div className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("settings.ai.modelHint")}</div>
                    </div>
                    <input
                      data-ai-field="model"
                      className="settings-input font-mono text-[12px]"
                      value={aiView.draft.model}
                      spellCheck={false}
                      onChange={(event) => aiSettings.edit("model", event.target.value)}
                      onBlur={() => { void aiSettings.save("model"); }}
                    />
                    <div className="flex flex-wrap items-center gap-2 text-[11.5px] text-[var(--text-secondary)]">
                      {aiView.models.state === "loading" ? <span role="status">{t("settings.ai.modelsLoading")}</span> : null}
                      {aiView.models.state === "ready" ? (
                        <>
                          <select
                            className="settings-select w-[240px]"
                            aria-label={t("settings.ai.modelsChoose")}
                            value=""
                            onChange={(event) => { if (event.target.value) void aiSettings.selectModel(event.target.value); }}
                          >
                            <option value="">{t("settings.ai.modelsChoose")}</option>
                            {aiView.models.items.map((model) => (
                              <option key={model} value={model}>{model}</option>
                            ))}
                          </select>
                          <button type="button" className="revia-btn revia-btn-secondary h-7 px-2.5" onClick={() => aiSettings.retryModels()}>
                            <AppIcon name="refresh" size={12} />
                            <span>{t("settings.ai.modelsRefresh")}</span>
                          </button>
                        </>
                      ) : null}
                      {aiView.models.state === "empty" ? <span>{t("settings.ai.modelsEmpty")}</span> : null}
                      {aiView.models.state === "error" && aiView.models.error ? (
                        <span role="alert" className="text-[var(--danger)]">{formatAppError(aiView.models.error)}</span>
                      ) : null}
                      {aiView.models.state === "empty" || aiView.models.state === "error" ? (
                        <button type="button" className="revia-btn revia-btn-secondary h-7 px-2.5" onClick={() => aiSettings.retryModels()}>
                          {t("settings.ai.modelsRetry")}
                        </button>
                      ) : null}
                    </div>
                  </div>

                  <SettingRow label={t("settings.ai.language")}>
                    <select
                      data-ai-field="language"
                      className="settings-select w-[190px]"
                      value={aiView.draft.language}
                      onChange={(event) => { void aiSettings.selectLanguage(event.target.value as AiLanguage); }}
                    >
                      <option value="repo">{t("settings.ai.languageRepo")}</option>
                      <option value="chinese">{t("settings.ai.languageChinese")}</option>
                      <option value="english">{t("settings.ai.languageEnglish")}</option>
                    </select>
                  </SettingRow>
                </div>

                <div className="setting-card space-y-2">
                  <div className="text-[12.5px] font-semibold text-[var(--text-primary)]">{t("settings.ai.key")}</div>
                  <div className="flex items-center gap-2">
                    <input
                      className="settings-input min-w-0 flex-1"
                      type="password"
                      autoComplete="off"
                      spellCheck={false}
                      value={aiKey}
                      placeholder={aiView.keySet ? t("settings.ai.keySet") : t("settings.ai.keyUnset")}
                      onChange={(event) => updateAiKey(event.target.value)}
                    />
                    <button
                      type="button"
                      className="revia-btn revia-btn-primary shrink-0"
                      disabled={!aiKey.trim()}
                      onClick={() => runAiKeyChange(() => aiSettings.saveKey(aiKey))}
                    >
                      {t("settings.ai.saveKey")}
                    </button>
                    <button
                      type="button"
                      className="revia-btn revia-btn-secondary shrink-0"
                      disabled={!aiView.keySet}
                      onClick={() => runAiKeyChange(() => aiSettings.clearKey())}
                    >
                      {t("settings.ai.clearKey")}
                    </button>
                  </div>
                  <p className="text-[11.5px] leading-relaxed text-[var(--text-muted)]">
                    {aiView.keySet ? t("settings.ai.keySet") : t("settings.ai.keyUnset")} · {t("settings.ai.keyHint")} {t("settings.ai.keyBinding")}
                  </p>
                </div>

                <div className="setting-card space-y-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[12.5px] font-semibold text-[var(--text-primary)]">{t("settings.ai.prompt")}</span>
                    <button type="button" className="revia-btn revia-btn-secondary h-7 px-2.5" onClick={() => void aiSettings.resetPrompt()}>
                      {t("settings.ai.resetPrompt")}
                    </button>
                  </div>
                  <textarea
                    className="settings-input min-h-36 w-full py-2 font-mono text-[12px] leading-relaxed"
                    value={aiView.draft.prompt}
                    spellCheck={false}
                    onChange={(event) => aiSettings.edit("prompt", event.target.value)}
                    onBlur={() => { void aiSettings.save("prompt"); }}
                  />
                  <p className="text-[11.5px] leading-relaxed text-[var(--text-muted)]">{t("settings.ai.promptHint")}</p>
                </div>

                {!aiView.configured ? <p className="text-[12px] text-[var(--text-muted)]">{t("settings.ai.unconfigured")}</p> : null}
                {aiView.notice ? (
                  <p role="status" className="text-[12px] text-[var(--reviewed)]">
                    {t(aiView.notice === "keySet" ? "settings.ai.keySet" : "settings.ai.keyUnset")}
                  </p>
                ) : null}
                {aiView.error ? (
                  <p role="alert" className="rounded-[6px] border border-[var(--danger)]/35 bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger)]">
                    {formatAppError(aiView.error)}
                  </p>
                ) : null}
              </div>
            ) : null}

            {/* 10. GitHub 账号 */}
            {page === "github" ? (
              <M7GithubSettings worktreeId={worktreeId} onChangeAccount={() => onAccountChanged?.()} />
            ) : null}

            {/* 11. 应用更新与诊断 */}
            {page === "updates" ? (
              <div className="space-y-4">
                <M7UpdateSettings
                  mode={values.appUpdateMode ?? "notify"}
                  onChangeMode={(mode) => commit({ appUpdateMode: mode })}
                  allowPrerelease={values.allowPrerelease ?? false}
                  onChangeAllowPrerelease={(value) => commit({ allowPrerelease: value })}
                  status={updateStatus}
                  onStatusChange={(status) => onUpdateStatus?.(status)}
                  onOpenUpdatePanel={
                    onOpenUpdatePanel
                      ? () => {
                          onClose();
                          onOpenUpdatePanel();
                        }
                      : undefined
                  }
                />
                {onOpenDiagnostics ? (
                  <div className="setting-card flex items-center justify-between gap-4">
                    <div className="min-w-0">
                      <p className="text-[12.5px] font-semibold text-[var(--text-primary)]">{t("diagnostics.title")}</p>
                      <p className="mt-0.5 text-[11.5px] text-[var(--text-muted)]">{t("diagnostics.hint")}</p>
                    </div>
                    <button
                      type="button"
                      className="revia-btn revia-btn-secondary shrink-0"
                      onClick={() => {
                        onClose();
                        onOpenDiagnostics();
                      }}
                    >
                      <AppIcon name="output" size={14} />
                      <span>{messages.settings.openDiagnostics}</span>
                    </button>
                  </div>
                ) : null}
              </div>
            ) : null}

            {/* 12. 关于 Revia */}
            {page === "about" ? (
              <div className="space-y-4 text-[12.5px] leading-relaxed">
                <div className="setting-card flex items-start gap-3.5">
                  <div className="grid size-10 shrink-0 place-items-center rounded-[9px] bg-[var(--accent)] text-[17px] font-bold text-white shadow-sm">
                    R
                  </div>
                  <div className="min-w-0 space-y-1">
                    <p className="text-[15px] font-semibold text-[var(--text-primary)]">Revia</p>
                    <p className="text-[var(--text-secondary)]">{messages.settings.aboutText}</p>
                    {measure?.measure ? (
                      <p className="text-[var(--text-muted)]">{t("settings.measureBuild", { commit: measure.commit })}</p>
                    ) : null}
                  </div>
                </div>
                <div className="setting-card space-y-2">
                  <p className="font-semibold text-[var(--text-primary)]">{messages.settings.licenses}</p>
                  <ul className="list-disc space-y-1 pl-5 text-[var(--text-secondary)]">
                    <li>{messages.settings.licenseLucide}</li>
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

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div className="min-w-0 flex-1">
        <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{label}</div>
        {hint ? <div className="mt-0.5 text-[11.5px] leading-relaxed text-[var(--text-muted)]">{hint}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function SwitchRow({
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
    <div className="setting-row">
      <div className="min-w-0 flex-1">
        <div className="text-[12.5px] font-medium text-[var(--text-primary)]">{label}</div>
        {hint ? <div className="mt-0.5 text-[11.5px] leading-relaxed text-[var(--text-muted)]">{hint}</div> : null}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        className="setting-switch"
        onClick={() => onChange(!checked)}
      />
    </div>
  );
}
