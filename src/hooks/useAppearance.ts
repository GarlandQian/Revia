import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "@/lib/ipc";
import {
  DEFAULT_DARK_THEME,
  DEFAULT_LIGHT_THEME,
  applyCodeFont,
  applyTheme,
  resolveTheme,
  systemPrefersDark,
  watchSystemTheme,
} from "@/lib/theme";

export function useAppearance() {
  const [themeLight, setThemeLight] = useState(DEFAULT_LIGHT_THEME);
  const [themeDark, setThemeDark] = useState(DEFAULT_DARK_THEME);
  const [codeFontFamily, setCodeFontFamily] = useState<string | null>(null);
  const [fontSize, setFontSize] = useState(12);
  const [uiZoom, setUiZoom] = useState(100);
  const uiZoomRef = useRef(uiZoom);
  uiZoomRef.current = uiZoom;
  const [prefersDark, setPrefersDark] = useState(() => systemPrefersDark());

  useEffect(() => {
    applyTheme(resolveTheme(themeLight, themeDark, prefersDark));
  }, [themeLight, themeDark, prefersDark]);

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
  };
}
