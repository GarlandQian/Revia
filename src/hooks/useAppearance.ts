import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "@/lib/ipc";
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
  const [themeLight, setThemeLight] = useState(DEFAULT_LIGHT_THEME);
  const [themeDark, setThemeDark] = useState(DEFAULT_DARK_THEME);
  const [codeFontFamily, setCodeFontFamily] = useState<string | null>(null);
  const [fontSize, setFontSize] = useState(12);
  const [uiZoom, setUiZoom] = useState(100);
  const uiZoomRef = useRef(uiZoom);
  uiZoomRef.current = uiZoom;
  const [prefersDark, setPrefersDark] = useState(() => systemPrefersDark());
  /** 导入主题的版本号：重新读取后强制再应用一次（M7 任务 02）。 */
  const [importedRevision, setImportedRevision] = useState(0);

  useEffect(() => {
    applyTheme(resolveTheme(themeLight, themeDark, prefersDark));
  }, [themeLight, themeDark, prefersDark, importedRevision]);

  /** 从 Rust 读取导入主题；失败时只保留内置主题，不阻塞界面。 */
  const reloadImportedThemes = useCallback(async () => {
    try {
      const list = await m7.listImportedThemes();
      setImportedThemes(list);
    } catch {
      setImportedThemes([]);
    }
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
    themeLight, setThemeLight, themeDark, setThemeDark,
    codeFontFamily, setCodeFontFamily, fontSize, setFontSize, changeFontSize,
    uiZoom, setUiZoom, uiZoomRef,
    reloadImportedThemes,
  };
}
