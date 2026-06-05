import { useEffect, useState } from "react";
import type { ResultItem } from "../lib/types";
import { formatTimeAgo } from "../lib/status";

interface ResultListProps {
  results: ResultItem[];
  onOpenFile: (itemId: string) => void;
  onRevealInFolder: (itemId: string) => void;
  onCopyPath: (itemId: string) => void;
  onUndo: (operationId: string) => void;
  onReclassify: (itemId: string) => void;
}

// Pixel budget heuristic: each result-card is ~96px tall; everything above
// the result list (drop-zone, processing card, paddings) and the stats-bar
// at the bottom take about 420px combined. Adjust if layout changes.
const RESERVED_PX = 420;
const CARD_PX = 96;
const MIN_VISIBLE = 3;

function computeVisibleCount(): number {
  if (typeof window === "undefined") return MIN_VISIBLE;
  const available = window.innerHeight - RESERVED_PX;
  return Math.max(MIN_VISIBLE, Math.floor(available / CARD_PX));
}

/* Minimal SVG icons */
const FileIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
  </svg>
);

const FolderIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

const OpenIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
    <polyline points="15 3 21 3 21 9" />
    <line x1="10" y1="14" x2="21" y2="3" />
  </svg>
);

const PinIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" />
    <circle cx="12" cy="10" r="3" />
  </svg>
);

const CopyIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </svg>
);

const UndoIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="1 4 1 10 7 10" />
    <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
  </svg>
);

const ReclassifyIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 12a9 9 0 1 1-3-6.7" />
    <polyline points="21 4 21 10 15 10" />
  </svg>
);

const EmptyIcon = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="3" width="7" height="7" />
    <rect x="14" y="3" width="7" height="7" />
    <rect x="14" y="14" width="7" height="7" />
    <rect x="3" y="14" width="7" height="7" />
  </svg>
);

export function ResultList({ results, onOpenFile, onRevealInFolder, onCopyPath, onUndo, onReclassify }: ResultListProps) {
  const [maxVisible, setMaxVisible] = useState(computeVisibleCount);

  useEffect(() => {
    const handler = () => setMaxVisible(computeVisibleCount());
    window.addEventListener("resize", handler);
    return () => window.removeEventListener("resize", handler);
  }, []);

  if (results.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-state__icon"><EmptyIcon /></div>
        <div className="empty-state__title">还没有整理记录</div>
        <div className="empty-state__hint">将文件拖到上方区域，整理完成后会在这里显示去向</div>
      </div>
    );
  }

  const visible = results.slice(0, maxVisible);
  const hidden = results.length - visible.length;

  return (
    <>
      <div className="section-header">
        <span className="section-header__title">整理结果</span>
        <span className="section-header__count">
          {hidden > 0 ? `显示 ${visible.length} / ${results.length}` : `${results.length} 个文件`}
        </span>
      </div>
      <div className="result-list">
        {visible.map((item, i) => (
          <article
            key={item.itemId}
            className="result-card"
            style={{ animationDelay: `${i * 25}ms`, cursor: "pointer" }}
            onClick={() => onOpenFile(item.itemId)}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === "Enter") onOpenFile(item.itemId);
            }}
          >
            <div className="result-card__icon">
              {item.itemType === "folder" ? <FolderIcon /> : <FileIcon />}
            </div>
            <div className="result-card__body">
              <div className="result-card__name">{item.fileName}</div>
              <div className="result-card__path">{item.currentPath}</div>
              <div className="result-card__meta">
                <span className="tag tag--accent">{item.categoryName}</span>
                <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>{formatTimeAgo(item.processedAt)}</span>
              </div>
            </div>
            <div className="result-card__actions" onClick={(e) => e.stopPropagation()}>
              <button className="action-btn" title="打开文件" onClick={() => onOpenFile(item.itemId)} type="button"><OpenIcon /></button>
              <button className="action-btn" title="打开所在位置" onClick={() => onRevealInFolder(item.itemId)} type="button"><PinIcon /></button>
              <button className="action-btn" title="复制路径" onClick={() => onCopyPath(item.itemId)} type="button"><CopyIcon /></button>
              <button className="action-btn" title="重新分类(让 AI 再判断一次)" onClick={() => onReclassify(item.itemId)} type="button"><ReclassifyIcon /></button>
              <button className="action-btn action-btn--danger" title="撤销整理(把文件移回原位置)" onClick={() => onUndo(item.itemId)} type="button"><UndoIcon /></button>
            </div>
          </article>
        ))}
      </div>
      {hidden > 0 && (
        <div
          style={{
            padding: "var(--s-3) var(--s-4)",
            fontSize: 12,
            opacity: 0.6,
            textAlign: "center",
          }}
        >
          还有 {hidden} 条更早的记录,使用 Ctrl+K 搜索查看
        </div>
      )}
    </>
  );
}
