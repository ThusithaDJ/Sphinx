import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { buildCsv, csvFormatFor, type CsvColumn, type CsvLayouts } from "../lib/csvFormats";
import { deliverySupportFor } from "../lib/delivery";
import { type ToastItem, type ToastKind } from "../components/Toast";
import {
  type AnalysisConfig,
  type AnalysisResult,
  type Asset,
  type EmbedConfig,
  type GeneratedMetadata,
  type GpuInfo,
  type IngestSummary,
  type Job,
  type JobCounts,
  type KeywordConfig,
  type LimiterProfile,
  type ProviderKind,
  type QueueJobType,
  type SftpProfile,
  type SftpSite,
  type TransportProtocol,
  type TranscriptionConfig,
  type VideoConfig,
  DEFAULT_ANALYSIS_CONFIG,
  DEFAULT_EMBED_CONFIG,
  DEFAULT_KEYWORD_CONFIG,
  DEFAULT_TRANSCRIPTION_CONFIG,
  DEFAULT_VIDEO_CONFIG,
  addSiteProfile,
  analyzeAsset,
  assetCount,
  cancelJob,
  removeJobs,
  checkExiftool,
  checkFfmpeg,
  checkOllama,
  createSftpProfile,
  deleteAsset,
  deleteSftpProfile,
  detectGpu,
  embedAssetMetadata,
  listSiteMetadata,
  enqueueBatch,
  enrichKeywords,
  generateMetadata,
  getAnalysis,
  getAnalysisConfig,
  getAnalysisConfigs,
  getEmbedConfig,
  getKeywordConfig,
  getLimiterProfile,
  getMetadata,
  getTranscriptionConfig,
  getVideoConfig,
  ingestFiles,
  ingestFolder,
  listAssets,
  listLimiterProfiles,
  listQueueJobs,
  getCsvLayouts,
  listSftpProfiles,
  listSiteProfiles,
  onAssetsIngested,
  onJobUpdated,
  queueJobCounts,
  removeSiteProfile,
  retryJob,
  setAnalysisConfig,
  setCsvLayouts,
  setEmbedConfig,
  setKeywordConfig,
  setLimiterProfile,
  setMetadata,
  setTranscriptionConfig,
  setVideoConfig,
  startWatch,
  stopWatch,
  testSftpConnection as testSftpConnectionApi,
  updateSftpProfile,
  uploadAsset,
} from "../lib/api";

export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff", "webp", "bmp", "heic"];
export const VIDEO_EXTENSIONS = ["mp4", "mov", "mkv", "avi", "webm", "m4v"];
export const ANALYZABLE_EXTENSIONS = ["jpg", "jpeg", "png", "webp", "gif"];

// Everything operates on the built-in "Default" project for now; a project
// switcher is out of scope for this redesign (see new_ui_design/README.md).
export const PROJECT_ID = 1;

export const PROVIDER_LABELS: Record<ProviderKind, string> = {
  openai: "OpenAI (GPT-4o)",
  gemini: "Google Gemini",
  anthropic: "Anthropic Claude",
  ollama: "Ollama (local)",
};

export function extensionOf(path: string): string {
  const dot = path.lastIndexOf(".");
  return dot === -1 ? "" : path.slice(dot + 1).toLowerCase();
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

function summaryLine(summary: IngestSummary): string {
  const parts = [`${summary.ingested} ingested`];
  if (summary.duplicates) parts.push(`${summary.duplicates} duplicate${summary.duplicates === 1 ? "" : "s"}`);
  if (summary.skipped) parts.push(`${summary.skipped} skipped`);
  if (summary.errors) parts.push(`${summary.errors} error${summary.errors === 1 ? "" : "s"}`);
  return parts.join(" · ");
}

/** Best-effort color for a status message that didn't specify one explicitly. */
function classifyStatus(message: string): ToastKind {
  if (/fail|could not|error/i.test(message)) return "error";
  if (/select .* first|no eligible|no assets are eligible|not configured/i.test(message)) return "info";
  return "success";
}

export interface ToolStatus {
  checking: boolean;
  ok: boolean;
  detail: string;
}

const CHECKING: ToolStatus = { checking: true, ok: false, detail: "checking…" };

/** Pipeline stage derived from what's on record for an asset, 0-5. */
export type Stage = 0 | 1 | 2 | 3 | 4 | 5;
export const STAGE_NAMES = ["new", "Analyze", "Generate", "Enrich", "Embed", "Upload"] as const;

/** Library-level content status for an asset. Shared by the Library grid,
 * its filter pills, the nav badge, Home and Triage so every "Needs review"
 * count agrees. */
export type AssetFlag = "new" | "review" | "ready" | "uploaded" | "blocked" | "failed";
export const ASSET_FLAG_LABEL: Record<AssetFlag, string> = {
  new: "new",
  review: "needs review",
  ready: "ready",
  uploaded: "uploaded",
  blocked: "blocked",
  failed: "failed",
};

/** Human label for the persisted `assets.status` column. */
export const ASSET_STATUS_LABEL: Record<string, string> = {
  ingested: "Imported",
  analyzed: "Analyzed",
  metadata_generated: "Metadata generated",
  embedded: "Metadata embedded",
  uploaded: "Uploaded",
};

export function assetFlagOf(
  asset: Asset,
  opts: {
    analyzeError: boolean;
    hasAnalysis: boolean;
    metadata: GeneratedMetadata | undefined;
    ffmpegOk: boolean;
  }
): AssetFlag {
  if (asset.status === "uploaded") return "uploaded";
  // Approving in the editor/triage or running Embed from the pipeline writes
  // the metadata into the file -- that approval is what makes an asset ready.
  if (asset.status === "embedded") return "ready";
  if (opts.analyzeError) return "failed";
  if (asset.media_type === "video" && !opts.ffmpegOk && !opts.hasAnalysis) return "blocked";
  if (!opts.metadata) return "new";
  return "review";
}

export function stageOf(
  asset: Asset,
  hasAnalysis: boolean,
  hasMetadata: boolean
): Stage {
  // Enrichment isn't tracked as a distinct persisted flag today, so this
  // never claims the "Enrich" dot on its own -- only Analyze/Generate/
  // Embed/Upload are backed by a concrete signal.
  if (asset.status === "uploaded") return 5;
  if (asset.status === "embedded") return 4;
  if (hasMetadata) return 2;
  if (hasAnalysis) return 1;
  return 0;
}

export type Screen = "home" | "import" | "library" | "activity" | "sites" | "connections" | "settings" | "about";

export type LibraryMode = "browse" | "triage";

export type PipelineStep = "analyze" | "generate_metadata" | "embed" | "upload" | "export_csv";
export type PipelineStopState = "done" | "next" | "locked";

export interface PipelineStop {
  step: PipelineStep;
  state: PipelineStopState;
  doneCount: number;
  total: number;
  blockedReason?: string;
}

const PIPELINE_STEPS: PipelineStep[] = ["analyze", "generate_metadata", "embed", "upload", "export_csv"];

/** True when a single asset has finished the given pipeline step. */
function stepDone(app: SphinxAppLike, asset: Asset, step: PipelineStep): boolean {
  switch (step) {
    case "analyze":
      return Boolean(app.analyses[asset.id]);
    case "generate_metadata":
      return Boolean(app.metadata[asset.id]);
    case "embed":
      return asset.status === "embedded" || asset.status === "uploaded";
    case "upload":
      return asset.status === "uploaded";
    case "export_csv":
      return app.csvExportedIds.has(asset.id);
  }
}

/** Minimal shape `pipelineStops` needs -- kept separate from the full app type
 * so this helper can be unit-reasoned about without the whole context. */
interface SphinxAppLike {
  assets: Asset[];
  analyses: Record<number, AnalysisResult>;
  metadata: Record<number, GeneratedMetadata>;
  hasKey: boolean;
  exiftool: ToolStatus;
  sftpProfiles: SftpProfile[];
  ffmpeg: ToolStatus;
  csvExportedIds: Set<number>;
}

/** Derives the pipeline stepper state for a selection of assets. A stop is
 * "done" only once every selected asset has finished it; the first stop that
 * isn't fully done is "next", everything after stays "locked" even if
 * individually satisfiable (the stepper is a dependency chain, not a set of
 * independent toggles). Export CSV is the exception: it only needs metadata,
 * since sites without SFTP/FTPS delivery (e.g. iStock) never reach Upload.
 * Upload delivers to `targetSite`'s transport. */
export function pipelineStops(app: SphinxAppLike, assetIds: number[], targetSite: string): PipelineStop[] {
  const selected = app.assets.filter((a) => assetIds.includes(a.id));
  const total = selected.length;
  const stops: PipelineStop[] = [];
  let reachedNext = false;
  for (const step of PIPELINE_STEPS) {
    const doneCount = selected.filter((a) => stepDone(app, a, step)).length;
    const done = total > 0 && doneCount === total;
    let state: PipelineStopState;
    if (done) state = "done";
    else if (step === "export_csv") {
      const hasMetadata = total > 0 && selected.every((a) => stepDone(app, a, "generate_metadata"));
      state = hasMetadata ? "next" : "locked";
    } else if (!reachedNext) {
      state = "next";
      reachedNext = true;
    } else state = "locked";

    let blockedReason: string | undefined;
    if (state === "next") {
      if (step === "analyze" && !app.hasKey) blockedReason = "Configure an AI provider in Connections first";
      else if (step === "analyze" && selected.some((a) => a.media_type === "video") && !app.ffmpeg.ok)
        blockedReason = "ffmpeg not found — needed for video keyframes";
      else if (step === "embed" && !app.exiftool.ok) blockedReason = "exiftool not found — configure it in Connections";
      else if (step === "upload" && !deliverySupportFor(targetSite).supported)
        blockedReason = `${targetSite} doesn't accept SFTP/FTPS uploads — use Export CSV`;
      else if (step === "upload" && !deliveryProfileFor(app.sftpProfiles, targetSite))
        blockedReason = `No delivery set up for ${targetSite} — configure it in Sites`;
    }

    stops.push({ step, state, doneCount, total, blockedReason });
  }
  return stops;
}

const STOP_FIRST_RUN_LABEL: Record<PipelineStep, string> = {
  analyze: "Start",
  generate_metadata: "Generate",
  embed: "Approve & Embed",
  upload: "Start",
  export_csv: "Export",
};

const STOP_RERUN_LABEL: Record<PipelineStep, string> = {
  analyze: "Re-analyze",
  generate_metadata: "Regenerate",
  embed: "Approve & Embed",
  upload: "Start",
  export_csv: "Re-export",
};

/** Button label for a pipeline stop, per the button-label table in the
 * redesign handoff (README §"Library — Browse (3a, 3b, 3c)"). */
export function stopLabel(step: PipelineStep, state: PipelineStopState): string {
  if (state === "done") return STOP_RERUN_LABEL[step];
  return STOP_FIRST_RUN_LABEL[step];
}

export const PIPELINE_STEP_NAMES: Record<PipelineStep, string> = {
  analyze: "Analyze",
  generate_metadata: "Generate metadata",
  embed: "Embed",
  upload: "Upload",
  export_csv: "Export CSV",
};

/** The delivery transport configured for a site profile, matched by name
 * (Adobe Stock also matches its dedicated `adobe_stock` transport kind). */
export function deliveryProfileFor(transports: SftpProfile[], siteName: string): SftpProfile | undefined {
  return (
    (siteName === "Adobe Stock" ? transports.find((t) => t.site === "adobe_stock") : undefined) ??
    transports.find((t) => t.name.toLowerCase() === siteName.toLowerCase())
  );
}

const STEP_LABEL_FOR_ATTENTION: Record<string, string> = {
  analyze: "Analyze",
  generate_metadata: "Generate metadata",
  enrich_keywords: "Enrich keywords",
  embed: "Embed",
  upload: "Upload",
};

export type IngestVerdict = "ready" | "duplicate" | "unsupported" | "needs-ffmpeg";

export interface IncomingFile {
  path: string;
  name: string;
  verdict: IngestVerdict;
  detail: string;
}

export interface WatchFolder {
  path: string;
  active: boolean;
}

export interface SftpDraft {
  name: string;
  site: SftpSite;
  protocol: TransportProtocol;
  host: string;
  port: number;
  username: string;
  remote_dir: string;
  password: string;
}

export const BLANK_SFTP_DRAFT: SftpDraft = {
  name: "",
  site: "generic",
  protocol: "sftp",
  host: "",
  port: 22,
  username: "",
  remote_dir: "/",
  password: "",
};

function useSphinxApp() {
  const [screen, setScreen] = useState<Screen>("home");
  const [libraryMode, setLibraryMode] = useState<LibraryMode>("browse");
  // Session-only: which assets have been included in a CSV export.
  const [csvExportedIds, setCsvExportedIds] = useState<Set<number>>(new Set());
  const [editingAssetId, setEditingAssetId] = useState<number | null>(null);
  // Set by Home's "Review failures" link so Activity opens pre-filtered;
  // Activity clears it once read.
  const [activityFilter, setActivityFilter] = useState<"failed" | null>(null);
  // Session-only: the demand floor below which a keyword is flagged as weak.
  // Not yet persisted server-side (no per-project field for it today).
  const [demandFloor, setDemandFloor] = useState(20);

  const [assets, setAssets] = useState<Asset[]>([]);
  const [assetTotal, setAssetTotal] = useState(0);
  const [status, setStatusRaw] = useState("Drop images or video to get started.");
  const [busy, setBusy] = useState(false);
  const [busyLabel, setBusyLabel] = useState<string | null>(null);

  // --- toast notifications ---
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const nextToastId = useRef(0);

  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const pushToast = useCallback(
    (message: string, kind: ToastKind) => {
      const id = ++nextToastId.current;
      setToasts((prev) => [...prev, { id, message, kind }]);
      setTimeout(() => dismissToast(id), 4200);
    },
    [dismissToast]
  );

  // Every action already reports through setStatus, so wiring the toast push
  // in here gives every action a colored notification for free instead of
  // touching every call site. Kind is inferred from the message unless the
  // caller is explicit -- see classifyStatus below.
  const setStatus = useCallback(
    (message: string, kind?: ToastKind) => {
      setStatusRaw(message);
      pushToast(message, kind ?? classifyStatus(message));
    },
    [pushToast]
  );

  // --- import (2a) ---
  const [isDragging, setIsDragging] = useState(false);
  const [watch, setWatch] = useState<WatchFolder | null>(null);
  const [incoming, setIncoming] = useState<IncomingFile[]>([]);
  const [queueAfterIngest, setQueueAfterIngest] = useState(true);
  const [lastIngestNote, setLastIngestNote] = useState<string | null>(null);

  // --- analysis (SPHIN-2) ---
  const [analysisConfig, setAnalysisConfigState] = useState<AnalysisConfig>(DEFAULT_ANALYSIS_CONFIG);
  const [activeAnalysisConfig, setActiveAnalysisConfig] = useState<AnalysisConfig | null>(null);
  const [savedAnalysisConfigs, setSavedAnalysisConfigs] = useState<Record<string, AnalysisConfig>>({});
  const [analysisConfigReady, setAnalysisConfigReady] = useState(false);
  const [analyses, setAnalyses] = useState<Record<number, AnalysisResult>>({});
  const [analyzing, setAnalyzing] = useState<Record<number, boolean>>({});
  const [analyzeErrors, setAnalyzeErrors] = useState<Record<number, string>>({});

  // --- local AI model support (SPHIN-9) ---
  const [ollama, setOllama] = useState<ToolStatus>(CHECKING);
  const [gpu, setGpu] = useState<GpuInfo | null>(null);

  // --- metadata generation & limiter profiles (SPHIN-3 / SPHIN-19) ---
  const [limiterPresets, setLimiterPresets] = useState<LimiterProfile[]>([]);
  const [builtInProfiles, setBuiltInProfiles] = useState<LimiterProfile[]>([]);
  const [activeProfile, setActiveProfile] = useState<LimiterProfile | null>(null);
  const [profileReady, setProfileReady] = useState(false);
  const [enabledProfileNames, setEnabledProfileNames] = useState<Set<string>>(new Set());
  const [metadata, setMetadataState] = useState<Record<number, GeneratedMetadata>>({});
  const [generating, setGenerating] = useState<Record<number, boolean>>({});

  // --- embedding (SPHIN-4) ---
  const [embedConfig, setEmbedConfigState] = useState<EmbedConfig>(DEFAULT_EMBED_CONFIG);
  const [exiftool, setExiftool] = useState<ToolStatus>(CHECKING);
  const [embedding, setEmbedding] = useState<Record<number, boolean>>({});

  // --- video pipeline (SPHIN-8) ---
  const [videoConfig, setVideoConfigState] = useState<VideoConfig>(DEFAULT_VIDEO_CONFIG);
  const [transcriptionConfig, setTranscriptionConfigState] =
    useState<TranscriptionConfig>(DEFAULT_TRANSCRIPTION_CONFIG);
  const [ffmpeg, setFfmpeg] = useState<ToolStatus>(CHECKING);

  // --- keyword enrichment (SPHIN-5) ---
  const [keywordConfig, setKeywordConfigState] = useState<KeywordConfig>(DEFAULT_KEYWORD_CONFIG);
  const [enriching, setEnriching] = useState<Record<number, boolean>>({});

  // --- CSV export layouts (user-edited, per site) ---
  const [csvLayouts, setCsvLayoutsState] = useState<CsvLayouts>({});

  // --- job orchestration (SPHIN-7) ---
  const [jobs, setJobs] = useState<Job[]>([]);
  const [jobCounts, setJobCounts] = useState<JobCounts>({ pending: 0, running: 0, done: 0, failed: 0, cancelled: 0 });
  const [selectedJobId, setSelectedJobId] = useState<number | null>(null);

  // --- SFTP upload (SPHIN-6) ---
  const [sftpProfiles, setSftpProfiles] = useState<SftpProfile[]>([]);
  const [selectedSftpProfileId, setSelectedSftpProfileId] = useState<number | null>(null);
  const [uploading, setUploading] = useState<Record<number, boolean>>({});

  const refresh = useCallback(async () => {
    const [rows, count] = await Promise.all([listAssets(500, 0), assetCount()]);
    setAssets(rows);
    setAssetTotal(count);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** Remove an asset from the library (catalog only -- never touches the file on disk). */
  async function removeAsset(asset: Asset) {
    try {
      await deleteAsset(asset.id);
      setAssets((prev) => prev.filter((a) => a.id !== asset.id));
      setAssetTotal((prev) => Math.max(0, prev - 1));
      setStatus(`Removed ${fileName(asset.path)} from the library.`, "warning");
    } catch (err) {
      setStatus(`Could not remove ${fileName(asset.path)}: ${String(err)}`, "error");
    }
  }

  // Backfill analyses/metadata for assets we haven't loaded yet.
  const loadedAnalysisIds = useRef<Set<number>>(new Set());
  useEffect(() => {
    const missing = assets.filter((a) => !loadedAnalysisIds.current.has(a.id));
    if (missing.length === 0) return;
    for (const a of missing) loadedAnalysisIds.current.add(a.id);
    void Promise.all(
      missing.map(async (a) => [a.id, (await getAnalysis(a.id))?.result ?? null] as const)
    ).then((loaded) => {
      setAnalyses((prev) => {
        const next = { ...prev };
        for (const [id, result] of loaded) if (result) next[id] = result;
        return next;
      });
    });
  }, [assets]);

  const loadedMetadataIds = useRef<Set<number>>(new Set());
  useEffect(() => {
    const missing = assets.filter((a) => !loadedMetadataIds.current.has(a.id));
    if (missing.length === 0) return;
    for (const a of missing) loadedMetadataIds.current.add(a.id);
    void Promise.all(
      missing.map(async (a) => [a.id, (await getMetadata(a.id))?.result ?? null] as const)
    ).then((loaded) => {
      setMetadataState((prev) => {
        const next = { ...prev };
        for (const [id, result] of loaded) if (result) next[id] = result;
        return next;
      });
    });
  }, [assets]);

  useEffect(() => {
    Promise.all([getAnalysisConfig(PROJECT_ID), getAnalysisConfigs(PROJECT_ID)])
      .then(([saved, all]) => {
        if (saved) {
          setAnalysisConfigState({ ...DEFAULT_ANALYSIS_CONFIG, ...saved });
          setActiveAnalysisConfig(saved);
        }
        setSavedAnalysisConfigs(all);
        setAnalysisConfigReady(true);
      })
      .catch(() => setAnalysisConfigReady(true));
  }, []);

  useEffect(() => {
    listLimiterProfiles()
      .then((builtins) => setBuiltInProfiles(builtins))
      .catch(() => {});
    listSiteProfiles(PROJECT_ID)
      .then((list) => {
        setLimiterPresets(list);
        setEnabledProfileNames(new Set(list.map((p) => p.name)));
      })
      .catch(() => {});
    getLimiterProfile(PROJECT_ID)
      .then((saved) => {
        setActiveProfile(saved ?? null);
        setProfileReady(true);
      })
      .catch(() => setProfileReady(true));
  }, []);

  const runExiftoolCheck = useCallback(async () => {
    setExiftool(CHECKING);
    try {
      const version = await checkExiftool(PROJECT_ID);
      setExiftool({ checking: false, ok: true, detail: `v${version}` });
    } catch (err) {
      setExiftool({ checking: false, ok: false, detail: String(err) });
    }
  }, []);

  useEffect(() => {
    getEmbedConfig(PROJECT_ID)
      .then((saved) => {
        setEmbedConfigState(saved ?? DEFAULT_EMBED_CONFIG);
        void runExiftoolCheck();
      })
      .catch(() => void runExiftoolCheck());
  }, [runExiftoolCheck]);

  const runFfmpegCheck = useCallback(async () => {
    setFfmpeg(CHECKING);
    try {
      const version = await checkFfmpeg(PROJECT_ID);
      setFfmpeg({ checking: false, ok: true, detail: version });
    } catch (err) {
      setFfmpeg({ checking: false, ok: false, detail: String(err) });
    }
  }, []);

  useEffect(() => {
    Promise.all([getVideoConfig(PROJECT_ID), getTranscriptionConfig(PROJECT_ID)])
      .then(([v, t]) => {
        setVideoConfigState(v ?? DEFAULT_VIDEO_CONFIG);
        setTranscriptionConfigState(t ?? DEFAULT_TRANSCRIPTION_CONFIG);
        void runFfmpegCheck();
      })
      .catch(() => void runFfmpegCheck());
  }, [runFfmpegCheck]);

  const runOllamaCheck = useCallback(async () => {
    setOllama(CHECKING);
    try {
      const detail = await checkOllama(PROJECT_ID);
      setOllama({ checking: false, ok: true, detail });
    } catch (err) {
      setOllama({ checking: false, ok: false, detail: String(err) });
    }
  }, []);

  useEffect(() => {
    void runOllamaCheck();
  }, [runOllamaCheck]);

  useEffect(() => {
    detectGpu()
      .then(setGpu)
      .catch(() => setGpu({ available: false, backend: "none", detail: "detection failed" }));
  }, []);

  useEffect(() => {
    getCsvLayouts(PROJECT_ID)
      .then((json) => setCsvLayoutsState(json ? (JSON.parse(json) as CsvLayouts) : {}))
      .catch(() => {});
  }, []);

  useEffect(() => {
    getKeywordConfig(PROJECT_ID)
      .then((saved) => setKeywordConfigState(saved ?? DEFAULT_KEYWORD_CONFIG))
      .catch(() => {});
  }, []);

  const refreshSftpProfiles = useCallback(async () => {
    const rows = await listSftpProfiles(PROJECT_ID);
    setSftpProfiles(rows);
    setSelectedSftpProfileId((prev) => prev ?? rows[0]?.id ?? null);
  }, []);

  useEffect(() => {
    void refreshSftpProfiles();
  }, [refreshSftpProfiles]);

  const refreshJobs = useCallback(async () => {
    const [rows, counts] = await Promise.all([listQueueJobs(200), queueJobCounts()]);
    setJobs(rows);
    setJobCounts(counts);
  }, []);

  useEffect(() => {
    void refreshJobs();
  }, [refreshJobs]);

  const refreshAssetResult = useCallback(async (assetId: number) => {
    const [analysis, meta] = await Promise.all([getAnalysis(assetId), getMetadata(assetId)]);
    if (analysis) setAnalyses((prev) => ({ ...prev, [assetId]: analysis.result }));
    if (meta) setMetadataState((prev) => ({ ...prev, [assetId]: meta.result }));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onJobUpdated((job) => {
      void refreshJobs();
      if (job.status === "done") {
        void refreshAssetResult(job.asset_id);
        void refresh();
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refreshJobs, refreshAssetResult, refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onAssetsIngested((summary) => {
      setLastIngestNote(`Watch folder: ${summaryLine(summary)}`);
      void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refresh]);

  const hasKey = Boolean(
    activeAnalysisConfig &&
      (activeAnalysisConfig.provider === "ollama" || activeAnalysisConfig.api_key.trim().length > 0)
  );
  const hasKeywordProvider = Boolean(keywordConfig.shutterstock || keywordConfig.adobe_stock);

  // --- ingest ---

  function classifyLocal(path: string): { verdict: IngestVerdict; detail: string } {
    const ext = extensionOf(path);
    if (IMAGE_EXTENSIONS.includes(ext)) return { verdict: "ready", detail: "image" };
    if (VIDEO_EXTENSIONS.includes(ext)) {
      return ffmpeg.ok
        ? { verdict: "ready", detail: "video" }
        : { verdict: "needs-ffmpeg", detail: "keyframes need ffmpeg (Settings → Video pipeline)" };
    }
    return { verdict: "unsupported", detail: `.${ext || "?"} is not a supported format` };
  }

  const addIncoming = useCallback(
    (paths: string[]) => {
      const rows: IncomingFile[] = paths.map((path) => {
        const c = classifyLocal(path);
        return { path, name: fileName(path), verdict: c.verdict, detail: c.detail };
      });
      setIncoming((prev) => [...prev, ...rows]);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ffmpeg.ok]
  );

  async function commitIngest() {
    const ready = incoming.filter((f) => f.verdict === "ready");
    if (ready.length === 0) {
      setStatus("No eligible files to ingest.");
      return;
    }
    setBusy(true);
    setBusyLabel(`Ingesting ${ready.length} file${ready.length === 1 ? "" : "s"}…`);
    try {
      const summary = await ingestFiles(ready.map((f) => f.path));
      setStatus(summaryLine(summary), "success");
      setIncoming([]);
      await refresh();
      if (queueAfterIngest) {
        const ids = summary.outcomes
          .filter((o) => o.outcome === "ingested")
          .map((o) => (o as { asset: Asset }).asset.id);
        if (ids.length) await handleEnqueueBatch("analyze", "Analyze", ids);
      }
    } catch (err) {
      setStatus(`Ingestion failed: ${String(err)}`, "error");
    } finally {
      setBusy(false);
      setBusyLabel(null);
    }
  }

  function clearIncoming() {
    setIncoming([]);
  }

  async function pickFiles() {
    const selection = await open({
      multiple: true,
      filters: [
        { name: "Images", extensions: IMAGE_EXTENSIONS },
        { name: "Video", extensions: VIDEO_EXTENSIONS },
      ],
    });
    if (!selection) return;
    addIncoming(Array.isArray(selection) ? selection : [selection]);
  }

  async function pickFolder() {
    const selection = await open({ directory: true });
    if (!selection || Array.isArray(selection)) return;
    setBusy(true);
    setBusyLabel(`Importing ${selection}…`);
    try {
      const summary = await ingestFolder(selection);
      setStatus(`Folder import: ${summaryLine(summary)}`, "success");
      await refresh();
    } catch (err) {
      setStatus(`Folder import failed: ${String(err)}`, "error");
    } finally {
      setBusy(false);
      setBusyLabel(null);
    }
  }

  async function addWatchFolder() {
    const selection = await open({ directory: true });
    if (!selection || Array.isArray(selection)) return;
    if (watch?.active) await stopWatch();
    await startWatch(selection);
    setWatch({ path: selection, active: true });
    setStatus(`Watching ${selection} for new files…`);
  }

  async function toggleWatch() {
    if (!watch) return;
    if (watch.active) {
      await stopWatch();
      setWatch({ ...watch, active: false });
      setStatus("Folder watch stopped.");
    } else {
      await startWatch(watch.path);
      setWatch({ ...watch, active: true });
      setStatus(`Watching ${watch.path} for new files…`);
    }
  }

  function removeWatchFolder() {
    if (watch?.active) void stopWatch();
    setWatch(null);
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setIsDragging(true);
        } else if (event.payload.type === "drop") {
          setIsDragging(false);
          addIncoming(event.payload.paths);
        } else {
          setIsDragging(false);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addIncoming]);

  // --- analysis config ---

  function patchAnalysisConfig(patch: Partial<AnalysisConfig>) {
    setAnalysisConfigState((prev) => ({ ...prev, ...patch }));
  }

  function switchAnalysisProvider(provider: ProviderKind) {
    const saved = savedAnalysisConfigs[provider];
    setAnalysisConfigState(
      saved ? { ...DEFAULT_ANALYSIS_CONFIG, ...saved } : { ...DEFAULT_ANALYSIS_CONFIG, provider }
    );
  }

  async function saveAnalysisConfig() {
    try {
      await setAnalysisConfig(PROJECT_ID, analysisConfig);
      setActiveAnalysisConfig(analysisConfig);
      setSavedAnalysisConfigs((prev) => ({ ...prev, [analysisConfig.provider]: analysisConfig }));
      setStatus(`Saved ${PROVIDER_LABELS[analysisConfig.provider]} configuration.`);
      if (analysisConfig.provider === "ollama") void runOllamaCheck();
    } catch (err) {
      setStatus(`Could not save configuration: ${String(err)}`);
    }
  }

  // --- limiter profiles / sites ---

  function toggleProfileEnabled(name: string) {
    setEnabledProfileNames((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  const enabledSites = useMemo(
    () => limiterPresets.filter((p) => enabledProfileNames.has(p.name)),
    [limiterPresets, enabledProfileNames]
  );

  async function saveActiveProfile(profile: LimiterProfile) {
    try {
      await setLimiterProfile(PROJECT_ID, profile);
      setActiveProfile(profile);
      setStatus(`"${profile.name}" is now the active target profile.`);
    } catch (err) {
      setStatus(`Could not save limiter profile: ${String(err)}`);
    }
  }

  /** Add a new stock-site profile with permissive starting limits (the user
   * tunes them afterwards from the per-profile detail form). Re-adding a
   * deleted built-in name restores that preset. Resolves to the added
   * profile's name, or null on failure. */
  async function createSiteProfile(name: string) {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      const visible = await addSiteProfile(PROJECT_ID, {
        name: trimmed,
        max_title_chars: 200,
        max_description_chars: 200,
        min_keywords: 0,
        max_keywords: 50,
        max_keyword_chars: 50,
      });
      setLimiterPresets(visible);
      const added = visible.find((p) => p.name.toLowerCase() === trimmed.toLowerCase())?.name ?? trimmed;
      setEnabledProfileNames((prev) => new Set(prev).add(added));
      setStatus(`Added site profile "${added}".`, "success");
      return added;
    } catch (err) {
      setStatus(`Could not add site profile: ${String(err)}`, "error");
      return null;
    }
  }

  /** Deletes a site profile (built-in or custom) along with its delivery
   * transport, if one is configured. */
  async function deleteSiteProfile(name: string) {
    try {
      const transport = deliveryProfileFor(sftpProfiles, name);
      if (transport) await removeSftpProfile(transport);
      const visible = await removeSiteProfile(PROJECT_ID, name);
      if (csvLayouts[name]) {
        const { [name]: _removed, ...rest } = csvLayouts;
        await persistCsvLayouts(rest);
      }
      setLimiterPresets(visible);
      setEnabledProfileNames((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
      setStatus(`Removed site profile "${name}".`, "warning");
    } catch (err) {
      setStatus(`Could not remove site profile: ${String(err)}`, "error");
    }
  }

  // --- generate / embed / enrich / upload / analyze ---

  async function handleAnalyze(asset: Asset) {
    setAnalyzing((prev) => ({ ...prev, [asset.id]: true }));
    setAnalyzeErrors((prev) => {
      const { [asset.id]: _drop, ...rest } = prev;
      return rest;
    });
    setStatus(`Analyzing ${fileName(asset.path)}…`);
    try {
      const { result } = await analyzeAsset(PROJECT_ID, asset.id);
      setAnalyses((prev) => ({ ...prev, [asset.id]: result }));
      setStatus(`Analyzed with ${result.provider}/${result.model}.`);
      await refresh();
    } catch (err) {
      const message = String(err);
      setAnalyzeErrors((prev) => ({ ...prev, [asset.id]: message }));
      setStatus(`Analysis failed: ${message}`);
    } finally {
      setAnalyzing((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  async function handleGenerateMetadata(asset: Asset) {
    setGenerating((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const { result } = await generateMetadata(PROJECT_ID, asset.id);
      setMetadataState((prev) => ({ ...prev, [asset.id]: result }));
      setStatus(`Generated metadata for ${fileName(asset.path)}.`);
      await refresh();
    } catch (err) {
      setStatus(`Metadata generation failed: ${String(err)}`);
    } finally {
      setGenerating((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  async function handleSaveMetadata(assetId: number, meta: GeneratedMetadata) {
    const { result } = await setMetadata(assetId, meta);
    setMetadataState((prev) => ({ ...prev, [assetId]: result }));
    return result;
  }

  /** Resolves true when exiftool wrote the file (the asset is now Ready).
   * `siteName` embeds that site's edited draft instead of the AI default. */
  async function handleEmbed(asset: Asset, siteName?: string): Promise<boolean> {
    setEmbedding((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const outcome = await embedAssetMetadata(PROJECT_ID, asset.id, siteName);
      setStatus(
        outcome.updated
          ? `Embedded metadata into ${fileName(asset.path)}.`
          : `Exiftool ran but reported no changes for ${fileName(asset.path)}.`
      );
      await refresh();
      return true;
    } catch (err) {
      setStatus(`Embedding failed: ${String(err)}`, "error");
      return false;
    } finally {
      setEmbedding((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  async function handleSaveAndEmbed(asset: Asset, meta: GeneratedMetadata) {
    await handleSaveMetadata(asset.id, meta);
    await handleEmbed(asset);
  }

  async function handleEnrich(asset: Asset) {
    setEnriching((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const { result, added, errors } = await enrichKeywords(PROJECT_ID, asset.id);
      setMetadataState((prev) => ({ ...prev, [asset.id]: result }));
      const parts = [
        added.length ? `added ${added.length} keyword${added.length === 1 ? "" : "s"}` : "no new keywords",
      ];
      if (errors.length) parts.push(`errors: ${errors.join("; ")}`);
      setStatus(`Enrichment: ${parts.join(" · ")}`);
    } catch (err) {
      setStatus(`Enrichment failed: ${String(err)}`);
    } finally {
      setEnriching((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  async function handleUpload(asset: Asset) {
    if (!selectedSftpProfileId) {
      setStatus("Select an SFTP profile first.");
      return;
    }
    setUploading((prev) => ({ ...prev, [asset.id]: true }));
    try {
      await uploadAsset(selectedSftpProfileId, asset.id);
      setStatus(`Uploaded ${fileName(asset.path)}.`);
      await refresh();
    } catch (err) {
      setStatus(`Upload failed: ${String(err)}`);
    } finally {
      setUploading((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  function eligibleForBatch(jobType: QueueJobType): Asset[] {
    switch (jobType) {
      case "analyze":
        return assets.filter(
          (a) =>
            hasKey &&
            ((a.media_type === "image" && ANALYZABLE_EXTENSIONS.includes(extensionOf(a.path))) ||
              a.media_type === "video") &&
            !analyses[a.id]
        );
      case "generate_metadata":
        return assets.filter((a) => analyses[a.id] && !metadata[a.id]);
      case "embed":
        return assets.filter((a) => metadata[a.id]);
      case "enrich_keywords":
        return hasKeywordProvider ? assets.filter((a) => metadata[a.id]) : [];
      case "upload":
        return selectedSftpProfileId ? assets.filter((a) => metadata[a.id]) : [];
    }
  }

  /** `sftpProfileId` picks the upload transport; it falls back to the first
   * saved one when omitted. */
  async function handleEnqueueBatch(
    jobType: QueueJobType,
    label: string,
    assetIds?: number[],
    sftpProfileId?: number,
    siteName?: string
  ) {
    const targets = assetIds ? assets.filter((a) => assetIds.includes(a.id)) : eligibleForBatch(jobType);
    if (targets.length === 0) {
      setStatus(`No assets are eligible to queue for "${label}" right now.`);
      return;
    }
    try {
      await enqueueBatch(
        PROJECT_ID,
        targets.map((a) => a.id),
        jobType,
        jobType === "upload" ? sftpProfileId ?? selectedSftpProfileId ?? undefined : undefined,
        siteName
      );
      setStatus(`Queued ${targets.length} asset(s) for "${label}".`);
      await refreshJobs();
    } catch (err) {
      setStatus(`Could not queue batch: ${String(err)}`);
    }
  }

  async function handleRetryJob(job: Job) {
    try {
      await retryJob(job.id);
      await refreshJobs();
    } catch (err) {
      setStatus(`Could not retry job: ${String(err)}`);
    }
  }

  async function handleRemoveJobs(opts: { jobIds?: number[]; status?: string }) {
    try {
      const removed = await removeJobs(opts);
      await refreshJobs();
      setStatus(`Removed ${removed} job${removed === 1 ? "" : "s"}.`);
    } catch (err) {
      setStatus(`Could not remove jobs: ${String(err)}`);
    }
  }

  async function handleCancelJob(job: Job) {
    try {
      await cancelJob(job.id);
      await refreshJobs();
    } catch (err) {
      setStatus(`Could not cancel job: ${String(err)}`);
    }
  }

  /** Writes the selection's metadata as a CSV in `siteName`'s bulk-upload
   * layout (see lib/csvFormats) to a path the user picks. */
  async function handleExportCsv(assetIds: number[], siteName: string) {
    // Each asset exports the site's edited draft when one exists, else the
    // AI-generated default.
    const picked = assets.filter((a) => assetIds.includes(a.id) && metadata[a.id]);
    const rows = await Promise.all(
      picked.map(async (a) => {
        const drafts = await listSiteMetadata(a.id).catch(() => []);
        const draft = drafts.find((d) => d.site_name === siteName)?.metadata;
        return { fileName: fileName(a.path), metadata: draft ?? metadata[a.id] };
      })
    );
    if (rows.length === 0) {
      setStatus("No selected assets have metadata to export.", "warning");
      return;
    }
    const slug = siteName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "metadata";
    const targetPath = await save({
      title: `Export ${siteName} CSV`,
      defaultPath: `${slug}-metadata.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!targetPath) return;
    try {
      await writeTextFile(targetPath, buildCsv(csvFormatFor(siteName, csvLayouts), rows));
      setCsvExportedIds((prev) => {
        const next = new Set(prev);
        for (const id of assetIds) next.add(id);
        return next;
      });
      setStatus(`Exported ${rows.length} asset(s) to ${fileName(targetPath)} (${siteName} format).`);
    } catch (err) {
      setStatus(`Could not export CSV: ${String(err)}`, "error");
    }
  }

  async function persistCsvLayouts(next: CsvLayouts) {
    await setCsvLayouts(PROJECT_ID, JSON.stringify(next));
    setCsvLayoutsState(next);
  }

  /** Saves `columns` as `siteName`'s CSV layout; `null` resets it to the
   * built-in template. */
  async function saveCsvLayout(siteName: string, columns: CsvColumn[] | null) {
    const next = { ...csvLayouts };
    if (columns) next[siteName] = columns;
    else delete next[siteName];
    try {
      await persistCsvLayouts(next);
      setStatus(columns ? `Saved the ${siteName} CSV layout.` : `Reset the ${siteName} CSV layout to its default.`, "success");
    } catch (err) {
      setStatus(`Could not save CSV layout: ${String(err)}`, "error");
    }
  }

  // --- embed / video / keyword config ---

  async function saveEmbedConfig(next: EmbedConfig) {
    await setEmbedConfig(PROJECT_ID, next);
    setEmbedConfigState(next);
    await runExiftoolCheck();
  }

  async function saveVideoConfig(nextVideo: VideoConfig, nextTranscription: TranscriptionConfig) {
    await Promise.all([
      setVideoConfig(PROJECT_ID, nextVideo),
      setTranscriptionConfig(PROJECT_ID, nextTranscription),
    ]);
    setVideoConfigState(nextVideo);
    setTranscriptionConfigState(nextTranscription);
    await runFfmpegCheck();
  }

  async function saveKeywordConfig(next: KeywordConfig) {
    await setKeywordConfig(PROJECT_ID, next);
    setKeywordConfigState(next);
  }

  // --- SFTP profiles ---

  async function addSftpProfile(draft: SftpDraft) {
    const { password, ...rest } = draft;
    const created = await createSftpProfile(PROJECT_ID, rest, password);
    await refreshSftpProfiles();
    setStatus(`Added SFTP profile "${rest.name}".`);
    return created;
  }

  async function editSftpProfile(id: number, draft: SftpDraft) {
    const { password, ...rest } = draft;
    await updateSftpProfile(id, rest, password || undefined);
    await refreshSftpProfiles();
    setStatus(`Updated SFTP profile "${rest.name}".`);
  }

  async function removeSftpProfile(profile: SftpProfile) {
    await deleteSftpProfile(profile.id);
    if (selectedSftpProfileId === profile.id) setSelectedSftpProfileId(null);
    await refreshSftpProfiles();
    setStatus(`Deleted SFTP profile "${profile.name}".`);
  }

  /** Connects and authenticates against a saved profile without uploading a
   * file, so credentials/reachability can be checked before a real upload. */
  async function testSftpConnection(profile: SftpProfile) {
    try {
      await testSftpConnectionApi(profile.id);
      setStatus(`Connection to "${profile.name}" succeeded.`, "success");
    } catch (err) {
      setStatus(`Connection to "${profile.name}" failed: ${String(err)}`, "error");
    } finally {
      await refreshSftpProfiles();
    }
  }

  function openEditor(assetId: number) {
    setEditingAssetId(assetId);
    setScreen("library");
  }

  function closeEditor() {
    setEditingAssetId(null);
  }

  const assetFlags = useMemo(() => {
    const map = new Map<number, AssetFlag>();
    for (const a of assets)
      map.set(
        a.id,
        assetFlagOf(a, {
          analyzeError: Boolean(analyzeErrors[a.id]),
          hasAnalysis: Boolean(analyses[a.id]),
          metadata: metadata[a.id],
          ffmpegOk: ffmpeg.ok,
        })
      );
    return map;
  }, [assets, analyses, analyzeErrors, metadata, ffmpeg.ok]);

  const needsReviewIds = useMemo(
    () => assets.filter((a) => assetFlags.get(a.id) === "review").map((a) => a.id),
    [assets, assetFlags]
  );

  /** Groups failed/degraded jobs by (step, normalized error) so the same root
   * cause reads as one row instead of N mysteries (redesign brief, Flow D).
   * Home and Activity both consume this. */
  const attentionGroups = useMemo(() => {
    const groups = new Map<string, { title: string; count: number; severity: "failed" | "degraded"; fixRoute: Screen; subline: string }>();
    for (const job of jobs) {
      if (job.status !== "failed") continue;
      const normalized = (job.error ?? "unknown error").replace(/[\w.-]+\.(jpg|jpeg|png|tif|tiff|webp|bmp|heic|mp4|mov|mkv|avi|webm|m4v)/gi, "<file>");
      const key = `${job.job_type}::${normalized}`;
      const stepName = STEP_LABEL_FOR_ATTENTION[job.job_type] ?? job.job_type;
      const fixRoute: Screen =
        job.job_type === "upload"
          ? "sites"
          : job.job_type === "analyze" || job.job_type === "embed" || job.job_type === "enrich_keywords"
            ? "connections"
            : "activity";
      const existing = groups.get(key);
      if (existing) existing.count += 1;
      else
        groups.set(key, {
          title: normalized === "unknown error" ? `${stepName} failed` : `${normalized} — blocking ${stepName.toLowerCase()}`,
          count: 1,
          severity: "failed",
          fixRoute,
          subline: `${stepName} step`,
        });
    }
    return Array.from(groups.values()).sort((a, b) => b.count - a.count);
  }, [jobs]);

  return {
    screen,
    setScreen,
    libraryMode,
    setLibraryMode,
    activityFilter,
    setActivityFilter,
    editingAssetId,
    setEditingAssetId,
    openEditor,
    closeEditor,
    attentionGroups,
    demandFloor,
    setDemandFloor,

    assets,
    assetTotal,
    status,
    setStatus,
    busy,
    busyLabel,
    refresh,
    removeAsset,
    needsReviewIds,
    assetFlags,
    toasts,
    dismissToast,

    isDragging,
    watch,
    incoming,
    setIncoming,
    queueAfterIngest,
    setQueueAfterIngest,
    lastIngestNote,
    addIncoming,
    commitIngest,
    clearIncoming,
    pickFiles,
    pickFolder,
    addWatchFolder,
    toggleWatch,
    removeWatchFolder,

    analysisConfig,
    activeAnalysisConfig,
    savedAnalysisConfigs,
    analysisConfigReady,
    analyses,
    analyzing,
    analyzeErrors,
    hasKey,
    patchAnalysisConfig,
    switchAnalysisProvider,
    saveAnalysisConfig,
    handleAnalyze,
    ollama,
    gpu,

    limiterPresets,
    builtInProfiles,
    activeProfile,
    profileReady,
    enabledProfileNames,
    enabledSites,
    toggleProfileEnabled,
    saveActiveProfile,
    createSiteProfile,
    deleteSiteProfile,

    metadata,
    generating,
    handleGenerateMetadata,
    handleSaveMetadata,
    handleSaveAndEmbed,
    refreshAssetResult,

    embedConfig,
    exiftool,
    embedding,
    saveEmbedConfig,
    runExiftoolCheck,
    handleEmbed,

    videoConfig,
    transcriptionConfig,
    ffmpeg,
    saveVideoConfig,
    runFfmpegCheck,

    keywordConfig,
    enriching,
    hasKeywordProvider,
    saveKeywordConfig,
    handleEnrich,

    jobs,
    jobCounts,
    selectedJobId,
    setSelectedJobId,
    handleEnqueueBatch,
    handleRetryJob,
    handleCancelJob,
    handleRemoveJobs,
    eligibleForBatch,
    handleExportCsv,
    csvExportedIds,
    csvLayouts,
    saveCsvLayout,

    sftpProfiles,
    selectedSftpProfileId,
    setSelectedSftpProfileId,
    uploading,
    handleUpload,
    addSftpProfile,
    editSftpProfile,
    removeSftpProfile,
    testSftpConnection,
  };
}

type SphinxApp = ReturnType<typeof useSphinxApp>;

const AppContext = createContext<SphinxApp | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const app = useSphinxApp();
  return <AppContext.Provider value={app}>{children}</AppContext.Provider>;
}

export function useApp(): SphinxApp {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used within AppProvider");
  return ctx;
}
