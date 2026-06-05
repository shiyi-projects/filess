import { useMemo } from "react";
import type { BatchSummary, TaskStatus } from "../lib/types";

interface ProcessingCardProps {
  batches: BatchSummary[];
}

const ACTIVE_STATUSES = new Set<TaskStatus>([
  "queued",
  "sniffing",
  "parsing",
  "retrieving_context",
  "calling_model",
  "executing",
  "awaiting_review",
]);

const STATUS_LABEL: Record<TaskStatus, string> = {
  queued: "排队中",
  sniffing: "嗅探",
  parsing: "解析",
  retrieving_context: "检索上下文",
  calling_model: "AI 分类",
  awaiting_review: "等待审阅",
  executing: "归档",
  completed: "完成",
  failed: "失败",
  rolled_back: "已回滚",
  skipped: "已跳过",
};

const Spinner = () => (
  <svg
    width="13" height="13" viewBox="0 0 24 24"
    fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round"
    style={{ animation: "spin 1.2s linear infinite" }}
    aria-hidden
  >
    <path d="M21 12a9 9 0 1 1-6.219-8.56" />
  </svg>
);
const CheckIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <polyline points="20 6 9 17 4 12" />
  </svg>
);
const FailIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);
const QueueIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <circle cx="12" cy="12" r="9" /><polyline points="12 7 12 12 15 14" />
  </svg>
);

function basename(path: string): string {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i >= 0 ? path.slice(i + 1) : path;
}
function statusIcon(status: TaskStatus) {
  if (status === "completed") return <CheckIcon />;
  if (status === "failed") return <FailIcon />;
  if (status === "queued") return <QueueIcon />;
  return <Spinner />;
}
function statusColor(status: TaskStatus): string {
  if (status === "completed") return "var(--green)";
  if (status === "failed") return "var(--red)";
  if (status === "queued") return "var(--text-tertiary)";
  return "var(--accent)";
}

export function ProcessingCard({ batches }: ProcessingCardProps) {
  const visibleBatches = useMemo(
    () => batches.filter((b) => b.completed + b.failed < b.total),
    [batches]
  );
  if (visibleBatches.length === 0) return null;

  return (
    <div
      className="card"
      style={{
        marginBottom: "var(--s-5)",
        padding: "var(--s-4) var(--s-5)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--s-4)",
      }}
    >
      {visibleBatches.map((batch) => {
        const total = batch.total;
        const done = batch.completed + batch.failed;
        const pct = total > 0 ? Math.round((done / total) * 100) : 0;
        const pendingTasks = batch.tasks.filter(
          (t) => t.status !== "completed" && t.status !== "failed"
        );

        return (
          <section key={batch.batchId} style={{ display: "flex", flexDirection: "column", gap: "var(--s-2)" }}>
            <header
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                fontSize: 13,
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ color: "var(--accent)", display: "inline-flex" }}><Spinner /></span>
                <strong style={{ color: "var(--text-primary)", letterSpacing: "-0.01em" }}>正在处理</strong>
                <span style={{ color: "var(--text-tertiary)", fontVariantNumeric: "tabular-nums" }}>
                  {done} / {total}
                </span>
                {batch.failed > 0 && (
                  <span style={{ color: "var(--red)", fontSize: 12 }}>· {batch.failed} 失败</span>
                )}
              </div>
              <span
                style={{
                  fontSize: 12,
                  color: "var(--text-secondary)",
                  fontWeight: 600,
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {pct}%
              </span>
            </header>

            <div
              style={{
                height: 6,
                borderRadius: 3,
                background: "var(--bg-active)",
                overflow: "hidden",
                position: "relative",
              }}
            >
              <div
                style={{
                  width: `${pct}%`,
                  height: "100%",
                  background:
                    "linear-gradient(90deg, var(--accent) 0%, var(--accent-hover) 50%, var(--accent) 100%)",
                  backgroundSize: "200% 100%",
                  animation: "shimmer 1.6s linear infinite",
                  transition: "width 280ms var(--ease)",
                }}
              />
            </div>

            <ul
              style={{
                listStyle: "none",
                margin: 0,
                padding: 0,
                display: "flex",
                flexDirection: "column",
                gap: 1,
                maxHeight: 200,
                overflowY: "auto",
              }}
            >
              {pendingTasks.map((task) => {
                const status = task.status as TaskStatus;
                const isActive = ACTIVE_STATUSES.has(status) && status !== "queued";
                return (
                  <li
                    key={task.taskId}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      padding: "5px 8px",
                      borderRadius: "var(--r-sm)",
                      fontSize: 12,
                      background: isActive ? "var(--accent-soft)" : "transparent",
                    }}
                    title={task.errorMessage ?? task.sourcePath}
                  >
                    <span style={{ color: statusColor(status), display: "inline-flex" }}>
                      {statusIcon(status)}
                    </span>
                    <span
                      style={{
                        flex: 1,
                        color: "var(--text-primary)",
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {basename(task.sourcePath)}
                    </span>
                    <span
                      style={{
                        opacity: 0.8,
                        fontSize: 11,
                        color:
                          status === "failed" ? "var(--red)" : "var(--text-tertiary)",
                      }}
                    >
                      {STATUS_LABEL[status] ?? status}
                    </span>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
