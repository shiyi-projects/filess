import { useEffect, useRef, useState, useCallback, useMemo, type KeyboardEvent } from "react";
import type { SearchResultItem, SearchResults } from "../lib/types";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onSearch: (query: string) => Promise<SearchResults>;
  onOpenFile: (itemId: string) => void;
  onRevealInFolder: (itemId: string) => void;
  onCopyPath: (itemId: string) => void;
}

const SearchIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
);

const DocIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
  </svg>
);

const EMPTY: SearchResults = { semantic: [], filename: [] };

export function CommandPalette({ open, onClose, onSearch, onOpenFile, onRevealInFolder, onCopyPath }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResults>(EMPTY);
  const [activeIdx, setActiveIdx] = useState(0);
  const [searched, setSearched] = useState(false);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const timer = useRef<number>(0);

  // Flatten the two groups into a single keyboard-navigable list
  const flat: SearchResultItem[] = useMemo(
    () => [...results.semantic, ...results.filename],
    [results]
  );

  useEffect(() => {
    if (open) {
      setQuery(""); setResults(EMPTY); setActiveIdx(0); setSearched(false); setBusy(false);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) { setResults(EMPTY); setSearched(false); return; }
    setBusy(true);
    try {
      const r = await onSearch(q);
      setResults(r); setActiveIdx(0); setSearched(true);
    } catch (err) {
      console.error("search failed:", err);
      setResults(EMPTY); setSearched(true);
    } finally {
      setBusy(false);
    }
  }, [onSearch]);

  const onChange = (v: string) => {
    setQuery(v);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => doSearch(v), 300);
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") { onClose(); return; }
    if (e.key === "ArrowDown") { e.preventDefault(); setActiveIdx(i => Math.min(i + 1, flat.length - 1)); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); setActiveIdx(i => Math.max(i - 1, 0)); return; }
    if (e.key === "Enter" && flat[activeIdx]) {
      e.preventDefault();
      if (e.ctrlKey) onRevealInFolder(flat[activeIdx].itemId);
      else onOpenFile(flat[activeIdx].itemId);
      onClose();
    }
    if (e.key === "c" && e.ctrlKey && flat[activeIdx]) { e.preventDefault(); onCopyPath(flat[activeIdx].itemId); }
  };

  if (!open) return null;

  const renderItem = (item: SearchResultItem, globalIdx: number) => (
    <button
      key={item.itemId}
      className={`cmd-item ${globalIdx === activeIdx ? "cmd-item--active" : ""}`}
      onClick={() => { onOpenFile(item.itemId); onClose(); }}
      onMouseEnter={() => setActiveIdx(globalIdx)}
      type="button"
    >
      <span className="cmd-item__icon"><DocIcon /></span>
      <div className="cmd-item__body">
        <div className="cmd-item__title">{item.title}</div>
        <div className="cmd-item__sub">{item.summaryExcerpt ?? item.currentPath}</div>
      </div>
      <span className="cmd-item__tag">{item.categoryName}</span>
      {typeof item.score === "number" && (
        <span className="cmd-item__score" style={{ marginLeft: 8, fontSize: 11, opacity: 0.65 }}>
          {(item.score * 100).toFixed(0)}%
        </span>
      )}
    </button>
  );

  const totalCount = results.semantic.length + results.filename.length;

  return (
    <div className="cmd-overlay" onClick={onClose}>
      <div className="cmd-palette" onClick={e => e.stopPropagation()} onKeyDown={onKey} role="dialog">
        <div className="cmd-input-wrap">
          <span className="cmd-input-wrap__icon"><SearchIcon /></span>
          <input
            ref={inputRef}
            className="cmd-input"
            placeholder="按内容或文件名搜索..."
            value={query}
            onChange={e => onChange(e.target.value)}
          />
          <span className="cmd-kbd">Esc</span>
        </div>
        <div className="cmd-results">
          {!searched && !busy && <div className="cmd-empty">输入关键词开始搜索</div>}
          {busy && <div className="cmd-empty">搜索中...</div>}
          {searched && !busy && totalCount === 0 && <div className="cmd-empty">没有匹配结果</div>}

          {results.semantic.length > 0 && (
            <>
              <div
                className="cmd-section-header"
                style={{
                  padding: "var(--s-2) var(--s-4)",
                  fontSize: 11,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  opacity: 0.6,
                  letterSpacing: 0.5,
                }}
              >
                相关内容
              </div>
              {results.semantic.map((it, i) => renderItem(it, i))}
            </>
          )}

          {results.filename.length > 0 && (
            <>
              <div
                className="cmd-section-header"
                style={{
                  padding: "var(--s-2) var(--s-4)",
                  fontSize: 11,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  opacity: 0.6,
                  letterSpacing: 0.5,
                  borderTop: results.semantic.length > 0 ? "1px solid var(--border, rgba(255,255,255,0.06))" : undefined,
                }}
              >
                文件名匹配
              </div>
              {results.filename.map((it, i) => renderItem(it, results.semantic.length + i))}
            </>
          )}
        </div>
        {totalCount > 0 && (
          <div className="cmd-footer">
            <span className="cmd-footer__hint"><span className="cmd-kbd">↑↓</span> 选择</span>
            <span className="cmd-footer__hint"><span className="cmd-kbd">↵</span> 打开</span>
            <span className="cmd-footer__hint"><span className="cmd-kbd">Ctrl↵</span> 定位</span>
          </div>
        )}
      </div>
    </div>
  );
}
