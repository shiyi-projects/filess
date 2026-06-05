import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppSettings, CpuInfo } from "../lib/types";
import { getAppSettings, getCpuInfo, updateAppSettings } from "../lib/tauri";
import { eventToHotkey, formatHotkey } from "../lib/hotkey";

interface SettingsDrawerProps {
  open: boolean;
  onClose: () => void;
}

const CHAT_MODELS: string[] = [
  "Qwen/Qwen2.5-7B-Instruct",
  "Qwen/Qwen2.5-14B-Instruct",
  "Qwen/Qwen2.5-32B-Instruct",
  "Qwen/Qwen2.5-72B-Instruct",
  "Qwen/QwQ-32B",
  "deepseek-ai/DeepSeek-V3",
  "deepseek-ai/DeepSeek-V2.5",
  "deepseek-ai/DeepSeek-R1",
  "deepseek-ai/DeepSeek-R1-Distill-Qwen-7B",
  "deepseek-ai/DeepSeek-R1-Distill-Qwen-32B",
  "meta-llama/Meta-Llama-3.1-8B-Instruct",
  "meta-llama/Meta-Llama-3.1-70B-Instruct",
  "google/gemma-2-9b-it",
  "THUDM/glm-4-9b-chat",
];

const EMBEDDING_MODELS: string[] = [
  "BAAI/bge-m3",
  "BAAI/bge-large-zh-v1.5",
  "BAAI/bge-large-en-v1.5",
  "netease-youdao/bce-embedding-base_v1",
];

const CUSTOM_OPTION = "__custom__";

interface ModelPickerProps {
  value: string;
  presets: string[];
  onChange: (next: string) => void;
}

function ModelPicker({ value, presets, onChange }: ModelPickerProps) {
  const isPreset = presets.includes(value);
  const [mode, setMode] = useState<string>(isPreset ? value : CUSTOM_OPTION);
  const [custom, setCustom] = useState<string>(isPreset ? "" : value);

  useEffect(() => {
    if (presets.includes(value)) { setMode(value); setCustom(""); }
    else { setMode(CUSTOM_OPTION); setCustom(value); }
  }, [value, presets]);

  const handleSelect = (v: string) => {
    setMode(v);
    if (v !== CUSTOM_OPTION) onChange(v);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <select className="input" value={mode} onChange={(e) => handleSelect(e.target.value)}>
        {presets.map((p) => (<option key={p} value={p}>{p}</option>))}
        <option value={CUSTOM_OPTION}>自定义...</option>
      </select>
      {mode === CUSTOM_OPTION && (
        <input
          className="input"
          type="text"
          value={custom}
          onChange={(e) => { setCustom(e.target.value); onChange(e.target.value); }}
          placeholder="例如:provider/model-name"
        />
      )}
    </div>
  );
}

interface HotkeyFieldProps {
  value: string;
  onChange: (next: string) => void;
}

function HotkeyField({ value, onChange }: HotkeyFieldProps) {
  const [recording, setRecording] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Capture keys ONLY while in recording mode and the field is focused.
  useEffect(() => {
    if (!recording) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      const hk = eventToHotkey(e);
      if (!hk) return; // bare modifier — keep waiting
      onChange(hk);
      setRecording(false);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [recording, onChange]);

  return (
    <div
      ref={ref}
      role="button"
      tabIndex={0}
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          setRecording(true);
        }
      }}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        padding: "6px 10px",
        height: 34,
        borderRadius: "var(--r-md)",
        background: recording ? "var(--accent-soft)" : "var(--bg-elevated)",
        border: `1px solid ${recording ? "var(--border-accent)" : "var(--border-default)"}`,
        boxShadow: recording ? "var(--shadow-glow)" : undefined,
        cursor: "pointer",
        fontSize: 13,
        userSelect: "none",
      }}
    >
      <span style={{ color: recording ? "var(--accent-text)" : "var(--text-primary)", fontFamily: "var(--font-mono)" }}>
        {recording ? "按下你想要的组合键..." : (formatHotkey(value) || "未设置")}
      </span>
      {!recording && (
        <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>点击修改</span>
      )}
      {recording && (
        <span style={{ fontSize: 11, color: "var(--accent-text)" }}>Esc 取消</span>
      )}
    </div>
  );
}

export function SettingsDrawer({ open, onClose }: SettingsDrawerProps) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [original, setOriginal] = useState<AppSettings | null>(null);
  const [cpuInfo, setCpuInfo] = useState<CpuInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [errMsg, setErrMsg] = useState<string | null>(null);
  const [okMsg, setOkMsg] = useState<string | null>(null);
  const [showApiKey, setShowApiKey] = useState(false);
  const [categoriesText, setCategoriesText] = useState("");

  const reload = useCallback(async () => {
    setLoading(true); setErrMsg(null); setOkMsg(null);
    try {
      const [s, cpu] = await Promise.all([getAppSettings(), getCpuInfo()]);
      setSettings(s); setOriginal(s);
      setCpuInfo(cpu);
      setCategoriesText(s.categories.join("\n"));
    } catch (e: any) {
      setErrMsg(typeof e === "string" ? e : e?.message ?? JSON.stringify(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { if (open) void reload(); }, [open, reload]);

  const dirty = useMemo(() => {
    if (!settings || !original) return false;
    if (categoriesText.split(/\r?\n/).map(s => s.trim()).filter(Boolean).join("\n") !== original.categories.join("\n")) return true;
    return JSON.stringify({ ...settings, categories: original.categories }) !== JSON.stringify({ ...original });
  }, [settings, original, categoriesText]);

  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
    setOkMsg(null);
  };

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true); setErrMsg(null); setOkMsg(null);
    const cats = categoriesText.split(/\r?\n/).map(s => s.trim()).filter(Boolean);
    const patch: AppSettings = { ...settings, categories: cats };
    try {
      await updateAppSettings(patch);
      setSettings(patch); setOriginal(patch);
      setOkMsg("已保存");
    } catch (e: any) {
      setErrMsg(typeof e === "string" ? e : e?.message ?? JSON.stringify(e));
    } finally {
      setSaving(false);
    }
  };

  const handleReset = () => {
    if (!original) return;
    setSettings(original);
    setCategoriesText(original.categories.join("\n"));
    setOkMsg(null); setErrMsg(null);
  };

  if (!open) return null;

  return (
    <>
      <div className="drawer-overlay" onClick={onClose} />
      <aside className="settings-drawer">
        <header className="settings-drawer__header">
          <span className="settings-drawer__title">设置</span>
          <button className="settings-drawer__close" onClick={onClose} type="button" aria-label="关闭">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </header>

        <div className="settings-drawer__body" style={{ colorScheme: "light" }}>
          {loading && <div style={{ color: "var(--text-tertiary)", fontSize: 12 }}>加载中...</div>}

          {settings && (
            <>
              {/* AI */}
              <section className="settings-section">
                <h3 className="settings-section__title">AI 模型</h3>

                <div className="field">
                  <label className="field__label">API Key (SiliconFlow)</label>
                  <div style={{ display: "flex", gap: 6 }}>
                    <input
                      className="input"
                      type={showApiKey ? "text" : "password"}
                      value={settings.apiKey}
                      onChange={(e) => update("apiKey", e.target.value)}
                      placeholder="sk-..."
                      style={{ fontFamily: "var(--font-mono)" }}
                    />
                    <button
                      type="button"
                      className="btn btn--subtle btn--sm"
                      onClick={() => setShowApiKey(v => !v)}
                    >
                      {showApiKey ? "隐藏" : "显示"}
                    </button>
                  </div>
                </div>

                <div className="field">
                  <label className="field__label">对话模型</label>
                  <ModelPicker value={settings.chatModel} presets={CHAT_MODELS} onChange={(v) => update("chatModel", v)} />
                </div>

                <div className="field">
                  <label className="field__label">嵌入模型</label>
                  <ModelPicker value={settings.embeddingModel} presets={EMBEDDING_MODELS} onChange={(v) => update("embeddingModel", v)} />
                </div>
              </section>

              {/* 性能 */}
              <section className="settings-section">
                <h3 className="settings-section__title">性能</h3>
                <div className="field">
                  <label className="field__label" style={{ display: "flex", justifyContent: "space-between" }}>
                    <span>并发处理任务数</span>
                    <strong style={{ color: "var(--accent)", fontVariantNumeric: "tabular-nums" }}>
                      {settings.maxConcurrentWorkers}
                    </strong>
                  </label>
                  <input
                    type="range"
                    className="slider"
                    min={1}
                    max={cpuInfo?.logical ?? 8}
                    step={1}
                    value={settings.maxConcurrentWorkers}
                    onChange={(e) => update("maxConcurrentWorkers", parseInt(e.target.value, 10))}
                  />
                  <div className="field__hint">
                    上限 {cpuInfo?.logical ?? "?"} (本机 CPU 逻辑核数);LLM 有限流,推荐 {cpuInfo?.recommended ?? 3}-5
                  </div>
                </div>
              </section>

              {/* 快捷键 */}
              <section className="settings-section">
                <h3 className="settings-section__title">快捷键</h3>
                <div className="field">
                  <label className="field__label">快速搜索</label>
                  <HotkeyField
                    value={settings.searchHotkey}
                    onChange={(v) => update("searchHotkey", v)}
                  />
                  <div className="field__hint">
                    点击后按下你想用的组合键。建议含 Ctrl/Alt 修饰键以避免与文本输入冲突。
                  </div>
                </div>
              </section>

              {/* 路径 */}
              <section className="settings-section">
                <h3 className="settings-section__title">路径</h3>
                <div className="field">
                  <label className="field__label">整理目录</label>
                  <input
                    className="input"
                    type="text"
                    value={settings.targetRoot}
                    onChange={(e) => update("targetRoot", e.target.value)}
                    placeholder="D:\Files"
                  />
                </div>
                <div className="field">
                  <label className="field__label">未分类目录</label>
                  <input
                    className="input"
                    type="text"
                    value={settings.unclassifiedRoot}
                    onChange={(e) => update("unclassifiedRoot", e.target.value)}
                    placeholder="D:\Files\未分类"
                  />
                </div>
              </section>

              {/* 分类 */}
              <section className="settings-section">
                <h3 className="settings-section__title">顶级分类(每行一个)</h3>
                <textarea
                  className="input"
                  value={categoriesText}
                  onChange={(e) => { setCategoriesText(e.target.value); setOkMsg(null); }}
                  rows={6}
                  placeholder="财务&#10;工作&#10;..."
                />
              </section>

              {/* 策略 */}
              <section className="settings-section">
                <h3 className="settings-section__title">策略</h3>
                <div className="field">
                  <label className="field__label" style={{ display: "flex", justifyContent: "space-between" }}>
                    <span>低置信度阈值</span>
                    <strong style={{ color: "var(--accent)", fontVariantNumeric: "tabular-nums" }}>
                      {settings.lowConfidenceThreshold.toFixed(2)}
                    </strong>
                  </label>
                  <input
                    type="range"
                    className="slider"
                    min={0}
                    max={1}
                    step={0.05}
                    value={settings.lowConfidenceThreshold}
                    onChange={(e) => update("lowConfidenceThreshold", parseFloat(e.target.value))}
                  />
                  <div className="field__hint">
                    AI 置信度低于此值的文件会被路由到未分类(若启用了"自动归未分类")
                  </div>
                </div>

                <div className="field" style={{ marginTop: "var(--s-3)" }}>
                  <label className="field__label">顶级分类上限</label>
                  <input
                    className="input"
                    type="number"
                    min={5}
                    max={100}
                    step={1}
                    value={settings.maxTopLevelCategories}
                    onChange={(e) => {
                      const raw = parseInt(e.target.value, 10);
                      if (Number.isNaN(raw)) return;
                      const clamped = Math.max(5, Math.min(100, raw));
                      update("maxTopLevelCategories", clamped);
                    }}
                    onBlur={(e) => {
                      // On blur, re-clamp in case user typed something out of range then tabbed away.
                      const raw = parseInt(e.target.value, 10);
                      const clamped = Number.isNaN(raw) ? 30 : Math.max(5, Math.min(100, raw));
                      if (clamped !== settings.maxTopLevelCategories) {
                        update("maxTopLevelCategories", clamped);
                      }
                    }}
                    style={{ maxWidth: 120 }}
                  />
                  <div className="field__hint">
                    范围 5-100,推荐 20-50。顶级分类数低于此值时 AI 会按需新建;达到上限后只能从已有里选
                  </div>
                </div>
              </section>

              {errMsg && (
                <div style={{ padding: "var(--s-3)", background: "var(--red-soft)", color: "var(--red)", borderRadius: "var(--r-md)", fontSize: 12, wordBreak: "break-all", marginBottom: "var(--s-3)" }}>
                  {errMsg}
                </div>
              )}
              {okMsg && (
                <div style={{ padding: "var(--s-3)", background: "var(--green-soft)", color: "var(--green)", borderRadius: "var(--r-md)", fontSize: 12, marginBottom: "var(--s-3)" }}>
                  {okMsg}
                </div>
              )}

              <div className="settings-actions">
                <button
                  type="button"
                  className="btn btn--ghost"
                  onClick={handleReset}
                  disabled={!dirty || saving}
                >
                  撤销修改
                </button>
                <button
                  type="button"
                  className="btn btn--primary"
                  onClick={handleSave}
                  disabled={!dirty || saving}
                >
                  {saving ? "保存中..." : "保存"}
                </button>
              </div>
            </>
          )}
        </div>
      </aside>
    </>
  );
}
