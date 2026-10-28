/**
 * 界面图标：VS Code 的 Codicons（M4 F01.3、Q130）。
 *
 * 图标字体与样式来自 npm 包 `@vscode/codicons` 0.0.46-24，
 * 在 `src/main.tsx` 里导入 `codicon.css`。图标本身以 CC BY 4.0 发布，
 * 许可见 `src/themes/licenses/codicons-CC-BY-4.0.txt`。
 */

import type { CSSProperties } from "react";

export function Codicon({
  name,
  size,
  className,
  title,
  spin,
  style,
}: {
  /** Codicons 名称，不含 `codicon-` 前缀（例如 `source-control`）。 */
  name: string;
  /** 图标字号（像素）；默认 16。 */
  size?: number;
  className?: string;
  /** 无障碍名称；不传时图标对读屏隐藏。 */
  title?: string;
  spin?: boolean;
  style?: CSSProperties;
}) {
  const classes = ["codicon", `codicon-${name}`];
  if (spin) classes.push("codicon-modifier-spin");
  if (className) classes.push(className);
  return (
    <span
      className={classes.join(" ")}
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      title={title}
      style={{ ...(size ? { fontSize: `${size}px` } : null), ...style }}
    />
  );
}
