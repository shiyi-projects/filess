interface StatsBarProps {
  todayCount: number;
  pendingCount: number;
  queuedCount: number;
  totalManaged: number;
  sidecarOnline: boolean;
  /** Clicking "总管理文件" opens the sidebar (file browser). */
  onBrowseClick: () => void;
}

export function StatsBar({
  todayCount,
  pendingCount,
  queuedCount,
  totalManaged,
  sidecarOnline,
  onBrowseClick,
}: StatsBarProps) {
  return (
    <footer className="stats-bar">
      <div className="stats-bar__item">
        <span>今日整理</span>
        <span className="stats-bar__value">{todayCount}</span>
      </div>

      <div className="stats-bar__separator" />

      <div className="stats-bar__item">
        <span>排队中</span>
        <span
          className="stats-bar__value"
          style={queuedCount > 0 ? { color: "var(--accent)" } : undefined}
        >
          {queuedCount}
        </span>
      </div>

      <div className="stats-bar__separator" />

      <div className="stats-bar__item">
        <span>待确认</span>
        <span
          className="stats-bar__value"
          style={pendingCount > 0 ? { color: "var(--amber)" } : undefined}
        >
          {pendingCount}
        </span>
      </div>

      <div className="stats-bar__separator" />

      <div
        className="stats-bar__item stats-bar__clickable"
        onClick={onBrowseClick}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => e.key === "Enter" && onBrowseClick()}
        title="点击浏览全部文件"
      >
        <span>总管理文件</span>
        <span className="stats-bar__value">{totalManaged}</span>
      </div>

      <div className="stats-bar__separator" />

      <div className="stats-bar__item">
        <span
          className={`stats-bar__dot ${sidecarOnline ? "" : "stats-bar__dot--offline"}`}
        />
        <span>{sidecarOnline ? "AI 就绪" : "AI 离线"}</span>
      </div>
    </footer>
  );
}
