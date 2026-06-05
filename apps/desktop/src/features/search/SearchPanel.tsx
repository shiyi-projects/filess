import { FormEvent, useState } from "react";
import type { SearchResultItem } from "../../lib/types";
import { SectionCard } from "../../components/SectionCard";

interface SearchPanelProps {
  results: SearchResultItem[];
  onSearch: (query: string) => Promise<void>;
  onSelect: (item: SearchResultItem) => void;
  hasSearched: boolean;
  selectedItemId: string | null;
}

export function SearchPanel({
  results,
  onSearch,
  onSelect,
  hasSearched,
  selectedItemId
}: SearchPanelProps) {
  const [query, setQuery] = useState("");

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSearch(query);
  }

  return (
    <SectionCard title="搜索" subtitle="文件名、分类、路径、摘要">
      <form className="search-form" onSubmit={handleSubmit}>
        <input
          placeholder="搜索已纳入管理的文件"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button type="submit">搜索</button>
      </form>
      <div className="search-results">
        {!hasSearched ? (
          <div className="search-placeholder">
            输入关键词后，在这里显示匹配结果和路径摘要。
          </div>
        ) : null}
        {hasSearched && results.length === 0 ? (
          <div className="search-placeholder">没有找到匹配结果。</div>
        ) : null}
        {hasSearched &&
          results.map((item) => (
          <button
            key={item.itemId}
            className={
              selectedItemId === item.itemId
                ? "search-result-item search-result-item--active"
                : "search-result-item"
            }
            onClick={() => onSelect(item)}
            type="button"
          >
            <div className="search-result-item__header">
              <strong>{item.title}</strong>
              <span>{item.categoryName}</span>
            </div>
            <p>{item.currentPath}</p>
            <small>{item.summaryExcerpt ?? "无摘要"}</small>
          </button>
          ))}
      </div>
    </SectionCard>
  );
}
