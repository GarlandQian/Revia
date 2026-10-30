import type { HistoryFile } from "@/types/bindings/HistoryFile";
import type { HistoryMergeFileGroup } from "@/types/bindings/HistoryMergeFileGroup";
import type { HistoryMergeMode } from "@/types/bindings/HistoryMergeMode";
import { messages } from "@/lib/i18n/index";
import { shortOid } from "./logModel";

export function effectiveMergeMode(parentCount: number, byParent: boolean): HistoryMergeMode {
  return byParent || parentCount > 2 ? "byParent" : "combined";
}

interface FileNode {
  kind: "file";
  key: string;
  file: HistoryFile;
  /** The comparison parent, not the optional parent used to label a group. */
  parent: string | null;
}

interface DirNode {
  kind: "dir" | "parent";
  key: string;
  name: string;
  children: FileTreeNode[];
}

export type FileTreeNode = FileNode | DirNode;

function fileNode(file: HistoryFile, parent: string | null): FileNode {
  return { kind: "file", key: `${parent ?? ""}:${file.path}`, file, parent };
}

export function firstHistoryFile(groups: HistoryMergeFileGroup[]): FileNode | null {
  for (const group of groups) {
    const file = group.files.find((item) => item.diffId);
    if (file) return fileNode(file, group.diffParent);
  }
  return null;
}

export function buildFileTree(groups: HistoryMergeFileGroup[], groupByDirectory: boolean): FileTreeNode[] {
  const nodes: FileTreeNode[] = [];
  for (const [index, group] of groups.entries()) {
    const files: FileTreeNode[] = [];
    for (const file of group.files) {
      let siblings = files;
      if (groupByDirectory) {
        let directory = "";
        for (const name of file.path.split("/").slice(0, -1)) {
          directory += `${name}/`;
          const key = `dir:${group.diffParent ?? ""}:${directory}`;
          let node = siblings.find((item): item is DirNode => item.kind === "dir" && item.key === key);
          if (!node) {
            node = { kind: "dir", key, name, children: [] };
            siblings.push(node);
          }
          siblings = node.children;
        }
      }
      siblings.push(fileNode(file, group.diffParent));
    }
    if (group.parent) {
      nodes.push({ kind: "parent", key: `parent:${group.parent}`,
        name: messages.log.parentGroup(index + 1, shortOid(group.parent)), children: files });
    } else nodes.push(...files);
  }
  return nodes;
}
