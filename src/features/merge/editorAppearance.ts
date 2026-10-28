import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { codeFontStack } from "@/lib/theme";

export interface MergeAppearanceOptions {
  fontSize: number;
  fontFamily: string | null;
  softWrap: boolean;
}

/** 仅配置显示；放进 Compartment 动态替换，不重建文档、选区或撤销历史。 */
export function mergeEditorAppearance(options: MergeAppearanceOptions, rowHeight: number): Extension {
  const fontFamily = codeFontStack(options.fontFamily);
  return [
    EditorView.theme({
      "&": { height: "100%", fontSize: `${options.fontSize}px`, backgroundColor: "var(--diff-bg)", color: "var(--foreground)" },
      ".cm-scroller": { fontFamily, lineHeight: `${rowHeight}px`, overflow: "auto" },
      ".cm-content": { padding: "0", caretColor: "var(--foreground)" },
      ".cm-line": { padding: "0 8px" },
      ".cm-gutters": { backgroundColor: "var(--diff-bg)", color: "var(--editorlinenumber-foreground)", border: "none" },
      ".cm-lineNumbers .cm-gutterElement": { padding: "0 6px 0 10px", minWidth: "32px" },
      "&.cm-focused": { outline: "none" },
      ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "var(--merge-selection)" },
    }),
    options.softWrap ? EditorView.lineWrapping : [],
  ];
}
