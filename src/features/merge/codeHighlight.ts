/**
 * 合并页签三栏的语法高亮：按内容请求 Rust 侧的 syntect 结果（ADR-0006），
 * 与 diff 页签用同一套类别与配色。只请求可视区附近的行；结果栏编辑后
 * 换一个内容版本重新请求（停顿片刻再发，连续输入不会每次都解析）。
 */

import { RangeSetBuilder, StateEffect } from "@codemirror/state";
import { Decoration, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from "@codemirror/view";
import { ipc, type HighlightLine } from "@/lib/ipc";

const MARGIN_LINES = 60;
const EDIT_DELAY_MS = 250;

let categoryNames: Promise<string[]> | null = null;

function categories(): Promise<string[]> {
  categoryNames ??= ipc.syntaxCategories().catch(() => []);
  return categoryNames;
}

const refresh = StateEffect.define<null>();

export interface HighlightSource {
  worktreeId: string;
  /** 稳定的文档标识（worktree + 文件 + 哪一栏）。 */
  documentId: string;
  language: string;
}

export function rustHighlight(source: HighlightSource) {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      private generation = 1;
      private sentGeneration = 0;
      private lines = new Map<number, HighlightLine>();
      private names: string[] = [];
      private timer = 0;
      private busy = false;
      private again = false;
      private destroyed = false;
      private plain = false;

      private readonly view: EditorView;

      constructor(view: EditorView) {
        this.view = view;
        void categories().then((names) => {
          this.names = names;
          this.schedule(0);
        });
      }

      update(update: ViewUpdate) {
        if (update.docChanged) {
          this.generation += 1;
          this.lines = new Map();
          this.decorations = this.decorations.map(update.changes);
          this.schedule(EDIT_DELAY_MS);
        } else if (update.viewportChanged) {
          // 编辑后新结果还没回来时保留映射过的旧颜色，不闪成纯文本。
          if (this.sentGeneration === this.generation) this.decorations = this.build();
          this.schedule(30);
        } else if (update.transactions.some((tr) => tr.effects.some((effect) => effect.is(refresh)))) {
          this.decorations = this.build();
        }
      }

      destroy() {
        this.destroyed = true;
        window.clearTimeout(this.timer);
      }

      private schedule(delay: number) {
        if (this.plain || this.destroyed) return;
        window.clearTimeout(this.timer);
        this.timer = window.setTimeout(() => void this.fetch(), delay);
      }

      private visibleLines(): [number, number] {
        const doc = this.view.state.doc;
        const { from, to } = this.view.viewport;
        const first = Math.max(0, doc.lineAt(from).number - 1 - MARGIN_LINES);
        const last = Math.min(doc.lines, doc.lineAt(to).number + MARGIN_LINES);
        return [first, last];
      }

      private async fetch() {
        if (this.destroyed) return;
        if (this.busy) {
          this.again = true;
          return;
        }
        const [first, last] = this.visibleLines();
        let missing = false;
        for (let line = first; line < last; line += 1) {
          if (!this.lines.has(line)) {
            missing = true;
            break;
          }
        }
        if (!missing) return;
        const generation = this.generation;
        const content = () => this.view.state.doc.toString();
        this.busy = true;
        try {
          const request = {
            worktreeId: source.worktreeId,
            documentId: source.documentId,
            generation,
            language: source.language,
            fromLine: first,
            toLine: last,
          };
          let result = await ipc.highlightText({
            ...request,
            content: this.sentGeneration === generation ? undefined : content(),
          });
          if (result.needContent) result = await ipc.highlightText({ ...request, content: content() });
          if (this.destroyed || generation !== this.generation) return;
          this.sentGeneration = generation;
          if (result.kind === "plainText") {
            this.plain = true;
            return;
          }
          result.lines.forEach((line, index) => this.lines.set(result.fromLine + index, line));
          this.view.dispatch({ effects: refresh.of(null) });
        } catch {
          // 高亮失败不影响合并：保持纯文本。
          this.plain = true;
        } finally {
          this.busy = false;
          if (this.again) {
            this.again = false;
            this.schedule(0);
          }
        }
      }

      private build(): DecorationSet {
        const builder = new RangeSetBuilder<Decoration>();
        const doc = this.view.state.doc;
        const { from, to } = this.view.viewport;
        const firstLine = doc.lineAt(from).number;
        const lastLine = doc.lineAt(to).number;
        for (let number = firstLine; number <= lastLine; number += 1) {
          const tokens = this.lines.get(number - 1);
          if (!tokens) continue;
          const line = doc.line(number);
          const total = tokens.reduce((sum, token) => sum + token.text.length, 0);
          // 内容版本对不上（行文本不同）时不着色，等新的结果。
          if (total !== line.length) continue;
          let offset = line.from;
          for (const token of tokens) {
            const end = offset + token.text.length;
            const name = this.names[token.category];
            if (name && name !== "plain" && end > offset) {
              builder.add(offset, end, Decoration.mark({ class: `tok-${name}` }));
            }
            offset = end;
          }
        }
        return builder.finish();
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}
