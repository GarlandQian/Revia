import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ipc, type AppearanceMode } from "@/lib/ipc";
import {
  DEFAULT_DARK_THEME,
  DEFAULT_LIGHT_THEME,
  applyCodeFont,
  applyTheme,
  resolveTheme,
  setImportedThemes,
  systemPrefersDark,
  watchSystemTheme,
} from "@/lib/theme";
import { m7 } from "@/lib/m7/api";

export function useAppearance() {
  const [appearanceMode, setAppearanceMode] = useState<AppearanceMode>("system");
  const [themeLight, setThemeLight] = useState(DEFAULT_LIGHT_THEME);
  const [themeDark, setThemeDark] = useState(DEFAULT_DARK_THEME);
  const [codeFontFamily, setCodeFontFamily] = useState<string | null>(null);
  const [fontSize, setFontSize] = useState(12);
  const [uiZoom, setUiZoom] = useState(100);
  const uiZoomRef = useRef(uiZoom);
  uiZoomRef.current = uiZoom;
  const [prefersDark, setPrefersDark] = useState(() => systemPrefersDark());
  const [importedRevision, setImportedRevision] = useState(0);

  const activeTheme = useMemo(
    () => resolveTheme(themeLight, themeDark, prefersDark, appearanceMode),
    [themeLight, themeDark, prefersDark, appearanceMode, importedRevision],
  );

  useEffect(() => {
    applyTheme(activeTheme);
  }, [activeTheme]);

  const reloadImportedThemes = useCallback(async (updated?: { themeLight?: string; themeDark?: string }) => {
    try {
      const list = await m7.listImportedThemes();
      setImportedThemes(list);
    } catch {
      setImportedThemes([]);
    }
    if (updated?.themeLight) setThemeLight(updated.themeLight);
    if (updated?.themeDark) setThemeDark(updated.themeDark);
    setImportedRevision((value) => value + 1);
  }, []);

  useEffect(() => {
    void reloadImportedThemes();
  }, [reloadImportedThemes]);

  useEffect(() => watchSystemTheme(setPrefersDark), []);

  useEffect(() => {
    applyCodeFont(codeFontFamily, fontSize);
  }, [codeFontFamily, fontSize]);

  useEffect(() => {
    document.documentElement.style.setProperty("--ui-zoom", String(uiZoom / 100));
  }, [uiZoom]);

  const changeFontSize = useCallback(async (delta: number) => {
    const next = Math.min(24, Math.max(9, fontSize + delta));
    setFontSize(next);
    try {
      await ipc.setDiffFontSize(next);
    } catch {
      // 字号保存失败不影响本次阅读。
    }
  }, [fontSize]);

  return {
    appearanceMode, setAppearanceMode,
    themeLight, setThemeLight, themeDark, setThemeDark,
    activeCodeThemeLabel: activeTheme.label,
    codeFontFamily, setCodeFontFamily, fontSize, setFontSize, changeFontSize,
    uiZoom, setUiZoom, uiZoomRef,
    reloadImportedThemes,
  };
}
