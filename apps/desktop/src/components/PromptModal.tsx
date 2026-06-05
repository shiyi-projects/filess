import { useEffect, useRef, useState } from "react";

interface PromptModalProps {
  open: boolean;
  title: string;
  /** Shown above the input. Can be plain text or rich JSX. */
  description?: React.ReactNode;
  placeholder?: string;
  /** Initial value prefilled in the input. */
  initialValue?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Preset chips shown under the input — clicking fills the value. */
  suggestions?: string[];
  onConfirm: (value: string) => void;
  onCancel: () => void;
}

export function PromptModal({
  open,
  title,
  description,
  placeholder,
  initialValue = "",
  confirmLabel = "确定",
  cancelLabel = "取消",
  suggestions,
  onConfirm,
  onCancel,
}: PromptModalProps) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setValue(initialValue);
      // Defer focus so the element is mounted
      const t = window.setTimeout(() => inputRef.current?.focus(), 60);
      return () => window.clearTimeout(t);
    }
  }, [open, initialValue]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div
        className="review-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <header className="review-modal__header">
          <span className="review-modal__title">{title}</span>
          <button
            type="button"
            className="review-modal__close"
            onClick={onCancel}
            aria-label="关闭"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </header>

        <div className="review-modal__body">
          {description && (
            <div
              style={{
                fontSize: 13,
                color: "var(--text-secondary)",
                lineHeight: 1.55,
              }}
            >
              {description}
            </div>
          )}

          <input
            ref={inputRef}
            className="input"
            type="text"
            value={value}
            placeholder={placeholder}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onConfirm(value);
              }
            }}
          />

          {suggestions && suggestions.length > 0 && (
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              {suggestions.map((s) => (
                <button
                  key={s}
                  type="button"
                  onClick={() => setValue(s)}
                  style={{
                    padding: "4px 10px",
                    fontSize: 11,
                    borderRadius: "var(--r-full)",
                    background: "var(--accent-soft)",
                    color: "var(--accent-text)",
                    border: "1px solid transparent",
                    cursor: "pointer",
                    transition: "all var(--duration-fast) var(--ease)",
                  }}
                  onMouseEnter={(e) => (e.currentTarget.style.borderColor = "var(--border-accent)")}
                  onMouseLeave={(e) => (e.currentTarget.style.borderColor = "transparent")}
                >
                  {s}
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="review-modal__footer">
          <button type="button" className="btn btn--ghost" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className="btn btn--primary"
            style={{ marginLeft: "auto" }}
            onClick={() => onConfirm(value)}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
