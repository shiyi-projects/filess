import type { TaskStatus } from "./types";

interface StatusInfo {
  label: string;
  variant: "green" | "amber" | "red" | "muted";
}

const STATUS_MAP: Record<TaskStatus, StatusInfo> = {
  queued:              { label: "排队中",       variant: "muted" },
  sniffing:            { label: "识别类型",     variant: "muted" },
  parsing:             { label: "提取内容",     variant: "muted" },
  retrieving_context:  { label: "检索相似",     variant: "muted" },
  calling_model:       { label: "AI 分析中",    variant: "muted" },
  awaiting_review:     { label: "需要确认",     variant: "amber" },
  executing:           { label: "正在移动",     variant: "muted" },
  completed:           { label: "整理完成",     variant: "green" },
  failed:              { label: "处理失败",     variant: "red" },
  rolled_back:         { label: "已撤销",       variant: "muted" },
  skipped:             { label: "已跳过",       variant: "muted" },
};

export function getStatusInfo(status: TaskStatus): StatusInfo {
  return STATUS_MAP[status] ?? { label: status, variant: "muted" as const };
}

export function formatTimeAgo(isoStr: string): string {
  const diff = Date.now() - new Date(isoStr).getTime();
  const s = Math.floor(diff / 1000);
  if (s < 60)  return `${s}秒前`;
  const m = Math.floor(s / 60);
  if (m < 60)  return `${m}分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24)  return `${h}小时前`;
  return `${Math.floor(h / 24)}天前`;
}
