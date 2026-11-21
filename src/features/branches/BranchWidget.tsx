import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError } from "@/lib/error";
/**
 * 顶栏分支组件与“更新 / 推送”按钮（M5 B02、B04、B05）。
 *
 * 点开弹层：搜索框、“新建分支…”、“fetch”；“本地”列出本地分支（当前分支置顶并标记，
 * 有上游时显示领先 / 落后数）；“远端”按远端分组。点分支弹出子菜单：
 * “检出”“从此分支新建分支…”。外观沿用 M4 的工具栏组件，不另起视觉风格。
 */

import { useState } from "react";
import type { BranchOverview } from "@/types/bindings/BranchOverview";
import type { LocalBranch } from "@/types/bindings/LocalBranch";
import type { RemoteBranch } from "@/types/bindings/RemoteBranch";
import type { CheckoutTarget } from "@/types/bindings/CheckoutTarget";
import type { BranchStart } from "@/types/bindings/BranchStart";
import { messages } from "@/lib/i18n/index";
import { MOD_SYMBOLS } from "@/lib/platform";
import { AppIcon } from "@/components/ui/app-icon";
import { Popover, MenuItem, MenuSeparator } from "@/components/ui/popover";

export interface BranchWidgetProps {
  overview: BranchOverview | null;
  loading: boolean;
  error: AppErrorPayload | null;
  incoming: boolean;
  busy: boolean;
  onRefresh: () => void;
  onCheckout: (target: CheckoutTarget) => void;
  onNewBranch: (start: BranchStart) => void;
  onFetch: () => void;
}

export function BranchWidget(props: BranchWidgetProps) {
  const { overview, loading, error, incoming, busy } = props;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [submenu, setSubmenu] = useState<string | null>(null);

  const current = overview?.currentBranch ?? null;
  const label = current ?? (overview?.detached ? overview?.shortHead ?? messages.changes.detachedHead : null);

  const filterText = query.trim().toLowerCase();
  const matches = (name: string) => filterText.length === 0 || name.toLowerCase().includes(filterText);
  const local = (overview?.local ?? []).filter((branch) => matches(branch.name));
  const remotes = (overview?.remotes ?? [])
    .map((group) => ({ ...group, branches: group.branches.filter((branch) => matches(branch.name)) }))
    .filter((group) => group.branches.length > 0);

  const runCheckout = (target: CheckoutTarget) => {
    setOpen(false);
    setSubmenu(null);
    props.onCheckout(target);
  };

  return (
    <div className="relative min-w-0">
      <Popover
        open={open}
        onOpen={() => {
          setOpen(true);
          props.onRefresh();
        }}
        onClose={() => {
          setOpen(false);
          setSubmenu(null);
        }}
        width={420}
        trigger={
          <button
            type="button"
            className="ctx-btn min-w-0"
            disabled={busy}
            title={error ? formatAppError(error) : messages.branches.open}
          >
            <span className="branch-dot" aria-hidden />
            <span className="max-w-[200px] truncate">{label ?? messages.changes.detachedHead}</span>
            {incoming ? (
              <span
                className="size-1.5 shrink-0 rounded-full bg-[var(--git-m)]"
                title={messages.branches.incoming}
              />
            ) : null}
            <AppIcon name="chevron-down" size={12} className="text-[var(--text-muted)]" />
          </button>
        }
      >
        <div className="px-1 pb-1">
          <div className="relative">
            <AppIcon name="search" size={12} className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-[var(--text-secondary)]" />
            <input
              className="settings-input w-full pl-6"
              placeholder={messages.branches.search}
              aria-label={messages.branches.search}
              value={query}
              spellCheck={false}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
        </div>
        <MenuItem
          onClick={() => {
            setOpen(false);
            props.onNewBranch({ kind: "head" });
          }}
        >
          <span className="flex items-center gap-1.5">
            <AppIcon name="add" size={13} />
            {messages.branches.newBranch}
          </span>
        </MenuItem>
        <MenuItem
          disabled={busy}
          onClick={() => {
            setOpen(false);
            props.onFetch();
          }}
        >
          <span className="flex items-center gap-1.5">
            <AppIcon name="cloud-download" size={13} />
            {messages.branches.fetch}
          </span>
        </MenuItem>
        <MenuSeparator />
        {loading && !overview ? (
          <p className="px-2 py-1.5 text-[11.5px] text-[var(--text-secondary)]">{messages.branches.loading}</p>
        ) : null}
        {error ? <p className="px-2 py-1.5 text-[11.5px] text-destructive">{formatAppError(error)}</p> : null}
        <p className="px-2 py-1 text-[11px] text-[var(--text-secondary)]">{messages.branches.local}</p>
        {local.length === 0 ? (
          <p className="px-2 py-1 text-[11.5px] text-[var(--text-secondary)]">{messages.branches.noLocal}</p>
        ) : null}
        {local.map((branch) => (
          <div key={branch.fullName} className="relative">
            <button
              type="button"
              role="menuitem"
              className="flex w-full items-center gap-2 rounded-[3px] px-2 py-1 text-left hover:bg-[var(--surface-hover)]"
              onClick={() => setSubmenu(submenu === branch.fullName ? null : branch.fullName)}
            >
              <span className="flex w-4 shrink-0 justify-center">
                {branch.isCurrent ? <AppIcon name="check" size={12} /> : null}
              </span>
              <span className="flex min-w-0 flex-1 items-center gap-1.5">
                <span className="min-w-0 truncate">{branch.name}</span>
                {branch.upstreamGone ? (
                  <span className="shrink-0 rounded bg-[var(--surface-subtle)] px-1 text-[10px] text-[var(--warn)]">
                    {messages.branches.upstreamGone}
                  </span>
                ) : null}
              </span>
              {branch.ahead || branch.behind ? (
                <span className="flex shrink-0 items-center gap-1 text-[11px] text-[var(--text-secondary)] tabular-nums" title={messages.statusBar.upstreamHint}>
                  <AppIcon name="arrow-up" size={10} />
                  {branch.ahead}
                  <AppIcon name="arrow-down" size={10} />
                  {branch.behind}
                </span>
              ) : null}
              <AppIcon name="chevron-right" size={11} />
            </button>
            {submenu === branch.fullName ? (
              <div className="ml-6 border-l border-[var(--border-subtle)] pl-1">
                <MenuItem
                  disabled={branch.isCurrent || busy}
                  onClick={() => runCheckout({ kind: "local", name: branch.name })}
                >
                  {messages.branches.checkout}
                </MenuItem>
                <MenuItem
                  onClick={() => {
                    setOpen(false);
                    setSubmenu(null);
                    props.onNewBranch({ kind: "ref", fullName: branch.fullName });
                  }}
                >
                  {messages.branches.newFromBranch}
                </MenuItem>
              </div>
            ) : null}
          </div>
        ))}
        <MenuSeparator />
        <p className="px-2 py-1 text-[11px] text-[var(--text-secondary)]">{messages.branches.remote}</p>
        {remotes.length === 0 ? (
          <p className="px-2 py-1 text-[11.5px] text-[var(--text-secondary)]">{messages.branches.noRemote}</p>
        ) : null}
        {remotes.map((group) => (
          <div key={group.name}>
            <p className="flex items-center gap-1 px-2 py-1 text-[11px] text-[var(--text-secondary)]">
              <AppIcon name="cloud" size={11} />
              {group.name}
            </p>
            {group.branches.map((branch) => (
              <RemoteBranchRow
                key={branch.fullName}
                branch={branch}
                remote={group.name}
                onCheckout={() => runCheckout({ kind: "remote", remote: group.name, branch: branch.branch })}
                onNewBranch={() => {
                  setOpen(false);
                  setSubmenu(null);
                  props.onNewBranch({ kind: "ref", fullName: branch.fullName });
                }}
              />
            ))}
          </div>
        ))}
      </Popover>
    </div>
  );
}

/** 更新与推送高频操作组（M13 Q240）。 */
export function SyncActions({
  busy,
  ahead = 0,
  behind = 0,
  onUpdate,
  onPush,
}: {
  busy: boolean;
  ahead?: number;
  behind?: number;
  onUpdate: () => void;
  onPush: () => void;
}) {
  return (
    <div className="tb-sync-group" role="group">
      <button
        type="button"
        className="tb-sync-btn"
        title={messages.branches.updateHint(`${MOD_SYMBOLS.primary}T`)}
        disabled={busy}
        onClick={onUpdate}
      >
        <AppIcon name="arrow-down" size={13} />
        <span className="max-w-[110px] truncate">{messages.branches.update}</span>
        {behind > 0 ? <span className="sync-count">{behind}</span> : null}
      </button>
      <button
        type="button"
        className="tb-sync-btn"
        title={messages.branches.pushHint(`${MOD_SYMBOLS.primary}${MOD_SYMBOLS.shift}K`)}
        disabled={busy}
        onClick={onPush}
      >
        <AppIcon name="arrow-up" size={13} />
        <span className="max-w-[110px] truncate">{messages.branches.push}</span>
        {ahead > 0 ? <span className="sync-count">{ahead}</span> : null}
      </button>
    </div>
  );
}

function RemoteBranchRow({
  branch,
  remote,
  onCheckout,
  onNewBranch,
}: {
  branch: RemoteBranch;
  remote: string;
  onCheckout: () => void;
  onNewBranch: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button
        type="button"
        role="menuitem"
        className="flex w-full items-center gap-2 rounded-[3px] px-2 py-1 text-left hover:bg-[var(--surface-hover)]"
        onClick={() => setOpen((value) => !value)}
      >
        <span className="flex w-4 shrink-0 justify-center">
          {branch.trackingLocal ? <AppIcon name="link" size={11} /> : null}
        </span>
        <span className="min-w-0 flex-1 truncate">{branch.branch}</span>
        {branch.ahead || branch.behind ? (
          <span className="flex shrink-0 items-center gap-1 text-[11px] text-[var(--text-secondary)] tabular-nums">
            <AppIcon name="arrow-up" size={10} />
            {branch.ahead}
            <AppIcon name="arrow-down" size={10} />
            {branch.behind}
          </span>
        ) : null}
        <AppIcon name="chevron-right" size={11} />
      </button>
      {open ? (
        <div className="ml-6 border-l border-[var(--border-subtle)] pl-1">
          <MenuItem onClick={onCheckout}>
            {branch.trackingLocal ? messages.branches.checkoutTracked(branch.trackingLocal) : messages.branches.checkoutAsLocal(branch.branch)}
          </MenuItem>
          <MenuItem onClick={onNewBranch}>{messages.branches.newFromBranch}</MenuItem>
          <p className="px-2 py-0.5 text-[10.5px] text-[var(--text-secondary)]">{`${remote}/${branch.branch}`}</p>
        </div>
      ) : null}
    </div>
  );
}

/** 新建分支对话框的起点说明（三种入口）。 */
export function branchStartLabel(start: BranchStart, currentBranch: string | null, shortOid: string | null): string {
  switch (start.kind) {
    case "head":
      return `${messages.branches.startHead}：${currentBranch ?? shortOid ?? messages.changes.detachedHead}`;
    case "ref":
      return `${messages.branches.startRef}：${start.fullName.replace(/^refs\/(heads|remotes|tags)\//, "")}`;
    case "commit":
      return `${messages.branches.startCommit}：${start.oid.slice(0, 7)}`;
    default:
      return messages.branches.startHead;
  }
}

/** 分支列表的分组标题（供测试与界面复用）。 */
export function groupBranchNames(branches: LocalBranch[]): string[] {
  return branches.map((branch) => branch.name);
}
