export type TaskStatus =
  | "queued"
  | "sniffing"
  | "parsing"
  | "retrieving_context"
  | "calling_model"
  | "awaiting_review"
  | "executing"
  | "completed"
  | "failed"
  | "rolled_back"
  | "skipped";

export interface TaskSummary {
  taskId: string;
  sourcePath: string;
  status: TaskStatus;
  errorMessage: string | null;
}

export interface BatchSummary {
  batchId: string;
  status: string;
  total: number;
  completed: number;
  failed: number;
  awaitingReview: number;
  tasks: TaskSummary[];
}

export interface SearchResultItem {
  itemId: string;
  title: string;
  categoryId: string;
  categoryName: string;
  currentPath: string;
  summaryExcerpt: string | null;
  matchedFields: string[];
  lastProcessedAt: string;
  score: number | null;
}

export interface SearchResults {
  semantic: SearchResultItem[];
  filename: SearchResultItem[];
}

/** A completed result displayed on the main screen */
export interface ResultItem {
  itemId: string;
  fileName: string;
  categoryName: string;
  currentPath: string;
  itemType: "file" | "folder";
  processedAt: string;
  operationId?: string;
}

/** Review data sent to the user for confirmation */
export interface ReviewRequest {
  taskId: string;
  fileName: string;
  sourcePath: string;
  suggestedCategoryId: string;
  suggestedCategoryName: string;
  suggestedName: string;
  confidence: number;
  reason: string;
}

export interface SettingsSnapshot {
  paths: Record<string, string>;
  classificationRules: Record<string, unknown>;
  ai: Record<string, unknown>;
  organizePolicy: Record<string, unknown>;
  dataAndLogs: Record<string, unknown>;
}

export interface AppBootstrapState {
  batches: BatchSummary[];
  searchResults: SearchResultItem[];
  settings: SettingsSnapshot;
  recentResults: ResultItem[];
}

export interface SkippedItem {
  sourcePath: string;
  existingPath: string;
  existingId: string;
  categoryName: string;
}

export interface EnqueueResult {
  batchId: string | null;
  queuedCount: number;
  skipped: SkippedItem[];
}

export type FileOperationMode = "move" | "copy";
export type SourceDisposition = "recycle_bin" | "delete";

export interface OrganizePolicy {
  fileOperationMode: FileOperationMode;
  sourceDisposition: SourceDisposition;
  autoUnclassifyLowConfidence: boolean;
}

/** Full editable app configuration shown in SettingsDrawer. */
export interface AppSettings {
  apiKey: string;
  chatModel: string;
  embeddingModel: string;
  targetRoot: string;
  unclassifiedRoot: string;
  lowConfidenceThreshold: number;
  categories: string[];
  fileOperationMode: FileOperationMode;
  sourceDisposition: SourceDisposition;
  autoUnclassifyLowConfidence: boolean;
  maxConcurrentWorkers: number;
  searchHotkey: string;
  maxTopLevelCategories: number;
}

export interface CpuInfo {
  logical: number;
  recommended: number;
}
