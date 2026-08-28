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
import { open } from "@tauri-apps/plugin-dialog";
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
  type TranscriptionConfig,
  type VideoConfig,
  DEFAULT_ANALYSIS_CONFIG,
  DEFAULT_EMBED_CONFIG,
  DEFAULT_KEYWORD_CONFIG,
  DEFAULT_TRANSCRIPTION_CONFIG,
  DEFAULT_VIDEO_CONFIG,
  analyzeAsset,
  assetCount,
  checkExiftool,
  checkFfmpeg,
  checkOllama,
  createSftpProfile,
  deleteSftpProfile,
  detectGpu,
  embedAssetMetadata,
  enqueueBatch,
  enrichKeywords,
  exportMetadataCsv,
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
  listSftpProfiles,
  onAssetsIngested,
  onJobUpdated,
  queueJobCounts,
  retryJob,
  setAnalysisConfig,
  setEmbedConfig,
  setKeywordConfig,
  setLimiterProfile,
  setMetadata,
  setTranscriptionConfig,
  setVideoConfig,
  startWatch,
  stopWatch,
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

export interface ToolStatus {
  checking: boolean;
  ok: boolean;
  detail: string;
}

const CHECKING: ToolStatus = { checking: true, ok: false, detail: "checking…" };

/** Pipeline stage derived from what's on record for an asset, 0-5. */
export type Stage = 0 | 1 | 2 | 3 | 4 | 5;
export const STAGE_NAMES = ["new", "Analyze", "Generate", "Enrich", "Embed", "Upload"] as const;

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

export type Screen = "import" | "library" | "editor" | "review" | "queue" | "sites" | "settings";

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
  host: string;
  port: number;
  username: string;
  remote_dir: string;
  password: string;
}

export const BLANK_SFTP_DRAFT: SftpDraft = {
  name: "",
  site: "generic",
  host: "",
  port: 22,
  username: "",
  remote_dir: "/",
  password: "",
};

function useSphinxApp() {
  const [screen, setScreen] = useState<Screen>("import");
  const [editingAssetId, setEditingAssetId] = useState<number | null>(null);
  // Session-only: the demand floor below which a keyword is flagged as weak.
  // Not yet persisted server-side (no per-project field for it today).
  const [demandFloor, setDemandFloor] = useState(20);

  const [assets, setAssets] = useState<Asset[]>([]);
  const [assetTotal, setAssetTotal] = useState(0);
  const [status, setStatus] = useState("Drop images or video to get started.");
  const [busy, setBusy] = useState(false);

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

  // --- job orchestration (SPHIN-7) ---
  const [jobs, setJobs] = useState<Job[]>([]);
  const [jobCounts, setJobCounts] = useState<JobCounts>({ pending: 0, running: 0, done: 0, failed: 0 });
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
    try {
      const summary = await ingestFiles(ready.map((f) => f.path));
      setStatus(summaryLine(summary));
      setIncoming([]);
      await refresh();
      if (queueAfterIngest) {
        const ids = summary.outcomes
          .filter((o) => o.outcome === "ingested")
          .map((o) => (o as { asset: Asset }).asset.id);
        if (ids.length) await handleEnqueueBatch("analyze", "Analyze", ids);
      }
    } catch (err) {
      setStatus(`Ingestion failed: ${String(err)}`);
    } finally {
      setBusy(false);
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
    try {
      const summary = await ingestFolder(selection);
      setStatus(`Folder import: ${summaryLine(summary)}`);
      await refresh();
    } catch (err) {
      setStatus(`Folder import failed: ${String(err)}`);
    } finally {
      setBusy(false);
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

  async function handleEmbed(asset: Asset) {
    setEmbedding((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const outcome = await embedAssetMetadata(PROJECT_ID, asset.id);
      setStatus(
        outcome.updated
          ? `Embedded metadata into ${fileName(asset.path)}.`
          : `Exiftool ran but reported no changes for ${fileName(asset.path)}.`
      );
      await refresh();
    } catch (err) {
      setStatus(`Embedding failed: ${String(err)}`);
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

  async function handleEnqueueBatch(jobType: QueueJobType, label: string, assetIds?: number[]) {
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
        jobType === "upload" ? selectedSftpProfileId ?? undefined : undefined
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

  async function handleExportCsv(assetIds: number[], targetPath: string) {
    await exportMetadataCsv(assetIds, targetPath);
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

  function openEditor(assetId: number) {
    setEditingAssetId(assetId);
    setScreen("editor");
  }

  const needsReviewIds = useMemo(
    () => assets.filter((a) => metadata[a.id] && a.status !== "uploaded").map((a) => a.id),
    [assets, metadata]
  );

  return {
    screen,
    setScreen,
    editingAssetId,
    setEditingAssetId,
    openEditor,
    demandFloor,
    setDemandFloor,

    assets,
    assetTotal,
    status,
    setStatus,
    busy,
    refresh,
    needsReviewIds,

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
    activeProfile,
    profileReady,
    enabledProfileNames,
    enabledSites,
    toggleProfileEnabled,
    saveActiveProfile,

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
    eligibleForBatch,
    handleExportCsv,

    sftpProfiles,
    selectedSftpProfileId,
    setSelectedSftpProfileId,
    uploading,
    handleUpload,
    addSftpProfile,
    editSftpProfile,
    removeSftpProfile,
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
