/**
 * 玻璃材质的运行期偏好（M7 任务 01、Q189–Q191）。
 *
 * React Bits 的三个组件需要知道两件 CSS 表达不了的事：
 * - 当前外观是浅色还是深色（`html[data-kind]`，导入主题时由 theme.ts 写入）；
 * - 用户是否要求减少动态效果（高光跟随鼠标、入场动画要关掉）。
 *
 * 减少透明度与高对比度只影响材质本身，放在 CSS 媒体查询里处理（index.css）。
 */

import { useEffect, useState } from "react";

export type ThemeKind = "light" | "dark";

function currentKind(): ThemeKind {
  return document.documentElement.dataset.kind === "dark" ? "dark" : "light";
}

/** 跟随 `html[data-kind]` 变化，切换主题时玻璃组件同步换配色。 */
export function useThemeKind(): ThemeKind {
  const [kind, setKind] = useState<ThemeKind>(currentKind);

  useEffect(() => {
    const root = document.documentElement;
    const sync = () => setKind(currentKind());
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(root, { attributes: true, attributeFilter: ["data-kind"] });
    return () => observer.disconnect();
  }, []);

  return kind;
}

function supportsBackdropFilter(): boolean {
  if (typeof window === "undefined") return false;
  return (
    window.CSS?.supports?.("backdrop-filter", "blur(1px)") === true ||
    window.CSS?.supports?.("-webkit-backdrop-filter", "blur(1px)") === true
  );
}

function flatGlassNow(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  if (window.matchMedia("(prefers-reduced-transparency: reduce)").matches) return true;
  if (window.matchMedia("(forced-colors: active)").matches) return true;
  return !supportsBackdropFilter();
}

/**
 * 材质是否整体退化为实色：减少透明度、高对比度，或 WebView 不支持背景滤镜。
 * 与 index.css 的媒体查询保持同一判据，组件只据此关掉折射与高光。
 */
export function useFlatGlass(): boolean {
  const [flat, setFlat] = useState(flatGlassNow);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const queries = [
      window.matchMedia("(prefers-reduced-transparency: reduce)"),
      window.matchMedia("(forced-colors: active)"),
    ];
    const sync = () => setFlat(flatGlassNow());
    sync();
    for (const query of queries) query.addEventListener("change", sync);
    return () => {
      for (const query of queries) query.removeEventListener("change", sync);
    };
  }, []);

  return flat;
}

function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** `prefers-reduced-motion: reduce`：关掉高光跟随与入场动画。 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(prefersReducedMotion);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const sync = () => setReduced(query.matches);
    sync();
    query.addEventListener("change", sync);
    return () => query.removeEventListener("change", sync);
  }, []);

  return reduced;
}
