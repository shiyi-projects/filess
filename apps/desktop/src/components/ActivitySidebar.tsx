import type { TaskSummary } from "../lib/types";
import { getStatusInfo } from "../lib/status";

interface ActivitySidebarProps {
  open: boolean;
  onClose: () => void;
  tasks: TaskSummary[];
  onReviewClick: (taskId: string) => void;
}

const DotIcon = ({ variant }: { variant: string }) => {
  const color = variant === "green" ? "var(--green)" : variant === "amber" ? "var(--amber)" : variant === "red" ? "var(--red)" : "var(--text-tertiary)";
  return (
    <svg width="13" height="13" viewBox="0 0 16 16"><circle cx="8" cy="8" r="3" fill={color} /></svg>
  );
};

export function ActivitySidebar({ open, onClose, tasks, onReviewClick }: ActivitySidebarProps) {
  if (!open) return null;

  return (
    <aside className="activity-sidebar">
      <header className="activity-sidebar__header">
        <span className="activity-sidebar__title">队列</span>
        <button className="rail-btn" onClick={onClose} title="关闭" type="button">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </header>
      <div className="activity-sidebar__body">
        {tasks.length === 0 && (
          <div className="empty-state">
            <div className="empty-state__hint" style={{ paddingTop: "var(--s-8)" }}>无处理中的任务</div>
          </div>
        )}
        {tasks.map(task => {
          const info = getStatusInfo(task.status);
          const isReview = task.status === "awaiting_review";
          const name = task.sourcePath.split(/[/\\]/).pop() ?? task.sourcePath;
          return (
            <div key={task.taskId}
              className={`activity-item ${isReview ? "activity-item--review" : ""}`}
              onClick={isReview ? () => onReviewClick(task.taskId) : undefined}
              role={isReview ? "button" : undefined}
              tabIndex={isReview ? 0 : undefined}
              onKeyDown={isReview ? e => e.key === "Enter" && onReviewClick(task.taskId) : undefined}
            >
              <span className="activity-item__icon"><DotIcon variant={info.variant} /></span>
              <span className="activity-item__name" title={task.sourcePath}>{name}</span>
              <span className="tag tag--muted">{info.label}</span>
            </div>
          );
        })}
      </div>
    </aside>
  );
}
