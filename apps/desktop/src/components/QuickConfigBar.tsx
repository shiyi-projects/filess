import { useCallback, useEffect, useState } from "react";
import type {
  FileOperationMode,
  OrganizePolicy,
  SourceDisposition,
} from "../lib/types";
import { getOrganizePolicy, updateSettings } from "../lib/tauri";

const DEFAULT_POLICY: OrganizePolicy = {
  fileOperationMode: "move",
  sourceDisposition: "recycle_bin",
  autoUnclassifyLowConfidence: true,
};

interface SegmentedProps<T extends string> {
  value: T;
  options: Array<{ value: T; label: string; hint?: string }>;
  onChange: (value: T) => void;
  disabled?: boolean;
}

function Segmented<T extends string>({ value, options, onChange, disabled }: SegmentedProps<T>) {
  return (
    <div className="segmented" role="radiogroup" data-disabled={disabled || undefined}>
      {options.map((opt) => {
        const active = value === opt.value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={disabled}
            title={opt.hint}
            className="segmented__btn"
            onClick={() => !disabled && onChange(opt.value)}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

const labelStyle: React.CSSProperties = {
  fontSize: 11,
  color: "var(--text-tertiary)",
  fontWeight: 500,
  letterSpacing: 0.02,
};

export function QuickConfigBar() {
  const [policy, setPolicy] = useState<OrganizePolicy>(DEFAULT_POLICY);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    getOrganizePolicy()
      .then((p) => { setPolicy(p); setLoaded(true); })
      .catch((err) => { console.error("getOrganizePolicy failed:", err); setLoaded(true); });
  }, []);

  const apply = useCallback(async (next: OrganizePolicy) => {
    setPolicy(next);
    try { await updateSettings(next); }
    catch (err) { console.error("updateSettings failed:", err); }
  }, []);

  const setMode = (m: FileOperationMode) => apply({ ...policy, fileOperationMode: m });
  const setDisposition = (d: SourceDisposition) => apply({ ...policy, sourceDisposition: d });
  const toggleAutoUnclassify = (v: boolean) => apply({ ...policy, autoUnclassifyLowConfidence: v });

  if (!loaded) return null;
  const copyMode = policy.fileOperationMode === "copy";

  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        alignItems: "center",
        gap: "var(--s-4)",
        padding: "var(--s-3) var(--s-4)",
        marginBottom: "var(--s-4)",
        background: "var(--bg-elevated)",
        border: "1px solid var(--border-subtle)",
        borderRadius: "var(--r-md)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={labelStyle}>整理</span>
        <Segmented<FileOperationMode>
          value={policy.fileOperationMode}
          onChange={setMode}
          options={[
            { value: "move", label: "移动", hint: "文件从源位置移走" },
            { value: "copy", label: "复制", hint: "保留源文件作为备份" },
          ]}
        />
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={labelStyle}>源文件</span>
        <Segmented<SourceDisposition>
          value={policy.sourceDisposition}
          onChange={setDisposition}
          disabled={copyMode}
          options={[
            { value: "recycle_bin", label: "回收站", hint: "可在系统回收站恢复" },
            { value: "delete", label: "永久删除", hint: "无法恢复" },
          ]}
        />
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={labelStyle}>低置信度</span>
        <Segmented<"yes" | "no">
          value={policy.autoUnclassifyLowConfidence ? "yes" : "no"}
          onChange={(v) => toggleAutoUnclassify(v === "yes")}
          options={[
            { value: "yes", label: "自动归未分类", hint: "AI 不确定时直接放未分类" },
            { value: "no", label: "等待审阅", hint: "AI 不确定时停下等用户" },
          ]}
        />
      </div>
    </div>
  );
}
