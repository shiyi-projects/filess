interface IconRailProps {
  onSearchClick: () => void;
  onSettingsClick: () => void;
  pendingReviewCount: number;
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
}

const HomeIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <polyline points="9 22 9 12 15 12 15 22" />
  </svg>
);

const FolderIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

const SearchIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
);

const SettingsIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

export function IconRail({
  onSearchClick,
  onSettingsClick,
  pendingReviewCount,
  sidebarOpen,
  onToggleSidebar,
}: IconRailProps) {
  return (
    <nav className="icon-rail">
      <div className="icon-rail__logo" title="Filess">F</div>

      {/* Primary work modes */}
      <button className="rail-btn rail-btn--active" title="工作台" type="button">
        <HomeIcon />
      </button>

      <button
        className={`rail-btn ${sidebarOpen ? "rail-btn--active" : ""}`}
        title="浏览(分类与正在处理)"
        onClick={onToggleSidebar}
        type="button"
      >
        <FolderIcon />
        {pendingReviewCount > 0 && (
          <span className="rail-btn__badge">{pendingReviewCount}</span>
        )}
      </button>

      <button
        className="rail-btn"
        title="搜索 (Ctrl+K)"
        onClick={onSearchClick}
        type="button"
      >
        <SearchIcon />
      </button>

      <div className="rail-spacer" />

      {/* Secondary / global actions live at the bottom */}
      <button
        className="rail-btn"
        title="设置"
        onClick={onSettingsClick}
        type="button"
      >
        <SettingsIcon />
      </button>
    </nav>
  );
}
