/**
 * 按钮材质适配（M7 任务 01）：第四版的低饱和透明填充，一套状态覆盖所有操作按钮。
 *
 * 两点约束：
 * - 原生按钮契约（type / disabled / aria-* / ref / onClick / form 提交）必须原样保留，
 *   材质层不改命中测试、不用 pointer-events 代替禁用；
 * - SpecularButton 每个实例创建一个 WebGL2 上下文，浏览器对同时存在的上下文有上限，
 *   所以只有显式 `specular` 的主要动作才渲染它，其余按钮用同一套 CSS 材质与状态。
 *   同一页面同时存在的着色器按钮不超过 {@link SPECULAR_BUDGET} 个。
 */

import {
  forwardRef,
  useEffect,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
  type Ref,
} from "react";
import { SpecularButton } from "@/components/m7/glass";
import { glassButtonOptics, glassButtonTheme } from "@/features/m7/glass/material";
import { nativeGlassActive } from "@/features/m7/glass/nativeGlass";
import { useFlatGlass, useReducedMotion, useThemeKind } from "@/hooks/useGlassPrefs";

/** 同时存在的着色器按钮上限（每个实例一个 WebGL2 上下文）。 */
export const SPECULAR_BUDGET = 6;

let specularSlots = 0;

/** 申请一个着色器名额：只有前 N 个挂载的按钮拿得到，卸载后归还。 */
function useSpecularSlot(): boolean {
  const [granted, setGranted] = useState(false);

  useEffect(() => {
    if (specularSlots >= SPECULAR_BUDGET) return undefined;
    specularSlots += 1;
    setGranted(true);
    return () => {
      specularSlots -= 1;
      setGranted(false);
    };
  }, []);

  return granted;
}

export type GlassButtonVariant = "primary" | "secondary" | "quiet" | "icon";

export interface GlassButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  children?: ReactNode;
  variant?: GlassButtonVariant;
  /** 主要动作使用着色器高光；默认只走 CSS 材质。 */
  specular?: boolean;
  /** 按钮圆角，默认 5（第四版）。 */
  radius?: number;
}

const VARIANT_OPACITY: Record<GlassButtonVariant, "primary" | "quiet"> = {
  primary: "primary",
  secondary: "quiet",
  quiet: "quiet",
  icon: "quiet",
};

export const GlassButton = forwardRef(function GlassButton(
  {
    children,
    variant = "secondary",
    specular = false,
    radius = 5,
    className,
    disabled,
    type = "button",
    ...rest
  }: GlassButtonProps,
  ref: Ref<HTMLButtonElement>,
) {
  const kind = useThemeKind();
  const flat = useFlatGlass();
  const reducedMotion = useReducedMotion();
  const slot = useSpecularSlot();
  const palette = glassButtonTheme(kind);
  const opacity = palette[VARIANT_OPACITY[variant] === "primary" ? "primaryOpacity" : "quietOpacity"];
  const classes = ["glass-button", `glass-button--${variant}`, flat ? "glass-button--flat" : "", className ?? ""]
    .filter(Boolean)
    .join(" ");

  // 原生玻璃下按钮跟随系统样式；着色器画布在透明 webview 里会留下一块偏色矩形。
  if (specular && slot && !nativeGlassActive()) {
    const optics = glassButtonOptics(flat);
    return (
      <SpecularButton
        {...rest}
        type={type}
        disabled={disabled}
        className={classes}
        size="sm"
        radius={radius}
        tint={palette.tint}
        tintOpacity={opacity}
        textColor={palette.text}
        lineColor={palette.line}
        baseColor={palette.base}
        intensity={optics.intensity}
        idleIntensity={optics.idleIntensity}
        glow={optics.glow}
        thickness={optics.thickness}
        blur={optics.blur}
        followMouse={optics.followMouse && !reducedMotion}
        proximity={optics.proximity}
        autoAnimate={optics.autoAnimate}
      >
        {children}
      </SpecularButton>
    );
  }

  return (
    <button
      {...rest}
      ref={ref}
      type={type}
      disabled={disabled}
      className={classes}
      style={{
        // 低饱和填充：颜色与不透明度都来自第四版参数，深浅色各一套。
        ["--gb-tint" as string]: palette.tint,
        ["--gb-opacity" as string]: String(opacity),
        ["--gb-text" as string]: palette.text,
        ["--gb-line" as string]: palette.line,
        ...rest.style,
      }}
    >
      {children}
    </button>
  );
});
