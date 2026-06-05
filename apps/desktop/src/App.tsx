import { useEffect, useRef, useState, useCallback } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { IconRail } from "./components/IconRail";
import { HeroDropZone } from "./components/HeroDropZone";
import { ProcessingCard } from "./components/ProcessingCard";
import { QuickConfigBar } from "./components/QuickConfigBar";
import { ResultList } from "./components/ResultList";
import { StatsBar } from "./components/StatsBar";
import { CommandPalette } from "./components/CommandPalette";
import { ReviewModal } from "./components/ReviewModal";
import { SettingsDrawer } from "./components/SettingsDrawer";
import { Sidebar } from "./components/Sidebar";
import { PromptModal } from "./components/PromptModal";
import {
  loadBootstrapState,
  searchFiles,
  openFile,
  revealInFolder,
  copyPath,
  enqueueItems,
  undoOperation,
  reclassifyItem,
  getAppSettings,
} from "./lib/tauri";
import { matchesHotkey } from "./lib/hotkey";
import type {
  AppBootstrapState,
  SearchResults,
  TaskSummary,
  ReviewRequest,
  EnqueueResult,
} from "./lib/types";

export function App() {
  const [state, setState] = useState<AppBootstrapState | null>(null);
  const [loading, setLoading] = useState(true);
  const [dragActive, setDragActive] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [infoMsg, setInfoMsg] = useState<string | null>(null);

  // Panel states
  const [searchOpen, setSearchOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [currentReview, setCurrentReview] = useState<ReviewRequest | null>(null);
  const [reclassifyTarget, setReclassifyTarget] = useState<string | null>(null);

  // Refresh state from backend
  const refreshState = useCallback(async () => {
    try {
      const data = await loadBootstrapState();
      setState(data);
    } catch {
      // ignore
    }
  }, []);

  // Load bootstrap state on mount
  useEffect(() => {
    loadBootstrapState()
      .then((data) => {
        setState(data);
        setLoading(false);
      })
      .catch(() => {
        setState({
          batches: [],
          searchResults: [],
          settings: { paths: {}, classificationRules: {}, ai: {}, organizePolicy: {}, dataAndLogs: {} },
          recentResults: [],
        });
        setLoading(false);
      });
  }, []);

  // Dedupe ref for native drop + fallback channel
  const lastDropRef = useRef<{ at: number; first: string }>({ at: 0, first: "" });

  const handleDroppedPaths = useCallback(
    async (paths: string[], source: string) => {
      if (!Array.isArray(paths) || paths.length === 0) return;
      const now = Date.now();
      const first = paths[0] ?? "";
      if (now - lastDropRef.current.at < 500 && lastDropRef.current.first === first) {
        if (import.meta.env.DEV) console.debug("[drag] dedup skip from", source);
        return;
      }
      lastDropRef.current = { at: now, first };
      if (import.meta.env.DEV) console.info("[drag] enqueue from", source, "count=", paths.length);
      setErrorMsg(null);
      try {
        // enqueueItems is fire-and-forget on the Rust side: it returns an
        // empty placeholder immediately and the real outcome (queued count +
        // skipped duplicates) arrives later via the `enqueue-result` event,
        // handled in the effect below. Don't inspect the return value here.
        await enqueueItems(paths);
        // Always refresh: even when everything was a duplicate, the back-end
        // bumped those records to the top of the recent-results cache so the
        // user can see "ah, that one is already at <category>".
        await refreshState();
      } catch (err: any) {
        const msg = typeof err === "string" ? err : err?.message ?? JSON.stringify(err);
        setErrorMsg(msg);
        console.error("enqueue failed:", err);
        await refreshState();
      }
    },
    [refreshState]
  );

  // ─── Tauri drag-drop events via onDragDropEvent() ───
  useEffect(() => {
    const pending: Promise<UnlistenFn>[] = [];

    const nativeUn = getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (import.meta.env.DEV) {
        const count =
          p.type === "enter" || p.type === "drop" ? p.paths?.length ?? 0 : 0;
        console.debug("[drag]", p.type, "paths=", count);
      }
      switch (p.type) {
        case "enter":
          setDragActive(true);
          break;
        case "leave":
          setDragActive(false);
          break;
        case "drop":
          setDragActive(false);
          void handleDroppedPaths(p.paths ?? [], "native");
          break;
        case "over":
        default:
          break;
      }
    });
    pending.push(nativeUn);

    // Secondary channel emitted from Rust on_window_event hook
    const fallbackUn = listen<string[]>("drag-drop-fallback", (event) => {
      if (import.meta.env.DEV) console.debug("[drag] fallback event received");
      setDragActive(false);
      void handleDroppedPaths(event.payload ?? [], "fallback");
    });
    pending.push(fallbackUn);

    // Batch progress events from the Rust background worker
    const batchUn = listen<string>("batch-updated", () => {
      if (import.meta.env.DEV) console.debug("[batch] updated → refreshState");
      void refreshState();
    });
    pending.push(batchUn);

    // Real enqueue outcome — the enqueue_items command is fire-and-forget and
    // returns an empty placeholder, so the skipped-duplicate list is delivered
    // here instead. Surface it to the user via the info banner.
    const enqueueUn = listen<EnqueueResult>("enqueue-result", (event) => {
      const skipped = event.payload?.skipped ?? [];
      if (skipped.length > 0) {
        if (import.meta.env.DEV) {
          console.info(
            `[enqueue] skipped ${skipped.length} duplicate(s) — already in library`,
            skipped.map((s) => `${s.sourcePath} → ${s.existingPath}`)
          );
        }
        setInfoMsg(
          `已跳过 ${skipped.length} 个重复文件（已在库中），并将其置顶到最近结果。`
        );
      }
    });
    pending.push(enqueueUn);

    return () => {
      Promise.all(pending)
        .then((fns) => fns.forEach((fn) => fn()))
        .catch(() => {
          /* if promise rejected, nothing to unlisten */
        });
    };
  }, [handleDroppedPaths, refreshState]);

  // ─── Browse fallback: open system file picker ───
  const handleBrowse = useCallback(async () => {
    try {
      const selected = await openDialog({ multiple: true, directory: false });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      await handleDroppedPaths(paths, "dialog");
    } catch (err: any) {
      const msg = typeof err === "string" ? err : err?.message ?? JSON.stringify(err);
      setErrorMsg(msg);
      console.error("openDialog failed:", err);
    }
  }, [handleDroppedPaths]);

  // Search hotkey — value comes from app settings, refetched whenever the
  // user saves new settings (we listen for the `settings-updated` event).
  const [searchHotkey, setSearchHotkey] = useState<string>("Ctrl+K");
  useEffect(() => {
    let cancelled = false;
    getAppSettings()
      .then((s) => { if (!cancelled) setSearchHotkey(s.searchHotkey || "Ctrl+K"); })
      .catch(() => {});
    const un = listen<{ searchHotkey?: string }>("settings-updated", (ev) => {
      const sh = ev.payload?.searchHotkey;
      if (typeof sh === "string" && sh.trim()) setSearchHotkey(sh);
    });
    return () => {
      cancelled = true;
      un.then((fn) => fn()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (matchesHotkey(e, searchHotkey)) {
        e.preventDefault();
        setSearchOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [searchHotkey]);

  // Derived data
  const allTasks: TaskSummary[] =
    state?.batches.flatMap((b) => b.tasks) ?? [];
  const activeStatuses = new Set([
    "sniffing", "parsing", "retrieving_context", "calling_model", "executing",
  ]);
  const processingTasks = allTasks.filter((t) => activeStatuses.has(t.status));
  const pendingReviewCount = allTasks.filter(
    (t) => t.status === "awaiting_review"
  ).length;
  const queuedCount = allTasks.filter((t) => t.status === "queued").length;
  const completedCount = allTasks.filter((t) => t.status === "completed").length;

  // Handlers
  const handleSearch = useCallback(
    async (query: string): Promise<SearchResults> => {
      return searchFiles(query);
    },
    []
  );

  const handleCopyPath = useCallback(async (itemId: string) => {
    const path = await copyPath(itemId);
    try {
      await navigator.clipboard.writeText(path);
    } catch (err) {
      // Fallback for non-secure contexts or missing permission
      const ta = document.createElement("textarea");
      ta.value = path;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      try {
        document.execCommand("copy");
      } finally {
        document.body.removeChild(ta);
      }
      console.warn("clipboard API failed, used execCommand fallback:", err);
    }
  }, []);

  const handleUndo = useCallback(
    async (itemId: string) => {
      if (!window.confirm("确认撤销?将把文件移回原位置")) return;
      // Capture the file name *before* the server call, because by the time
      // we know it's a stale_cleanup the record is already gone from state.
      const target = state?.recentResults.find((r) => r.itemId === itemId);
      const name = target?.fileName ?? "该文件";
      try {
        const outcome = await undoOperation(itemId);
        if (outcome === "stale_cleanup") {
          setInfoMsg(
            `「${name}」已不在记录位置(被手动移动或删除),已从结果列表中移除该记录`
          );
        }
        await refreshState();
      } catch (err: any) {
        const msg = typeof err === "string" ? err : err?.message ?? JSON.stringify(err);
        setErrorMsg(msg);
        console.error("undoOperation failed:", err);
      }
    },
    [refreshState, state]
  );

  // Clicking the reclassify button opens a PromptModal — the actual RPC
  // happens in `runReclassify` once the user confirms with (or without) a hint.
  const handleReclassify = useCallback((itemId: string) => {
    setReclassifyTarget(itemId);
  }, []);

  const runReclassify = useCallback(
    async (hint: string) => {
      const itemId = reclassifyTarget;
      setReclassifyTarget(null);
      if (!itemId) return;
      try {
        setErrorMsg(null);
        await reclassifyItem(itemId, hint.trim() || undefined);
        await refreshState();
      } catch (err: any) {
        const msg = typeof err === "string" ? err : err?.message ?? JSON.stringify(err);
        setErrorMsg(msg);
        console.error("reclassifyItem failed:", err);
      }
    },
    [reclassifyTarget, refreshState]
  );

  const handleReviewApprove = useCallback(
    async (_taskId: string, _categoryId: string, _newName: string) => {
      setCurrentReview(null);
    },
    []
  );

  const handleReviewReroute = useCallback(async (_taskId: string) => {
    setCurrentReview(null);
  }, []);

  const handleReviewSkip = useCallback(async (_taskId: string) => {
    setCurrentReview(null);
  }, []);

  const handleReviewClick = useCallback((_taskId: string) => {
    setSidebarOpen(false);
  }, []);

  const handleBrowseAllClick = useCallback(() => {
    setSidebarOpen(true);
  }, []);

  // Loading screen
  if (loading) {
    return (
      <div className="loading-screen">
        <div className="loading-screen__spinner" />
        <div className="loading-screen__text">正在初始化工作台...</div>
      </div>
    );
  }

  return (
    <main className="app-shell">
      <IconRail
        onSearchClick={() => setSearchOpen(true)}
        onSettingsClick={() => setSettingsOpen(true)}
        pendingReviewCount={pendingReviewCount}
        sidebarOpen={sidebarOpen}
        onToggleSidebar={() => setSidebarOpen((v) => !v)}
      />

      <div className="main-content">
        <div className="main-scroll">
          <QuickConfigBar />

          <HeroDropZone
            dragActive={dragActive}
            onBrowseClick={handleBrowse}
          />

          <ProcessingCard batches={state?.batches ?? []} />

          {/* Error banner */}
          {errorMsg && (
            <div style={{
              padding: "var(--s-3) var(--s-4)",
              borderRadius: "var(--r-md)",
              background: "var(--red-soft)",
              color: "var(--red)",
              fontSize: 12,
              marginBottom: "var(--s-3)",
              wordBreak: "break-all",
            }}>
              {errorMsg}
              <button
                onClick={() => setErrorMsg(null)}
                style={{ float: "right", color: "var(--red)", fontWeight: 600 }}
              >
                ✕
              </button>
            </div>
          )}

          {/* Info banner — warnings / non-fatal notices */}
          {infoMsg && (
            <div style={{
              padding: "var(--s-3) var(--s-4)",
              borderRadius: "var(--r-md)",
              background: "var(--amber-soft)",
              color: "var(--amber)",
              fontSize: 12,
              marginBottom: "var(--s-4)",
              wordBreak: "break-all",
            }}>
              {infoMsg}
              <button
                onClick={() => setInfoMsg(null)}
                style={{ float: "right", color: "var(--amber)", fontWeight: 600 }}
              >
                ✕
              </button>
            </div>
          )}

          <ResultList
            results={state?.recentResults ?? []}
            onOpenFile={openFile}
            onRevealInFolder={revealInFolder}
            onCopyPath={handleCopyPath}
            onUndo={handleUndo}
            onReclassify={handleReclassify}
          />
        </div>

        <StatsBar
          todayCount={state?.recentResults.length ?? 0}
          pendingCount={pendingReviewCount}
          queuedCount={queuedCount}
          totalManaged={state?.recentResults.length ?? 0}
          sidecarOnline={true}
          onBrowseClick={handleBrowseAllClick}
        />
      </div>

      <Sidebar
        open={sidebarOpen}
        onClose={() => setSidebarOpen(false)}
        tasks={[
          ...allTasks.filter((t) => t.status === "queued"),
          ...processingTasks,
        ]}
        refreshKey={completedCount}
        onOpenFile={openFile}
        onRevealInFolder={revealInFolder}
        onCopyPath={handleCopyPath}
        onReviewClick={handleReviewClick}
      />

      <CommandPalette
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSearch={handleSearch}
        onOpenFile={(id) => { openFile(id); setSearchOpen(false); }}
        onRevealInFolder={(id) => { revealInFolder(id); setSearchOpen(false); }}
        onCopyPath={handleCopyPath}
      />

      <ReviewModal
        review={currentReview}
        onApprove={handleReviewApprove}
        onReroute={handleReviewReroute}
        onSkip={handleReviewSkip}
        onClose={() => setCurrentReview(null)}
      />

      <SettingsDrawer
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />

      {(() => {
        const target = reclassifyTarget
          ? state?.recentResults.find((r) => r.itemId === reclassifyTarget)
          : null;
        const description = target ? (
          <>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 6,
                padding: "var(--s-3)",
                background: "var(--bg-elevated)",
                border: "1px solid var(--border-subtle)",
                borderRadius: "var(--r-md)",
                marginBottom: "var(--s-3)",
              }}
            >
              <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
                <span style={{ fontSize: 11, color: "var(--text-tertiary)", flexShrink: 0, minWidth: 56 }}>
                  文件
                </span>
                <span style={{ fontSize: 13, color: "var(--text-primary)", fontWeight: 500, wordBreak: "break-all" }}>
                  {target.fileName}
                </span>
              </div>
              <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
                <span style={{ fontSize: 11, color: "var(--text-tertiary)", flexShrink: 0, minWidth: 56 }}>
                  当前分类
                </span>
                <span className="tag tag--accent">{target.categoryName}</span>
              </div>
            </div>
            <p style={{ margin: 0 }}>
              可选:给 AI 一句话提示,帮它改正。留空也可以直接重试。
            </p>
          </>
        ) : (
          "可选:给 AI 一句话提示,帮它改正。"
        );
        return (
          <PromptModal
            open={reclassifyTarget !== null}
            title="重新分类"
            description={description}
            placeholder="例如:这是工作文件 / 归到 财务/报销"
            confirmLabel="重新分类"
            suggestions={[
              "这是工作文件",
              "这是学习资料",
              "这是同事/客户的交付物",
              "归到 财务/报销",
            ]}
            onConfirm={runReclassify}
            onCancel={() => setReclassifyTarget(null)}
          />
        );
      })()}
    </main>
  );
}
