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

// --- SPHIN-2: media analysis -------------------------------------------------

export interface Project {
  id: number;
  name: string;
  created_at: string;
  updated_at: string;
}

export type ProviderKind = "openai" | "gemini" | "anthropic";

export interface AnalysisConfig {
  provider: ProviderKind;
  api_key: string;
  /** Empty string => provider default model. */
  model: string;
  /** Empty string => provider default base URL. */
  base_url: string;
  /** Extra guidance appended to the analysis prompt for this project. */
  prompt_extra: string;
  timeout_secs: number;
}

export const DEFAULT_ANALYSIS_CONFIG: AnalysisConfig = {
  provider: "openai",
  api_key: "",
  model: "",
  base_url: "",
  prompt_extra: "",
  timeout_secs: 90,
};

/** The structured content description a vision model returns (SPHIN-16). */
export interface AnalysisResult {
  description: string;
  subjects: string[];
  scene: string;
  mood: string;
  colors: string[];
  keywords: string[];
  editorial: boolean;
  text_content: string;
  model: string;
  provider: string;
}

export interface AnalysisResponse {
  record_id: number;
  result: AnalysisResult;
}

export function listProjects(): Promise<Project[]> {
  return invoke("list_projects");
}

export function createProject(name: string): Promise<Project> {
  return invoke("create_project", { name });
}

export function getAnalysisConfig(projectId: number): Promise<AnalysisConfig | null> {
  return invoke("get_analysis_config", { projectId });
}

export function setAnalysisConfig(
  projectId: number,
  config: AnalysisConfig
): Promise<void> {
  return invoke("set_analysis_config", { projectId, config });
}

export function getAnalysis(assetId: number): Promise<AnalysisResponse | null> {
  return invoke("get_analysis", { assetId });
}

export function analyzeAsset(
  projectId: number,
  assetId: number
): Promise<AnalysisResponse> {
  return invoke("analyze_asset", { projectId, assetId });
}

// --- SPHIN-3: metadata generation & limiter profiles -------------------------

/** Length/count limits enforced when generating metadata for a stock site. */
export interface LimiterProfile {
  name: string;
  max_title_chars: number;
  max_description_chars: number;
  min_keywords: number;
  max_keywords: number;
  max_keyword_chars: number;
}

export interface GeneratedMetadata {
  title: string;
  description: string;
  keywords: string[];
  profile: string;
  meets_minimum_keywords: boolean;
}

export interface MetadataResponse {
  record_id: number;
  result: GeneratedMetadata;
}

export function listLimiterProfiles(): Promise<LimiterProfile[]> {
  return invoke("list_limiter_profiles");
}

export function getLimiterProfile(projectId: number): Promise<LimiterProfile | null> {
  return invoke("get_limiter_profile", { projectId });
}

export function setLimiterProfile(
  projectId: number,
  profile: LimiterProfile
): Promise<void> {
  return invoke("set_limiter_profile", { projectId, profile });
}

export function getMetadata(assetId: number): Promise<MetadataResponse | null> {
  return invoke("get_metadata", { assetId });
}

export function generateMetadata(
  projectId: number,
  assetId: number
): Promise<MetadataResponse> {
  return invoke("generate_metadata", { projectId, assetId });
}
