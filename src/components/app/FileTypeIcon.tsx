/**
 * 文件类型图标：照 JetBrains 新版界面的彩色圆角方块加字母（Q107）。
 * 只是辨认文件类型的视觉提示；Git 状态仍只由行右侧的状态字母表示（Q31）。
 */

import { Link2 } from "lucide-react";

const TYPES: Record<string, [color: string, label: string]> = {
  ts: ["#3178c6", "TS"],
  tsx: ["#3178c6", "TX"],
  js: ["#d6a514", "JS"],
  jsx: ["#d6a514", "JX"],
  mjs: ["#d6a514", "JS"],
  rs: ["#c46a2f", "RS"],
  json: ["#7a8291", "{}"],
  yaml: ["#c9634f", "Y"],
  yml: ["#c9634f", "Y"],
  toml: ["#9a6b3f", "T"],
  md: ["#5b8bd0", "M"],
  css: ["#8250df", "#"],
  html: ["#d4643a", "<>"],
  sql: ["#d98e2b", "SQ"],
  sh: ["#4f9a5b", "$"],
  py: ["#3d7ab8", "PY"],
  png: ["#9c6ade", "P"],
  jpg: ["#9c6ade", "P"],
  jpeg: ["#9c6ade", "P"],
  gif: ["#9c6ade", "P"],
  svg: ["#9c6ade", "P"],
};
const FALLBACK: [string, string] = ["#8c90a0", ""];

export function FileTypeIcon({ path, symlink }: { path: string; symlink?: boolean }) {
  if (symlink) return <Link2 aria-hidden className="size-4 shrink-0 text-icon" strokeWidth={1.6} />;
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  const [color, label] = dot > 0 ? (TYPES[name.slice(dot + 1).toLowerCase()] ?? FALLBACK) : FALLBACK;
  return (
    <svg aria-hidden viewBox="0 0 16 16" className="size-4 shrink-0">
      <rect x="1.5" y="1.5" width="13" height="13" rx="3" fill={color} />
      {label ? (
        <text x="8" y="10.6" textAnchor="middle" fontSize={label.length > 1 ? 6.4 : 7.6} fontWeight={700}
          fontFamily="Inter, sans-serif" fill="#ffffff">
          {label}
        </text>
      ) : null}
    </svg>
  );
}
