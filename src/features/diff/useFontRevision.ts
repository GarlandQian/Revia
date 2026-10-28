import { useEffect, useState } from "react";

/** 字体文件加载完成可能改变折行数，即使容器宽度没有变化。 */
export function useFontRevision(wrap: boolean): number {
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (!wrap) return;
    const loaded = () => setRevision((value) => value + 1);
    document.fonts.addEventListener("loadingdone", loaded);
    return () => document.fonts.removeEventListener("loadingdone", loaded);
  }, [wrap]);
  return revision;
}
