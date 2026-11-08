# 10：第一轮实测（测量版）

Status: ready-for-human
Blocked by: 01, 02, 03, 04, 05, 06

## What to build

性能任务完成后，规划方推送 `m9` 出测量版。用户在 Windows 上按 [windows-measure](../windows-measure.md) 测量并发回诊断信息；规划方在 macOS 上测打开与切换仓库。根据结果决定窗口底是否改为实色，以及任务 04 末项的渲染优化做不做。

## Acceptance criteria

- [ ] 规划方复核任务 01–06 后，只推送 `m9`（Q216），确认 Actions 产物可以下载。
- [ ] 用户按测量步骤，分别在对照开关关闭和打开时导出诊断信息，并说明主观感受（拖动、缩放、滚动是否跟手）。
- [ ] 规划方在 macOS 可见窗口中，测 `diffs` 和大仓库 fixture 的第一次打开与切换。
- [ ] 规划方把两个平台的数据与 Q211 门槛逐项对照，写进 `measure-round1.md`，并写明是否改为实色窗口底、哪些渲染优化要做；需要实施的部分写进对应任务的 Comments。
- [ ] 规划方删除测量用的对照开关之前，确认原因已经查明（Q210）。

## Comments
