import { useEffect, type RefObject } from "react";

/** 统一管理 `<dialog>` 的 `showModal()` / `close()` 生命周期（M13 移除旧玻璃布局同步）。 */
export function useModalDialog(ref: RefObject<HTMLDialogElement | null>, open = true): void {
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open) {
      if (!dialog.open) dialog.showModal();
      return () => {
        if (dialog.open) dialog.close();
      };
    }
    if (dialog.open) dialog.close();
  }, [ref, open]);
}
