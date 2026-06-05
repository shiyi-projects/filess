import { useCallback, useEffect, useMemo, useState } from "react";
import type { ResultItem, TaskSummary } from "../lib/types";
import { listAllItems } from "../lib/tauri";

interface SidebarProps {
  open: boolean;
  onClose: () => void;
  /** Tasks currently in flight (queued + active). */
  tasks: TaskSummary[];
  /** Bumps when batch state changes — triggers tree refetch. */
  refreshKey: number;
  onOpenFile: (itemId: string) => void;
  onRevealInFolder: (itemId: string) => void;
  onCopyPath: (itemId: string) => void;
  onReviewClick: (taskId: string) => void;
}

// ── Tree types ────────────────────────────────────────────────
interface TreeNode {
  name: string;
  fullPath: string;
  children: Map<string, TreeNode>;
  items: ResultItem[];
  totalCount: number;
}

function buildTree(items: ResultItem[]): TreeNode {
  const root: TreeNode = {
    name: "",
    fullPath: "",
    children: new Map(),
    items: [],
    totalCount: 0,
  };
  for (const item of items) {
    const segments = (item.categoryName || "未分类")
      .split("/")
      .map((s) => s.trim())
      .filter(Boolean);
    if (segments.length === 0) segments.push("未分类");
    let node = root;
    let acc = "";
    for (const seg of segments) {
      acc = acc ? `${acc}/${seg}` : seg;
      let child = node.children.get(seg);
      if (!child) {
        child = { name: seg, fullPath: acc, children: new Map(), items: [], totalCount: 0 };
        node.children.set(seg, child);
      }
      node = child;
    }
    node.items.push(item);
  }
  function fold(n: TreeNode): number {
    let count = n.items.length;
    for (const child of n.children.values()) count += fold(child);
    n.totalCount = count;
    return count;
  }
  fold(root);
  return root;
}

// ── Icons ─────────────────────────────────────────────────────
const FolderClosed = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);
const FileIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
  </svg>
);
const Chevron = ({ expanded }: { expanded: boolean }) => (
  <svg
    width="10" height="10" viewBox="0 0 24 24"
    fill="none" stroke="currentColor" strokeWidth="2.5"
    strokeLinecap="round" strokeLinejoin="round"
    style={{ transform: expanded ? "rotate(90deg)" : "none", transition: "transform 120ms" }}
  >
    <polyline points="9 18 15 12 9 6" />
  </svg>
);
const Spinner = () => (
  <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" style={{ animation: "spin 1.2s linear infinite" }}>
    <path d="M21 12a9 9 0 1 1-6.219-8.56" />
  </svg>
);
const CloseIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

// ── Tree row ──────────────────────────────────────────────────
interface TreeRowProps {
  node: TreeNode;
  depth: number;
  expanded: Set<string>;
  toggle: (path: string) => void;
  onItemClick: (item: ResultItem, e: React.MouseEvent) => void;
  onItemContextMenu?: (item: ResultItem, e: React.MouseEvent) => void;
}

function TreeRow({ node, depth, expanded, toggle, onItemClick, onItemContextMenu }: TreeRowProps) {
  const isExpanded = expanded.has(node.fullPath);
  const hasChildren = node.children.size > 0 || node.items.length > 0;
  const indent = depth * 12;

  return (
    <>
      <button
        type="button"
        className="tree-row tree-row--folder"
        onClick={() => hasChildren && toggle(node.fullPath)}
        style={{ paddingLeft: 8 + indent, cursor: hasChildren ? "pointer" : "default" }}
      >
        <span className="tree-row__chev">
          {hasChildren ? <Chevron expanded={isExpanded} /> : null}
        </span>
        <span className="tree-row__icon-folder"><FolderClosed /></span>
        <span className="tree-row__name">{node.name}</span>
        <span className="tree-row__count">{node.totalCount}</span>
      </button>
      {isExpanded && (
        <>
          {[...node.children.values()]
            .sort((a, b) => b.totalCount - a.totalCount)
            .map((child) => (
              <TreeRow
                key={child.fullPath}
                node={child}
                depth={depth + 1}
                expanded={expanded}
                toggle={toggle}
                onItemClick={onItemClick}
                onItemContextMenu={onItemContextMenu}
              />
            ))}
          {node.items.map((item) => (
            <button
              key={item.itemId}
              type="button"
              onClick={(e) => onItemClick(item, e)}
              onContextMenu={(e) => {
                if (onItemContextMenu) {
                  e.preventDefault();
                  onItemContextMenu(item, e);
                }
              }}
              title={item.currentPath}
              className="tree-row"
              style={{ paddingLeft: 8 + (depth + 1) * 12 + 12, cursor: "pointer" }}
            >
              <span className="tree-row__icon-file">
                {item.itemType === "folder" ? <FolderClosed /> : <FileIcon />}
              </span>
              <span className="tree-row__name">{item.fileName}</span>
            </button>
          ))}
        </>
      )}
    </>
  );
}

// ── Sidebar component ─────────────────────────────────────────
export function Sidebar({
  open,
  onClose,
  tasks,
  refreshKey,
  onOpenFile,
  onRevealInFolder,
  onReviewClick,
}: SidebarProps) {
  const [allItems, setAllItems] = useState<ResultItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [showProcessing, setShowProcessing] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const items = await listAllItems();
      setAllItems(items);
    } catch (err) {
      console.error("listAllItems failed:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) reload();
  }, [open, refreshKey, reload]);

  const tree = useMemo(() => buildTree(allItems), [allItems]);

  const toggle = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const handleItemClick = useCallback((item: ResultItem, e: React.MouseEvent) => {
    if (e.ctrlKey || e.metaKey) onRevealInFolder(item.itemId);
    else onOpenFile(item.itemId);
  }, [onOpenFile, onRevealInFolder]);

  const handleItemContextMenu = useCallback((item: ResultItem) => {
    onRevealInFolder(item.itemId);
  }, [onRevealInFolder]);

  if (!open) return null;

  return (
    <aside
      className="activity-sidebar"
      style={{ display: "flex", flexDirection: "column" }}
    >
      <header className="activity-sidebar__header">
        <span className="activity-sidebar__title">浏览</span>
        <button className="rail-btn" onClick={onClose} title="关闭" type="button" style={{ width: 28, height: 28 }}>
          <CloseIcon />
        </button>
      </header>

      <div style={{ flex: 1, overflowY: "auto", padding: "var(--s-2) 0" }}>
        {/* Processing — always pinned at the top while there are active tasks */}
        {tasks.length > 0 && (
          <section style={{ marginBottom: "var(--s-3)" }}>
            <button
              type="button"
              onClick={() => setShowProcessing((v) => !v)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "6px 12px",
                width: "100%",
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                fontSize: 11,
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: 0.4,
                cursor: "pointer",
              }}
            >
              <Chevron expanded={showProcessing} />
              <Spinner />
              <span>正在处理</span>
              <span style={{ opacity: 0.65, fontWeight: 400 }}>({tasks.length})</span>
            </button>
            {showProcessing &&
              tasks.map((task) => {
                const name = task.sourcePath.split(/[\\/]/).pop() ?? task.sourcePath;
                const isReview = task.status === "awaiting_review";
                return (
                  <button
                    key={task.taskId}
                    type="button"
                    onClick={isReview ? () => onReviewClick(task.taskId) : undefined}
                    title={task.errorMessage ?? task.sourcePath}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      padding: "4px 12px 4px 28px",
                      width: "100%",
                      textAlign: "left",
                      background: "transparent",
                      border: "none",
                      color: "var(--text-secondary)",
                      fontSize: 12,
                      cursor: isReview ? "pointer" : "default",
                    }}
                  >
                    <Spinner />
                    <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {name}
                    </span>
                    <span style={{ fontSize: 10, color: "var(--text-tertiary)" }}>{task.status}</span>
                  </button>
                );
              })}
          </section>
        )}

        {/* Category tree */}
        <section
          style={{
            borderTop: tasks.length > 0 ? "1px solid var(--border-subtle)" : undefined,
            paddingTop: tasks.length > 0 ? "var(--s-2)" : 0,
          }}
        >
          <div
            style={{
              padding: "6px 12px",
              fontSize: 11,
              fontWeight: 600,
              textTransform: "uppercase",
              letterSpacing: 0.4,
              color: "var(--text-secondary)",
            }}
          >
            分类
          </div>
          {loading && allItems.length === 0 && (
            <div style={{ padding: "var(--s-4)", fontSize: 12, color: "var(--text-tertiary)" }}>加载中...</div>
          )}
          {!loading && allItems.length === 0 && (
            <div style={{ padding: "var(--s-4)", fontSize: 12, color: "var(--text-tertiary)" }}>
              还没有整理过的文件
            </div>
          )}
          {[...tree.children.values()]
            .sort((a, b) => b.totalCount - a.totalCount)
            .map((child) => (
              <TreeRow
                key={child.fullPath}
                node={child}
                depth={0}
                expanded={expanded}
                toggle={toggle}
                onItemClick={handleItemClick}
                onItemContextMenu={handleItemContextMenu}
              />
            ))}
        </section>
      </div>

      <div
        style={{
          padding: "var(--s-2) var(--s-3)",
          fontSize: 10,
          color: "var(--text-tertiary)",
          borderTop: "1px solid var(--border-subtle)",
          lineHeight: 1.5,
        }}
      >
        点击打开 · Ctrl+点击 / 右键定位 · <kbd className="cmd-kbd">Ctrl+K</kbd> 搜索
      </div>
    </aside>
  );
}
