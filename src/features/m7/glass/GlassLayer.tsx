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
  const radius = variant === "panel" ? 0 : glassRadius(variant, isMac ? "macos" : "windows");

  return (
    <GlassSurface
      className={["glass-layer", `glass-layer--${variant}`, flat ? "glass-layer--flat" : "", className ?? ""]
        .filter(Boolean)
        .join(" ")}
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
      // 支持时走折射路径（Chromium/WebView2 的 SVG 滤镜），WKWebView 自动降级为磨砂。
      refraction={!flat}
      renderer="auto"
    />
  );
}
