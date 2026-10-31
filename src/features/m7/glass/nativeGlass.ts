/**
 * 原生 Liquid Glass 的区域同步（macOS 26+）。
 *
 * 外壳上需要玻璃的元素用 `glassPane()` 登记；这里测量它们在窗口里的矩形，合并成一份布局
 * 交给 Rust，由 AppKit 在 webview 下方逐块放 NSGlassEffectView。网页在这些区域保持透明
 * （`html.window-material` / `html.native-glass` 下的样式）；代码与 diff 也放在玻璃上，语法高亮颜色照旧（Q200）。
 * 圆角取元素自身的 CSS border-radius：形状只在 CSS 里定义一次。
 */

import { useEffect, type RefObject } from "react";
import { m7 } from "@/lib/m7/api";
import type { GlassBackground } from "@/types/bindings/GlassBackground";
import type { GlassEdge } from "@/types/bindings/GlassEdge";
import type { GlassLayout } from "@/types/bindings/GlassLayout";
import type { GlassRegion } from "@/types/bindings/GlassRegion";
import type { GlassStyle } from "@/types/bindings/GlassStyle";
import type { GlassSupport } from "@/types/bindings/GlassSupport";

export type GlassPaneOptions = {
  id: string;
  /** 窗口缩放时这块玻璃贴住哪条边；默认贴左、贴顶。 */
  x?: GlassEdge;
  y?: GlassEdge;
  style?: GlassStyle;
};

type Pane = Required<GlassPaneOptions>;
type PaneRef = (element: HTMLElement | null) => (() => void) | undefined;

const panes = new Map<HTMLElement, Pane>();
/** 打开着的模态对话框，最后一个在最上面。panel 是浮在窗口上的一块玻璃，page 铺满整窗（设置页）。 */
const dialogs: { element: HTMLDialogElement; kind: "panel" | "page" }[] = [];
const refs = new Map<string, PaneRef>();
let active = false;
/** macOS 的系统玻璃能按区域放（胶囊、对话框面板）；Windows 的 Mica 只铺整窗。 */
let regionGlass = false;
let timer = 0;
let lastSent = "";
let resize: ResizeObserver | null = null;
const probes = new Map<string, HTMLElement>();

/** 稳定的回调 ref：同一组选项每次渲染拿到同一个函数，不会反复登记。 */
export function glassPane(options: GlassPaneOptions): PaneRef {
  const pane: Pane = { x: "start", y: "start", style: "regular", ...options };
  const key = `${pane.id}|${pane.x}|${pane.y}|${pane.style}`;
  let ref = refs.get(key);
  if (!ref) {
    ref = (element) => {
      if (!element) return undefined;
      panes.set(element, pane);
      observe(element);
      schedule();
      // 弹层由定位库在挂载后异步摆到位置上，位置变了而尺寸没变：稍后再量一次。
      window.setTimeout(() => schedule(), 80);
      return () => {
        panes.delete(element);
        resize?.unobserve(element);
        schedule();
      };
    };
    refs.set(key, ref);
  }
  return ref;
}

/** 窗口底是否为系统材质（macOS 玻璃或 Windows Mica）；是的话网页里不再画模拟材质（着色器高光等）。 */
export function nativeGlassActive(): boolean {
  return active;
}

/**
 * 询问原生层支持哪种窗口材质并开始同步：macOS 26+ 打开 `html.window-material.native-glass`，
 * Windows 11 的 Mica 只打开 `html.window-material`（区域上的材质由网页画）。
 */
export async function startNativeGlass(): Promise<boolean> {
  if (active) return true;
  let support: GlassSupport;
  try {
    support = await m7.glassSupport();
  } catch {
    return false;
  }
  if (support === "unsupported") return false;
  active = true;
  regionGlass = support === "native";
  const root = document.documentElement;
  // window-material：窗口底是系统材质（macOS 与 Windows 共用的外观层）；native-glass：胶囊等区域也是系统玻璃。
  root.classList.add("window-material");
  if (regionGlass) root.classList.add("native-glass");
  resize = new ResizeObserver(() => schedule());
  resize.observe(root);
  for (const element of panes.keys()) observe(element);
  // 主题、界面缩放与导入主题的变量都写在 <html> 上。
  new MutationObserver(() => schedule()).observe(root, {
    attributes: true,
    attributeFilter: ["data-kind", "data-theme", "style"],
  });
  // 媒体查询变化不会修改 html 属性：辅助功能切换后也要立即撤掉原生透明层。
  for (const query of ["(prefers-reduced-transparency: reduce)", "(forced-colors: active)"]) {
    window.matchMedia(query).addEventListener("change", () => schedule());
  }
  void document.fonts?.ready.then(() => schedule());
  schedule();
  return true;
}

/**
 * 打开模态对话框（showModal），卸载或 open 变假时关闭。系统材质下：panel 在 macOS 上是对话框位置的一块原生玻璃
 * （Windows 由样式画面板），后面的主界面由样式淡出；page 铺满整窗，主界面由样式藏起。工具栏胶囊的玻璃暂时撤掉。
 */
export function useModalDialog(ref: RefObject<HTMLDialogElement | null>, open = true, kind: "panel" | "page" = "panel"): void {
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || !open) return;
    dialog.showModal();
    const entry = { element: dialog, kind };
    dialogs.push(entry);
    observe(dialog);
    schedule();
    return () => {
      dialog.close();
      dialogs.splice(dialogs.indexOf(entry), 1);
      resize?.unobserve(dialog);
      schedule();
    };
  }, [ref, open, kind]);
}

function observe(element: HTMLElement) {
  if (!resize) return;
  resize.observe(element);
  // 兄弟元素变宽会把胶囊挤开而胶囊本身尺寸不变：一并观察父元素。
  if (element.parentElement) resize.observe(element.parentElement);
}

function schedule(delay = 0) {
  if (!active || timer) return;
  // 用定时器而不是 rAF：窗口被遮挡时 WebKit 暂停 rAF，玻璃会停在启动时的空布局。
  timer = window.setTimeout(flush, delay);
}

function flush() {
  timer = 0;
  const layout = measure();
  if (!layout) {
    // 样式表还没生效：稍后再测，不送半成品布局。
    schedule(50);
    return;
  }
  const key = JSON.stringify(layout);
  if (key === lastSent) return;
  lastSent = key;
  void m7.glassLayout(layout).catch(() => {
    lastSent = "";
  });
}

function measure(): GlassLayout | null {
  // `--glass-window` 只在 html.window-material 的样式里定义：读不到说明样式表还没生效。
  const root = getComputedStyle(document.documentElement);
  const mode = root.getPropertyValue("--glass-window").trim();
  if (!mode) return null;
  // solid：窗口画成编辑器底色；glass：窗口透明，整窗一块玻璃折射桌面。
  let background: GlassBackground;
  if (mode === "solid") {
    const color = cssColor("--glass-window-background");
    if (!color) return null;
    background = { solid: [color[0], color[1], color[2]] };
  } else {
    background = { glass: { radius: Number.parseFloat(root.getPropertyValue("--window-radius")) || 0 } };
  }
  // Mica 只铺整窗，没有按区域的玻璃。
  if (!regionGlass || mode === "solid") return { background, regions: [] };
  const top = dialogs.at(-1);
  if (top) {
    // 模态对话框盖住整个外壳：只留对话框自己那块玻璃（整窗的设置页连这块也不要）。
    const panel = top.kind === "panel" ? region(top.element, { id: "dialog", x: "start", y: "start", style: "regular" }) : null;
    return { background, regions: panel ? [panel] : [] };
  }
  const regions: GlassRegion[] = [];
  for (const [element, pane] of panes) {
    const glass = region(element, pane);
    if (glass) regions.push(glass);
  }
  return { background, regions };
}

function region(element: HTMLElement, pane: Pane): GlassRegion | null {
  if (!element.isConnected) return null;
  const rect = element.getBoundingClientRect();
  if (rect.width < 1 || rect.height < 1) return null;
  const style = getComputedStyle(element);
  if (style.visibility === "hidden") return null;
  // 界面缩放用 CSS zoom：getBoundingClientRect 已是缩放后的尺寸，圆角要同样换算。
  const scale = element.offsetWidth ? rect.width / element.offsetWidth : 1;
  const radius = (Number.parseFloat(style.borderTopLeftRadius) || 0) * scale;
  return {
    id: pane.id,
    x: round(rect.left),
    y: round(rect.top),
    width: round(rect.width),
    height: round(rect.height),
    radius: round(Math.min(radius, rect.height / 2, rect.width / 2)),
    horizontal: pane.x,
    vertical: pane.y,
    style: pane.style,
  };
}

function round(value: number) {
  return Math.round(value * 2) / 2;
}

/**
 * 读取一个颜色变量的计算值（sRGB 0–255 + 不透明度 0–255）。主题变量可能是任意 CSS 颜色写法，
 * 交给浏览器解析；解析不出颜色时返回 null。
 */
function cssColor(variable: string): [number, number, number, number] | null {
  let probe = probes.get(variable);
  if (!probe) {
    probe = document.createElement("div");
    probe.setAttribute("aria-hidden", "true");
    probe.style.cssText = `position:fixed;width:0;height:0;pointer-events:none;background:var(${variable})`;
    document.body.append(probe);
    probes.set(variable, probe);
  }
  const value = getComputedStyle(probe).backgroundColor;
  const channels = value.match(/[\d.]+/g)?.map(Number) ?? [];
  if (channels.length < 3) return null;
  // color-mix() 的计算值是 `color(srgb r g b / a)`，通道为 0–1；rgb() / rgba() 为 0–255。
  const unit = value.startsWith("color(") ? 255 : 1;
  const [red, green, blue] = channels.slice(0, 3).map((channel) => Math.round(Math.min(255, Math.max(0, channel * unit))));
  const alpha = Math.round(Math.min(1, Math.max(0, channels[3] ?? 1)) * 255);
  return [red, green, blue, alpha];
}
