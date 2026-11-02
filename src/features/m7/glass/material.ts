/**
 * 第四版玻璃材质的唯一参数来源（M7 任务 01、Q189–Q199）。
 *
 * 数值取自 docs/milestones/m7/mockups/react-bits/mount.tsx 与 adapt.css：
 * 组件本身原样复用 vendor 源码，Rev ia 只在这里集中放主题相关的参数，
 * 避免每个接入点各写一套颜色，导致材质不连贯。
 *
 * 三种材质：
 * - surface：整窗 / 侧栏 / 工具条 / 状态栏 / 浮层 / 对话框的连续材质层；
 * - button：工具按钮与操作按钮的低饱和透明填充；
 * - rail：工具图标（GlassIcons）的玻璃底盘。
 */

export type GlassKind = "light" | "dark";

/** 材质层的形状与光学参数（第四版 mount.tsx 的 GlassSurface 参数）。 */
export const GLASS_SURFACE = {
  borderWidth: 0.04,
  backgroundOpacity: 0.1,
  brightness: 55,
  saturation: 1.05,
  distortionScale: -80,
  redOffset: 0,
  greenOffset: 5,
  blueOffset: 10,
  blur: 12,
} as const;

/** 圆角：窗口外壳 macOS 12 / 其他平台 8，浮层 7（第四版）。 */
export function glassRadius(part: "window" | "popover", platform: "macos" | "windows"): number {
  if (part === "popover") return 7;
  return platform === "macos" ? 12 : 8;
}

/** 按钮材质：低饱和透明填充 + 对应墨色（第四版 mount.tsx）。 */
export function glassButtonTheme(kind: GlassKind) {
  return kind === "dark"
    ? {
        tint: "#c3d2dd",
        text: "#e5ebf0",
        line: "#c2d5e3",
        base: "#556573",
        primaryOpacity: 0.16,
        quietOpacity: 0.045,
      }
    : {
        tint: "#405b71",
        text: "#294155",
        line: "#ffffff",
        base: "#adbcc7",
        primaryOpacity: 0.12,
        quietOpacity: 0.045,
      };
}

/** 工具图标材质（第四版 GlassIcons 参数）。 */
export function glassRailTheme(kind: GlassKind) {
  return kind === "dark"
    ? { tint: "#b7c9d6", tintOpacity: 0.08, iconColor: "#d4e0e8", theme: "dark" as const }
    : { tint: "#ffffff", tintOpacity: 0.15, iconColor: "#3c5265", theme: "light" as const };
}

/**
 * 一个按钮需要的高光强度。
 * `flat` 为真时（减少透明度 / 高对比度 / 不支持背景滤镜）整体退化为实色。
 */
export function glassButtonOptics(flat: boolean) {
  return {
    intensity: flat ? 0 : 0.55,
    idleIntensity: flat ? 0 : 0.18,
    glow: 0.25,
    thickness: 1,
    blur: flat ? 0 : 10,
    followMouse: !flat,
    proximity: 80,
    autoAnimate: false,
  } as const;
}
