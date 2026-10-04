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

/** Remove an asset from the library. The original file on disk is untouched. */
export function deleteAsset(assetId: number): Promise<void> {
  return invoke("delete_asset", { assetId });
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

export type ProviderKind = "openai" | "gemini" | "anthropic" | "ollama";

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

/** Every provider config this project has saved, keyed by provider name. */
export function getAnalysisConfigs(
  projectId: number
): Promise<Record<string, AnalysisConfig>> {
  return invoke("get_analysis_configs", { projectId });
}

/** Resolves to a summary of models pulled on the local Ollama server, or rejects if it isn't reachable. */
export function checkOllama(projectId: number): Promise<string> {
  return invoke("check_ollama", { projectId });
}

/** Best-effort local GPU capability check (SPHIN-36); never rejects. */
export interface GpuInfo {
  available: boolean;
  backend: string;
  detail: string;
}

export function detectGpu(): Promise<GpuInfo> {
  return invoke("detect_gpu");
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

// --- SPHIN-8: video pipeline (keyframes + optional transcription) -----------

export interface VideoConfig {
  /** Empty string => look up "ffmpeg" on PATH. */
  ffmpeg_path: string;
  max_keyframes: number;
  scene_threshold: number;
}

export const DEFAULT_VIDEO_CONFIG: VideoConfig = {
  ffmpeg_path: "",
  max_keyframes: 6,
  scene_threshold: 0.4,
};

export interface TranscriptionConfig {
  enabled: boolean;
  api_key: string;
  /** Empty string => https://api.openai.com/v1. */
  base_url: string;
  model: string;
}

export const DEFAULT_TRANSCRIPTION_CONFIG: TranscriptionConfig = {
  enabled: false,
  api_key: "",
  base_url: "",
  model: "whisper-1",
};

export function getVideoConfig(projectId: number): Promise<VideoConfig | null> {
  return invoke("get_video_config", { projectId });
}

export function setVideoConfig(projectId: number, config: VideoConfig): Promise<void> {
  return invoke("set_video_config", { projectId, config });
}

/** Resolves to ffmpeg's version string, or rejects if it isn't reachable. */
export function checkFfmpeg(projectId: number): Promise<string> {
  return invoke("check_ffmpeg", { projectId });
}

export function getTranscriptionConfig(
  projectId: number
): Promise<TranscriptionConfig | null> {
  return invoke("get_transcription_config", { projectId });
}

export function setTranscriptionConfig(
  projectId: number,
  config: TranscriptionConfig
): Promise<void> {
  return invoke("set_transcription_config", { projectId, config });
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

/** Built-in presets the project hasn't deleted, plus its custom site profiles. */
export function listSiteProfiles(projectId: number): Promise<LimiterProfile[]> {
  return invoke("list_site_profiles", { projectId });
}

/** Add (or edit, by re-adding with the same name) a custom site profile, or
 * restore a deleted built-in. Resolves to the full visible list. */
export function addSiteProfile(
  projectId: number,
  profile: LimiterProfile
): Promise<LimiterProfile[]> {
  return invoke("add_site_profile", { projectId, profile });
}

/** Delete a site profile by name (built-in or custom). Resolves to the full
 * visible list. */
export function removeSiteProfile(projectId: number, name: string): Promise<LimiterProfile[]> {
  return invoke("remove_site_profile", { projectId, name });
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

/** Persist a user-edited title/description/keyword set for an asset. */
export function setMetadata(
  assetId: number,
  metadata: GeneratedMetadata
): Promise<MetadataResponse> {
  return invoke("set_metadata", { assetId, metadata });
}

// --- SPHIN-4: metadata embedding (IPTC/XMP via exiftool) + CSV export -------

export interface EmbedConfig {
  /** Empty string => look up "exiftool" on PATH. */
  exiftool_path: string;
}

export const DEFAULT_EMBED_CONFIG: EmbedConfig = { exiftool_path: "" };

export interface EmbedOutcome {
  updated: boolean;
  message: string;
}

export function getEmbedConfig(projectId: number): Promise<EmbedConfig | null> {
  return invoke("get_embed_config", { projectId });
}

export function setEmbedConfig(projectId: number, config: EmbedConfig): Promise<void> {
  return invoke("set_embed_config", { projectId, config });
}

/** Resolves to exiftool's version string, or rejects if it isn't reachable. */
export function checkExiftool(projectId: number): Promise<string> {
  return invoke("check_exiftool", { projectId });
}

/** One stock site's edited metadata for an asset (editor's "Preview as"). */
export interface SiteMetadata {
  site_name: string;
  metadata: GeneratedMetadata;
  updated_at: string;
}

export function listSiteMetadata(assetId: number): Promise<SiteMetadata[]> {
  return invoke("list_site_metadata", { assetId });
}

export function setSiteMetadata(assetId: number, siteName: string, metadata: GeneratedMetadata): Promise<void> {
  return invoke("set_site_metadata", { assetId, siteName, metadata });
}

export function deleteSiteMetadata(assetId: number, siteName: string): Promise<void> {
  return invoke("delete_site_metadata", { assetId, siteName });
}

export function embedAssetMetadata(
  projectId: number,
  assetId: number,
  siteName?: string
): Promise<EmbedOutcome> {
  return invoke("embed_asset_metadata", { projectId, assetId, siteName: siteName ?? null });
}

export function exportMetadataCsv(
  assetIds: number[],
  targetPath: string
): Promise<void> {
  return invoke("export_metadata_csv", { assetIds, targetPath });
}

// --- SPHIN-5: keyword enrichment (Shutterstock / Adobe Stock) ---------------

export interface SiteCredentials {
  api_key: string;
  /** Empty string => use the provider's default base URL. */
  base_url: string;
}

export interface KeywordConfig {
  shutterstock: SiteCredentials | null;
  adobe_stock: SiteCredentials | null;
}

export const DEFAULT_KEYWORD_CONFIG: KeywordConfig = {
  shutterstock: null,
  adobe_stock: null,
};

export interface EnrichResponse {
  record_id: number;
  result: GeneratedMetadata;
  added: string[];
  errors: string[];
}

/** Raw JSON of the project's user-edited CSV layouts (see lib/csvFormats). */
export function getCsvLayouts(projectId: number): Promise<string | null> {
  return invoke("get_csv_layouts", { projectId });
}

export function setCsvLayouts(projectId: number, json: string): Promise<void> {
  return invoke("set_csv_layouts", { projectId, json });
}

export function getKeywordConfig(projectId: number): Promise<KeywordConfig | null> {
  return invoke("get_keyword_config", { projectId });
}

export function setKeywordConfig(
  projectId: number,
  config: KeywordConfig
): Promise<void> {
  return invoke("set_keyword_config", { projectId, config });
}

export function enrichKeywords(
  projectId: number,
  assetId: number
): Promise<EnrichResponse> {
  return invoke("enrich_keywords", { projectId, assetId });
}

// --- SPHIN-7: job orchestration ---------------------------------------------

export type QueueJobType = "analyze" | "generate_metadata" | "embed" | "enrich_keywords" | "upload";

export interface Job {
  id: number;
  asset_id: number;
  job_type: string;
  status: string;
  error: string | null;
  source: string;
  payload_json: string;
  attempts: number;
  not_before: string | null;
  created_at: string;
  updated_at: string;
}

export interface JobCounts {
  pending: number;
  running: number;
  done: number;
  failed: number;
  cancelled: number;
}

/** One recorded status-change event for a job (queued, started, retried,
 * done/failed), oldest first -- the real per-job history, as opposed to
 * `Job.status`/`error` which each new transition overwrites. */
export interface JobEvent {
  id: number;
  job_id: number;
  status: string;
  message: string;
  at: string;
}

export function enqueueBatch(
  projectId: number,
  assetIds: number[],
  jobType: QueueJobType,
  profileId?: number,
  siteName?: string
): Promise<Job[]> {
  return invoke("enqueue_batch", { projectId, assetIds, jobType, profileId: profileId ?? null, siteName: siteName ?? null });
}

export function listQueueJobs(limit = 100): Promise<Job[]> {
  return invoke("list_queue_jobs", { limit });
}

export function queueJobCounts(): Promise<JobCounts> {
  return invoke("queue_job_counts");
}

export function retryJob(jobId: number): Promise<void> {
  return invoke("retry_job", { jobId });
}

export function cancelJob(jobId: number): Promise<void> {
  return invoke("cancel_job", { jobId });
}

/** Deletes finished (done/failed/cancelled) queue jobs: the given ids, or
 * every finished job with `status` when no ids are passed. */
export function removeJobs(opts: { jobIds?: number[]; status?: string }): Promise<number> {
  return invoke("remove_jobs", { jobIds: opts.jobIds ?? null, status: opts.status ?? null });
}

export function listJobEvents(jobId: number): Promise<JobEvent[]> {
  return invoke("list_job_events", { jobId });
}

/** Fires whenever a queue job's status changes (claimed, done, failed). */
export function onJobUpdated(handler: (job: Job) => void): Promise<UnlistenFn> {
  return listen<Job>("job-updated", (event) => handler(event.payload));
}

// --- SPHIN-6: SFTP upload ----------------------------------------------------

export type SftpSite = "generic" | "adobe_stock";
export type TransportProtocol = "sftp" | "ftps";

export interface SftpProfile {
  id: number;
  project_id: number;
  name: string;
  site: SftpSite;
  protocol: TransportProtocol;
  host: string;
  port: number;
  username: string;
  remote_dir: string;
  credential_key: string;
  host_key_fingerprint: string | null;
  created_at: string;
  updated_at: string;
}

export function listSftpProfiles(projectId: number): Promise<SftpProfile[]> {
  return invoke("list_sftp_profiles", { projectId });
}

export function createSftpProfile(
  projectId: number,
  profile: { name: string; site: SftpSite; protocol: TransportProtocol; host: string; port: number; username: string; remote_dir: string },
  password: string
): Promise<SftpProfile> {
  const { remote_dir, ...rest } = profile;
  return invoke("create_sftp_profile", { projectId, ...rest, remoteDir: remote_dir, password });
}

export function updateSftpProfile(
  id: number,
  profile: { name: string; site: SftpSite; protocol: TransportProtocol; host: string; port: number; username: string; remote_dir: string },
  password?: string
): Promise<void> {
  const { remote_dir, ...rest } = profile;
  return invoke("update_sftp_profile", { id, ...rest, remoteDir: remote_dir, password: password || null });
}

export function deleteSftpProfile(id: number): Promise<void> {
  return invoke("delete_sftp_profile", { id });
}

export function uploadAsset(profileId: number, assetId: number): Promise<void> {
  return invoke("upload_asset", { profileId, assetId });
}

/** Connects and authenticates against a saved profile without transferring a
 * file. Resolves with the host key fingerprint seen (and pinned, same as a
 * real upload) on success. */
export function testSftpConnection(profileId: number): Promise<string> {
  return invoke("test_sftp_connection", { profileId });
}
