import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-shell";

// 开源地址固定指向本仓库；B站为作者主页。
const GITHUB_URL = "https://github.com/shiyi-projects/filess";
const BILIBILI_URL = "https://space.bilibili.com/19276680";

/**
 * 主界面底部的署名条：logo 占位 + by shiyi0x7f + 版本号 + 开源/B站 链接。
 * 版本号通过 Tauri 的 getVersion() 读取，始终与打包版本一致。
 */
export function AppFooter() {
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
    <footer className="app-footer">
      <div className="app-footer__brand">
        {/* logo 占位：替换真实 logo 时改成 <img src=... className="app-footer__logo" /> */}
        <span className="app-footer__logo" aria-hidden="true">
          LOGO
        </span>
        <span className="app-footer__by">by shiyi0x7f</span>
        {version && <span className="app-footer__version">v{version}</span>}
      </div>

      <div className="app-footer__links">
        <button
          type="button"
          className="app-footer__link"
          onClick={() => openExternal(GITHUB_URL)}
          title={GITHUB_URL}
        >
          开源地址
        </button>
        <span className="app-footer__dot" aria-hidden="true" />
        <button
          type="button"
          className="app-footer__link"
          onClick={() => openExternal(BILIBILI_URL)}
          title="B站主页"
        >
          B站
        </button>
      </div>
    </footer>
  );
}
