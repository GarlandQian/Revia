/** 提交输出的归属、实时缓冲和终态；React/Tauri 只负责传入事件与展示快照。 */

import type { CommitOutputEvent } from "@/types/bindings/CommitOutputEvent";

const MAX_OUTPUT_BYTES = 256 * 1024;

export interface CommitOutputRun {
  readonly worktreeId: string;
  readonly clientNonce: string;
}

interface OutputState {
  run: CommitOutputRun;
  chunks: Array<{ text: string; bytes: number }>;
  bytes: number;
  finalOutput: string[] | null;
}

export class CommitOutputStore {
  private streams = new Map<string, OutputState>();
  private encoder = new TextEncoder();

  start(worktreeId: string, clientNonce: string): CommitOutputRun {
    const run = { worktreeId, clientNonce };
    this.streams.set(worktreeId, { run, chunks: [], bytes: 0, finalOutput: null });
    return run;
  }

  accept(event: CommitOutputEvent | null | undefined): { worktreeId: string; output: string[] } | null {
    if (!event || typeof event.line !== "string" || event.line.length === 0) return null;
    const stream = this.streams.get(event.worktreeId);
    if (!stream || stream.run.clientNonce !== event.clientNonce || stream.finalOutput !== null) return null;

    // 保留完整 UTF-8 片段，字节数只在入队时计算，不反复编码整个输出尾部。
    const bytes = this.encoder.encode(event.line).length;
    stream.chunks.push({ text: event.line, bytes });
    stream.bytes += bytes;
    let first = 0;
    while (stream.bytes > MAX_OUTPUT_BYTES && first < stream.chunks.length) {
      stream.bytes -= stream.chunks[first++].bytes;
    }
    if (first > 0) stream.chunks = stream.chunks.slice(first);

    return { worktreeId: stream.run.worktreeId, output: this.lines(stream) };
  }

  /** 收到后端完整终态输出才封口，迟到事件不能覆盖该次最终文本。 */
  finish(run: CommitOutputRun, output: string): string[] {
    const stream = this.streams.get(run.worktreeId);
    if (!stream || stream.run !== run) return [];
    if (stream.finalOutput === null) {
      stream.finalOutput = output.length > 0 ? output.split("\n") : [];
      stream.chunks = [];
      stream.bytes = 0;
    }
    return stream.finalOutput.slice();
  }

  /** 错误回包没有完整输出时只取快照，仍接收排队中的同次事件直到 release。 */
  snapshot(run: CommitOutputRun): string[] {
    const stream = this.streams.get(run.worktreeId);
    if (!stream || stream.run !== run) return [];
    return stream.finalOutput === null ? this.lines(stream) : stream.finalOutput.slice();
  }

  release(run: CommitOutputRun): void {
    // 旧请求的 finally 不能清掉同一 worktree 后来开始的输出。
    if (this.streams.get(run.worktreeId)?.run === run) this.streams.delete(run.worktreeId);
  }

  private lines(stream: OutputState): string[] {
    return stream.chunks.map((chunk) => chunk.text).join("").split("\n");
  }
}
