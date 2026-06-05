import type { BatchSummary } from "../../lib/types";
import { SectionCard } from "../../components/SectionCard";

interface BatchPanelProps {
  batches: BatchSummary[];
  selectedBatchId: string | null;
  onSelectBatch: (batchId: string) => void;
}

export function BatchPanel({
  batches,
  selectedBatchId,
  onSelectBatch
}: BatchPanelProps) {
  return (
    <SectionCard title="最近批次" subtitle="队列视图">
      <div className="batch-list">
        {batches.map((batch) => (
          <button
            key={batch.batchId}
            className={
              selectedBatchId === batch.batchId
                ? "batch-item batch-item--active"
                : "batch-item"
            }
            onClick={() => onSelectBatch(batch.batchId)}
            type="button"
          >
            <div className="batch-item__title">
              <strong>{batch.batchId}</strong>
              <span className="status-pill">{batch.status}</span>
            </div>
            <div className="batch-item__metrics">
              <span>{batch.total} 个任务</span>
              <span>{batch.awaitingReview} 待确认</span>
              <span>{batch.failed} 失败</span>
            </div>
          </button>
        ))}
      </div>
    </SectionCard>
  );
}
