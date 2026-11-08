import type { MenuOptions } from "@tauri-apps/api/menu";
import { t } from "@/lib/i18n/index";
import { isMac } from "@/lib/platform";

/** Keep native editing/window roles while giving every product label a locale. */
export function nativeMenuOptions(
  closeWindow: () => void,
  measure?: { plainAppearance: boolean; onTogglePlain: () => void },
): MenuOptions {
  const close = (id: string, accelerator?: string) => ({
    id, text: t("nativeMenu.closeWindow"), accelerator, action: closeWindow,
  });
  return { items: [
    ...(isMac ? [{ text: "Revia", items: [
      { item: { About: null }, text: t("nativeMenu.about") },
      { item: "Separator" },
      { item: "Services", text: t("nativeMenu.services") },
      { item: "Separator" },
      { item: "Hide", text: t("nativeMenu.hide") },
      { item: "HideOthers", text: t("nativeMenu.hideOthers") },
      { item: "Separator" },
      { item: "Quit", text: t("nativeMenu.quit") },
    ] } as NonNullable<MenuOptions["items"]>[number]] : []),
    { text: t("nativeMenu.file"), items: [
      close("revia-close-window", "CmdOrCtrl+W"),
      // Revia has one application window. Both actions use its normal close
      // request, including the existing in-flight/pending-result protection.
      ...(isMac ? [{ id: "revia-close-all", text: t("nativeMenu.closeAll"), accelerator: "Alt+CmdOrCtrl+W", action: closeWindow }] : []),
      ...(!isMac ? [{ item: "Quit", text: t("nativeMenu.quit") } as const] : []),
    ] },
    { text: t("nativeMenu.edit"), items: [
      { item: "Undo", text: t("nativeMenu.undo") },
      { item: "Redo", text: t("nativeMenu.redo") },
      { item: "Separator" },
      { item: "Cut", text: t("nativeMenu.cut") },
      { item: "Copy", text: t("nativeMenu.copy") },
      { item: "Paste", text: t("nativeMenu.paste") },
      { item: "SelectAll", text: t("nativeMenu.selectAll") },
    ] },
    ...(isMac ? [{ text: t("nativeMenu.view"), items: [
      { item: "Fullscreen", text: t("nativeMenu.fullscreen") },
    ] } as NonNullable<MenuOptions["items"]>[number]] : []),
    { id: "__tauri_window_menu__", text: t("nativeMenu.window"), items: [
      { item: "Minimize", text: t("nativeMenu.minimize") },
      { item: "Maximize", text: t("nativeMenu.maximize") },
      { item: "Separator" },
      close("revia-window-close"),
    ] },
    { id: "__tauri_help_menu__", text: t("nativeMenu.help"), items: [
      ...(isMac ? [] : [{ item: { About: null }, text: t("nativeMenu.about") } as const]),
      // M9 测量版：对照外观开关只在 measure 构建里出现（Q216）。
      ...(measure ? [{ id: "revia-plain-appearance", text: `${t("nativeMenu.plainAppearance")}${measure.plainAppearance ? " ✓" : ""}`, action: measure.onTogglePlain }] : []),
    ] },
  ] };
}

let updates: Promise<void> = Promise.resolve();

/** Serialize native updates; queued work reads the latest locale when it runs. */
export function updateNativeMenu(
  closeWindow: () => void,
  measure?: { plainAppearance: boolean; onTogglePlain: () => void },
): Promise<void> {
  const next = updates.catch(() => undefined).then(async () => {
    const { Menu } = await import("@tauri-apps/api/menu");
    const menu = await Menu.new(nativeMenuOptions(closeWindow, measure));
    try {
      const previous = await menu.setAsAppMenu();
      await previous?.close();
    } finally {
      // The installed app menu keeps its native reference; release our handle.
      await menu.close();
    }
  });
  updates = next;
  return next;
}
