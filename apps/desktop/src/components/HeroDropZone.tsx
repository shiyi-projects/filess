interface HeroDropZoneProps {
  dragActive: boolean;
  onBrowseClick?: () => void;
}

const ArrowDownIcon = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
  </svg>
);

export function HeroDropZone({ dragActive, onBrowseClick }: HeroDropZoneProps) {
  const cls = dragActive ? "drop-zone drop-zone--active" : "drop-zone";
  // The entire zone is clickable — falling back to the browse handler when set.
  // This way users don't have to aim at the small button to pick files.
  const handleZoneClick = () => {
    if (!dragActive && typeof onBrowseClick === "function") onBrowseClick();
  };

  return (
    <div
      className={cls}
      onClick={handleZoneClick}
      role={onBrowseClick ? "button" : undefined}
      tabIndex={onBrowseClick ? 0 : undefined}
      onKeyDown={(e) => {
        if (onBrowseClick && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onBrowseClick();
        }
      }}
      style={{ cursor: onBrowseClick && !dragActive ? "pointer" : "default" }}
    >
      <div className="drop-zone__icon"><ArrowDownIcon /></div>
      <div className="drop-zone__title">
        {dragActive ? "松开以开始整理" : "拖入文件或文件夹"}
      </div>
      <div className="drop-zone__hint">
        {dragActive ? "释放鼠标即可" : "支持批量,可拖入多个项目"}
      </div>
      {!dragActive && onBrowseClick && (
        <button
          type="button"
          className="drop-zone__browse"
          onClick={(e) => {
            e.stopPropagation();
            onBrowseClick();
          }}
        >
          或选择文件
        </button>
      )}
    </div>
  );
}
