import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-shell";
import githubIcon from "../assets/github.svg";
import bilibiliIcon from "../assets/bilibili.svg";

// 开源地址指向本仓库，B站为作者主页。
const GITHUB_URL = "https://github.com/shiyi-projects/filess";
const BILIBILI_URL = "https://space.bilibili.com/19276680";

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
  // 版本号通过 Tauri getVersion() 读取，始终与打包版本一致。
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch((err) => console.error("getVersion failed:", err));
  }, []);

  const openExternal = (url: string) => {
    open(url).catch((err) => console.error("open external url failed:", err));
  };

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

      {/* 右侧：作者署名 + 版本 + 开源/B站 图标 */}
      <div className="stats-bar__credit">
        <span className="stats-bar__by">by shiyi0x7f</span>
        {version && <span className="stats-bar__ver">v{version}</span>}
        <button
          type="button"
          className="stats-bar__icon"
          onClick={() => openExternal(GITHUB_URL)}
          title="开源地址"
          aria-label="GitHub"
        >
          <img src={githubIcon} alt="GitHub" />
        </button>
        <button
          type="button"
          className="stats-bar__icon"
          onClick={() => openExternal(BILIBILI_URL)}
          title="B站主页"
          aria-label="哔哩哔哩"
        >
          <img src={bilibiliIcon} alt="B站" />
        </button>
      </div>
    </footer>
  );
}
