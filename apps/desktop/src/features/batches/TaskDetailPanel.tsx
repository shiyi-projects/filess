import type { BatchSummary } from "../../lib/types";
import { SectionCard } from "../../components/SectionCard";

interface TaskDetailPanelProps {
  batch: BatchSummary | null;
}

export function TaskDetailPanel({ batch }: TaskDetailPanelProps) {
  if (!batch) {
    return (
      <SectionCard title="整理详情" subtitle="选择一个批次">
        <div className="empty-state">当前没有选中的整理批次。</div>
      </SectionCard>
    );
  }

  return (
    <SectionCard title="批次详情" subtitle="当前批次状态">
      <div className="task-detail-summary">
        <div>
          <span className="label">批次状态</span>
          <strong className="metric-value">{batch.status}</strong>
        </div>
        <div>
          <span className="label">任务总数</span>
          <strong className="metric-value">{batch.total}</strong>
        </div>
        <div>
          <span className="label">待确认</span>
          <strong className="metric-value">{batch.awaitingReview}</strong>
        </div>
        <div>
          <span className="label">失败</span>
          <strong className="metric-value">{batch.failed}</strong>
        </div>
      </div>
      <div className="task-list">
        {batch.tasks.map((task) => (
          <article key={task.taskId} className="task-row">
            <div className="task-row__content">
              <strong className="task-row__title">{task.sourcePath}</strong>
              <p>{task.taskId}</p>
            </div>
            <div className="task-row__meta">
              <span className="status-pill">{task.status}</span>
              {task.errorMessage ? <span>{task.errorMessage}</span> : null}
            </div>
          </article>
        ))}
      </div>
    </SectionCard>
  );
}
