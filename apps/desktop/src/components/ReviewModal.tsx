import { useState } from "react";
import type { ReviewRequest } from "../lib/types";

interface ReviewModalProps {
  review: ReviewRequest | null;
  onApprove: (taskId: string, categoryId: string, newName: string) => void;
  onReroute: (taskId: string) => void;
  onSkip: (taskId: string) => void;
  onClose: () => void;
}

export function ReviewModal({ review, onApprove, onReroute, onSkip, onClose }: ReviewModalProps) {
  const [editName, setEditName] = useState("");

  if (!review) return null;

  const pct = Math.round(review.confidence * 100);
  const tier = pct >= 80 ? "confidence--high" : pct >= 50 ? "confidence--mid" : "confidence--low";

  if (editName === "" && review.suggestedName) {
    setTimeout(() => setEditName(review.suggestedName), 0);
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="review-modal" onClick={e => e.stopPropagation()} role="dialog" aria-label="人工确认">
        <header className="review-modal__header">
          <span className="review-modal__title">确认整理</span>
          <button className="review-modal__close" onClick={onClose} type="button">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </header>
        <div className="review-modal__body">
          <div className="review-file-info">
            <div className="review-file-info__row">
              <span className="review-file-info__label">文件</span>
              <span className="review-file-info__value">{review.fileName}</span>
            </div>
            <div className="review-file-info__row">
              <span className="review-file-info__label">来源</span>
              <span className="review-file-info__value">{review.sourcePath}</span>
            </div>
          </div>
          <div className="review-suggestion">
            <div className="review-suggestion__header">
              <span className="review-suggestion__title">AI 建议</span>
              <div className={`confidence-bar ${tier}`}>
                <div className="confidence-bar__track"><div className="confidence-bar__fill" style={{ width: `${pct}%` }} /></div>
                <span className="confidence-bar__label">{pct}%</span>
              </div>
            </div>
            <div className="review-suggestion__row">分类 <strong>{review.suggestedCategoryName}</strong></div>
            <div className="review-suggestion__row">{review.reason}</div>
          </div>
          <div className="review-field">
            <label className="review-field__label">文件名</label>
            <input className="review-field__input" value={editName} onChange={e => setEditName(e.target.value)} />
          </div>
        </div>
        <footer className="review-modal__footer">
          <button className="review-btn review-btn--primary" onClick={() => onApprove(review.taskId, review.suggestedCategoryId, editName || review.suggestedName)} type="button">确认</button>
          <button className="review-btn review-btn--outline" onClick={() => onReroute(review.taskId)} type="button">改投未分类</button>
          <button className="review-btn review-btn--ghost" onClick={() => onSkip(review.taskId)} type="button">跳过</button>
        </footer>
      </div>
    </div>
  );
}
