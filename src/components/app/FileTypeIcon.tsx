/**
 * 文件类型图标：VS Code 的 Material Icon Theme（M4 F01.3、Q130）。
 *
 * 图标与映射来自 VS Code 扩展 pkief.material-icon-theme 5.39.0（MIT），
 * 由 `tools/sync-material-icons.mjs` 复制到 `src/assets/material-icons/`；
 * 许可见同目录的 `LICENSE.txt`。查找顺序与 VS Code 一致：
 * 完整文件名 → 从长到短的扩展名 → 默认文件图标。
 */

import manifest from "@/assets/material-icons/manifest.json";
import { Codicon } from "@/components/ui/codicon";

const iconModules = import.meta.glob<string>("../../assets/material-icons/icons/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
});

const iconUrls = new Map<string, string>();
for (const [path, url] of Object.entries(iconModules)) {
  const name = path.slice(path.lastIndexOf("/") + 1, -".svg".length);
  iconUrls.set(name, url);
}

const fileExtensions = manifest.fileExtensions as Record<string, string>;
const fileNames = manifest.fileNames as Record<string, string>;
const defaultIcon = manifest.default as string;

/** 解析一个路径应使用的 Material Icon 名称。 */
export function materialIconName(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const exact = fileNames[name] ?? fileNames[name.toLowerCase()];
  if (exact) return exact;
  const parts = name.toLowerCase().split(".");
  for (let index = 1; index < parts.length; index += 1) {
    const extension = parts.slice(index).join(".");
    const icon = fileExtensions[extension];
    if (icon) return icon;
  }
  return defaultIcon;
}

export function FileTypeIcon({ path, symlink, className }: { path: string; symlink?: boolean; className?: string }) {
  if (symlink) {
    return <Codicon name="link" size={16} className={`shrink-0 ${className ?? ""}`} />;
  }
  const name = materialIconName(path);
  const url = iconUrls.get(name) ?? iconUrls.get(defaultIcon);
  if (!url) return null;
  return (
    <img
      src={url}
      alt=""
      aria-hidden
      draggable={false}
      className={`size-4 shrink-0 select-none ${className ?? ""}`}
    />
  );
}
