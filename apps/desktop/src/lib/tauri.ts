import { invoke } from "@tauri-apps/api/core";
import type {
  AppBootstrapState,
  AppSettings,
  BatchSummary,
  CpuInfo,
  EnqueueResult,
  OrganizePolicy,
  ResultItem,
  SearchResultItem,
  SearchResults,
  SettingsSnapshot
} from "./types";

export async function loadBootstrapState(): Promise<AppBootstrapState> {
  const raw = await invoke<Record<string, unknown>>("bootstrap_app_state");
  return {
    batches: (raw.batches ?? []) as AppBootstrapState["batches"],
    searchResults: (raw.searchResults ?? []) as AppBootstrapState["searchResults"],
    settings: (raw.settings ?? {
      paths: {},
      classificationRules: {},
      ai: {},
      organizePolicy: {},
      dataAndLogs: {},
    }) as AppBootstrapState["settings"],
    recentResults: (raw.recentResults ?? []) as AppBootstrapState["recentResults"],
  };
}

export async function searchFiles(query: string): Promise<SearchResults> {
  return invoke<SearchResults>("search_files", {
    payload: { query, filters: null, limit: 20, offset: 0 }
  });
}

export async function searchByFilename(query: string): Promise<SearchResultItem[]> {
  return invoke<SearchResultItem[]>("search_by_filename", {
    payload: { query, filters: null, limit: 20, offset: 0 }
  });
}

export async function searchSemantic(query: string): Promise<SearchResultItem[]> {
  return invoke<SearchResultItem[]>("search_semantic", {
    payload: { query, filters: null, limit: 20, offset: 0 }
  });
}

export async function listAllItems(): Promise<ResultItem[]> {
  return invoke<ResultItem[]>("list_all_items");
}

export async function getBatchStatus(batchId: string): Promise<BatchSummary> {
  return invoke<BatchSummary>("get_batch_status", { batchId });
}

export async function getSettings(): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("get_settings");
}

export async function openFile(itemId: string): Promise<void> {
  return invoke("open_file", { itemId });
}

export async function revealInFolder(itemId: string): Promise<void> {
  return invoke("reveal_in_folder", { itemId });
}

export async function copyPath(itemId: string): Promise<string> {
  return invoke<string>("copy_path", { itemId });
}

export async function enqueueItems(paths: string[]): Promise<EnqueueResult> {
  return invoke<EnqueueResult>("enqueue_items", { payload: { paths } });
}

export async function getOrganizePolicy(): Promise<OrganizePolicy> {
  return invoke<OrganizePolicy>("get_organize_policy");
}

export async function updateSettings(patch: OrganizePolicy): Promise<void> {
  return invoke<void>("update_settings", { patch });
}

export type UndoOutcome = "restored" | "stale_cleanup";

export async function undoOperation(itemId: string): Promise<UndoOutcome> {
  return invoke<UndoOutcome>("undo_operation", { itemId });
}

export async function reclassifyItem(itemId: string, hint?: string): Promise<EnqueueResult> {
  return invoke<EnqueueResult>("reclassify_item", { itemId, hint: hint ?? null });
}

export async function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_app_settings");
}

export async function updateAppSettings(patch: AppSettings): Promise<void> {
  return invoke<void>("update_app_settings", { patch });
}

export async function getCpuInfo(): Promise<CpuInfo> {
  return invoke<CpuInfo>("get_cpu_info");
}
