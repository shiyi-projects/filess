import type { SettingsSnapshot } from "../../lib/types";

interface SettingsPanelProps {
  settings: SettingsSnapshot;
}

function formatValue(value: unknown): string {
  if (typeof value === "boolean") {
    return value ? "已启用" : "已关闭";
  }

  if (value === null || value === undefined) {
    return "未设置";
  }

  if (typeof value === "object") {
    return JSON.stringify(value);
  }

  return String(value);
}

export function SettingsPanel({ settings }: SettingsPanelProps) {
  const groups = [
    ["基础路径", settings.paths],
    ["分类规则", settings.classificationRules],
    ["AI 设置", settings.ai],
    ["整理策略", settings.organizePolicy],
    ["数据与日志", settings.dataAndLogs]
  ] as const;

  return (
    <section className="settings-page">
      <header className="settings-page__header">
        <div>
          <span className="content-header__eyebrow">设置中心</span>
          <h1>设置</h1>
          <p>管理路径、模型与整理策略。</p>
        </div>
      </header>
      <div className="settings-groups">
        {groups.map(([name, value]) => (
          <article key={name} className="settings-group settings-group--page">
            <header className="settings-group__header">
              <h3>{name}</h3>
              <span>独立维护</span>
            </header>
            <div className="settings-group__list">
              {Object.entries(value).map(([key, entryValue]) => (
                <div key={key} className="settings-row">
                  <span className="settings-row__key">{key}</span>
                  <strong className="settings-row__value">
                    {formatValue(entryValue)}
                  </strong>
                </div>
              ))}
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
