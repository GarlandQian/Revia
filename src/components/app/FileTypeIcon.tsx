/**
 * 文件类型图标：照 JetBrains 新版界面的彩色圆角方块加字母（Q107）。
 * 只是辨认文件类型的视觉提示；Git 状态仍只由行右侧的状态字母表示（Q31）。
 */

import { Link2 } from "lucide-react";

const TYPES: Record<string, [color: string, label: string]> = {
  ts: ["var(--file-typescript)", "TS"],
  tsx: ["var(--file-typescript)", "TX"],
  js: ["var(--file-javascript)", "JS"],
  jsx: ["var(--file-javascript)", "JX"],
  mjs: ["var(--file-javascript)", "JS"],
  rs: ["var(--file-rust)", "RS"],
  json: ["var(--file-json)", "{}"],
  yaml: ["var(--file-yaml)", "Y"],
  yml: ["var(--file-yaml)", "Y"],
  toml: ["var(--file-toml)", "T"],
  md: ["var(--file-markdown)", "M"],
  css: ["var(--file-css)", "#"],
  html: ["var(--file-html)", "<>"],
  sql: ["var(--file-sql)", "SQ"],
  sh: ["var(--file-shell)", "$"],
  py: ["var(--file-python)", "PY"],
  png: ["var(--file-image)", "P"],
  jpg: ["var(--file-image)", "P"],
  jpeg: ["var(--file-image)", "P"],
  gif: ["var(--file-image)", "P"],
  svg: ["var(--file-image)", "P"],
};
const FALLBACK: [string, string] = ["var(--file-default)", ""];

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
          fontFamily="Inter, sans-serif" fill="var(--file-icon-label)">
          {label}
        </text>
      ) : null}
    </svg>
  );
}
