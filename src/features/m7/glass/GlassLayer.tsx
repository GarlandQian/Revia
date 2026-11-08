/**
 * 材质容器（M7 任务 01）：第四版“一整窗连贯玻璃”的装饰层。
 *
 * 用法固定为容器里的第一个子元素，容器自身保持 `isolation: isolate`，
 * 内容层由 index.css 的 `.glass-host > *:not(.glass-layer)` 抬到材质之上。
 * 这一层永远 `aria-hidden` + `pointer-events: none`，不参与焦点、键盘与命中测试。
 */

import { GlassSurface } from "@/components/m7/glass";
import { isMac } from "@/lib/platform";
import { GLASS_SURFACE, glassRadius } from "@/features/m7/glass/material";
import { useFlatGlass } from "@/hooks/useGlassPrefs";

export type GlassLayerVariant = "window" | "panel" | "popover";

export function GlassLayer({
  variant,
  className,
}: {
  /** window：整窗外壳；panel：侧栏 / 工具条 / 状态栏；popover：浮层、对话框、通知。 */
  variant: GlassLayerVariant;
  className?: string;
}) {
  const flat = useFlatGlass();
  // M9 对照外观（S4）：完全不渲染 GlassLayer（hooks 之后再提前返回）。
  if (typeof document !== "undefined" && document.documentElement.dataset.plainAppearance === "true") {
    return null;
  }
  const radius = variant === "panel" ? 0 : glassRadius(variant, isMac ? "macos" : "windows");

  return (
    <GlassSurface
      className={["glass-layer", `glass-layer--${variant}`, flat ? "glass-layer--flat" : "", className ?? ""]
        .filter(Boolean)
        .join(" ")}
      // F01：vendor 默认 200×80 行内尺寸会把材质层钉成小块；显式铺满宿主。
      width="100%"
      height="100%"
      borderRadius={radius}
      borderWidth={GLASS_SURFACE.borderWidth}
      backgroundOpacity={GLASS_SURFACE.backgroundOpacity}
      brightness={GLASS_SURFACE.brightness}
      saturation={GLASS_SURFACE.saturation}
      distortionScale={GLASS_SURFACE.distortionScale}
      redOffset={GLASS_SURFACE.redOffset}
      greenOffset={GLASS_SURFACE.greenOffset}
      blueOffset={GLASS_SURFACE.blueOffset}
      blur={GLASS_SURFACE.blur}
      // M9（Q209）：Windows 不渲染 SVG 折射（WebView2 的 backdrop-filter: url() 开销高），
      // 只保留普通半透明与卡片；macOS 本来就走磨砂（WKWebView 不渲染 SVG 折射）。
      refraction={false}
      renderer="auto"
    />
  );
}
