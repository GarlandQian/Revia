import { useEffect, useRef } from "react";

/** 组件是否仍挂载：异步写入完成时，已卸载的组件不再关闭页签或更新状态。 */
export function useMountedRef() {
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  return mounted;
}
