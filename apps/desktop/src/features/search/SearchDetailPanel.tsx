import type { SearchResultItem } from "../../lib/types";
import { SectionCard } from "../../components/SectionCard";

interface SearchDetailPanelProps {
  item: SearchResultItem | null;
  onOpenFile: (itemId: string) => Promise<void>;
  onRevealInFolder: (itemId: string) => Promise<void>;
  onCopyPath: (itemId: string) => Promise<void>;
}

export function SearchDetailPanel({
  item,
  onOpenFile,
  onRevealInFolder,
  onCopyPath
}: SearchDetailPanelProps) {
  if (!item) {
    return (
      <SectionCard title="文件详情" subtitle="搜索结果">
        <div className="empty-state">当前没有选中的搜索结果。</div>
      </SectionCard>
    );
  }

  return (
    <SectionCard
      title="文件详情"
      subtitle="查看路径并执行操作"
      actions={
        <div className="inline-actions">
          <button
            type="button"
            className="action-button action-button--primary"
            onClick={() => onOpenFile(item.itemId)}
          >
            打开文件
          </button>
          <button
            type="button"
            className="action-button"
            onClick={() => onRevealInFolder(item.itemId)}
          >
            打开所在位置
          </button>
          <button
            type="button"
            className="action-button"
            onClick={() => onCopyPath(item.itemId)}
          >
            复制路径
          </button>
        </div>
      }
    >
      <div className="detail-hero">
        <div>
          <span className="label">当前文件</span>
          <h3>{item.title}</h3>
        </div>
        <span className="status-pill">{item.categoryName}</span>
      </div>
      <div className="detail-grid">
        <div className="detail-grid__full">
          <span className="label">本地路径</span>
          <strong>{item.currentPath}</strong>
        </div>
        <div className="detail-grid__full">
          <span className="label">摘要</span>
          <p>{item.summaryExcerpt ?? "无摘要"}</p>
        </div>
        <div>
          <span className="label">命中字段</span>
          <strong>{item.matchedFields.join(" / ")}</strong>
        </div>
        <div>
          <span className="label">最近整理时间</span>
          <strong>{item.lastProcessedAt}</strong>
        </div>
      </div>
    </SectionCard>
  );
}
