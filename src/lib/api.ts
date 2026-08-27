// Thin wrapper around the Tauri commands exposed by src-tauri/src/lib.rs.
// Keeping all `invoke` calls in one place means the rest of the UI never
// touches Tauri's IPC layer directly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MediaType = "image" | "video";

export interface Asset {
  id: number;
  path: string;
  hash: string;
  size: number;
  media_type: MediaType;
  status: string;
  created_at: string;
  updated_at: string;
}

export type IngestOutcome =
  | { outcome: "ingested"; asset: Asset }
  | { outcome: "duplicate"; existing: Asset; path: string }
  | { outcome: "skipped"; path: string; reason: string };

export interface IngestSummary {
  ingested: number;
  duplicates: number;
  skipped: number;
  errors: number;
  outcomes: IngestOutcome[];
}

export function ingestFiles(paths: string[]): Promise<IngestSummary> {
  return invoke("ingest_files", { paths });
}

export function ingestFolder(dir: string): Promise<IngestSummary> {
  return invoke("ingest_folder", { dir });
}

export function startWatch(dir: string): Promise<void> {
  return invoke("start_watch", { dir });
}

export function stopWatch(): Promise<void> {
  return invoke("stop_watch");
}

export function listAssets(limit = 100, offset = 0): Promise<Asset[]> {
  return invoke("list_assets", { limit, offset });
}

export function assetCount(): Promise<number> {
  return invoke("asset_count");
}

/** Fires whenever folder-watch mode ingests one or more new files. */
export function onAssetsIngested(
  handler: (summary: IngestSummary) => void
): Promise<UnlistenFn> {
  return listen<IngestSummary>("assets-ingested", (event) => handler(event.payload));
}
