import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";
import {
  type AnalysisConfig,
  type AnalysisResult,
  type Asset,
  type EmbedConfig,
  type GeneratedMetadata,
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
  createSftpProfile,
  deleteSftpProfile,
  embedAssetMetadata,
  enqueueBatch,
  enrichKeywords,
  exportMetadataCsv,
  generateMetadata,
  getAnalysis,
  getAnalysisConfig,
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
  setTranscriptionConfig,
  setVideoConfig,
  startWatch,
  stopWatch,
  uploadAsset,
} from "./lib/api";

const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff", "webp", "bmp", "heic"];
const VIDEO_EXTENSIONS = ["mp4", "mov", "mkv", "avi", "webm", "m4v"];

// Everything operates on the built-in "Default" project for now; a project
// switcher lands with job orchestration (SPHIN-7 / SPHIN-10).
const PROJECT_ID = 1;

const PROVIDER_LABELS: Record<ProviderKind, string> = {
  openai: "OpenAI (GPT-4o)",
  gemini: "Google Gemini",
  anthropic: "Anthropic Claude",
};

// Mirrors LimiterProfile::shutterstock() / ::default() on the Rust side, used
// only until the built-in list and any saved project profile have loaded.
const FALLBACK_LIMITER_PROFILE: LimiterProfile = {
  name: "Shutterstock",
  max_title_chars: 200,
  max_description_chars: 200,
  min_keywords: 7,
  max_keywords: 50,
  max_keyword_chars: 50,
};

// Vision models accept a narrower set of formats than we ingest.
const ANALYZABLE_EXTENSIONS = ["jpg", "jpeg", "png", "webp", "gif"];

function summaryLine(summary: IngestSummary): string {
  const parts = [`${summary.ingested} ingested`];
  if (summary.duplicates) parts.push(`${summary.duplicates} duplicate${summary.duplicates === 1 ? "" : "s"}`);
  if (summary.skipped) parts.push(`${summary.skipped} skipped`);
  if (summary.errors) parts.push(`${summary.errors} error${summary.errors === 1 ? "" : "s"}`);
  return parts.join(" · ");
}

function formatBytes(bytes: number): string {
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

function extensionOf(path: string): string {
  const dot = path.lastIndexOf(".");
  return dot === -1 ? "" : path.slice(dot + 1).toLowerCase();
}

export default function App() {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [total, setTotal] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [status, setStatus] = useState<string>("Drop images or video here, or use the buttons below.");
  const [watching, setWatching] = useState(false);
  const [watchDir, setWatchDir] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dropRef = useRef<HTMLDivElement>(null);

  // --- SPHIN-2 analysis state ---
  const [config, setConfig] = useState<AnalysisConfig>(DEFAULT_ANALYSIS_CONFIG);
  const [configReady, setConfigReady] = useState(false);
  const [configDirty, setConfigDirty] = useState(false);
  const [showConfig, setShowConfig] = useState(false);
  const [analyses, setAnalyses] = useState<Record<number, AnalysisResult>>({});
  const [analyzing, setAnalyzing] = useState<Record<number, boolean>>({});
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});

  // --- SPHIN-3 metadata generation state ---
  const [profiles, setProfiles] = useState<LimiterProfile[]>([FALLBACK_LIMITER_PROFILE]);
  const [profile, setProfile] = useState<LimiterProfile>(FALLBACK_LIMITER_PROFILE);
  const [profileReady, setProfileReady] = useState(false);
  const [profileDirty, setProfileDirty] = useState(false);
  const [showProfile, setShowProfile] = useState(false);
  const [generated, setGenerated] = useState<Record<number, GeneratedMetadata>>({});
  const [generating, setGenerating] = useState<Record<number, boolean>>({});

  // --- SPHIN-4 metadata embedding state ---
  const [embedConfig, setEmbedConfigState] = useState<EmbedConfig>(DEFAULT_EMBED_CONFIG);
  const [embedConfigReady, setEmbedConfigReady] = useState(false);
  const [embedConfigDirty, setEmbedConfigDirty] = useState(false);
  const [showEmbedConfig, setShowEmbedConfig] = useState(false);
  const [exiftoolStatus, setExiftoolStatus] = useState<string>("checking…");
  const [embedding, setEmbedding] = useState<Record<number, boolean>>({});

  // --- SPHIN-8 video pipeline state (keyframes + optional transcription) ---
  const [videoConfig, setVideoConfigState] = useState<VideoConfig>(DEFAULT_VIDEO_CONFIG);
  const [transcriptionConfig, setTranscriptionConfigState] =
    useState<TranscriptionConfig>(DEFAULT_TRANSCRIPTION_CONFIG);
  const [videoConfigReady, setVideoConfigReady] = useState(false);
  const [videoConfigDirty, setVideoConfigDirty] = useState(false);
  const [showVideoConfig, setShowVideoConfig] = useState(false);
  const [ffmpegStatus, setFfmpegStatus] = useState<string>("checking…");

  // --- SPHIN-5 keyword enrichment state ---
  const [keywordConfig, setKeywordConfigState] = useState<KeywordConfig>(DEFAULT_KEYWORD_CONFIG);
  const [keywordConfigReady, setKeywordConfigReady] = useState(false);
  const [keywordConfigDirty, setKeywordConfigDirty] = useState(false);
  const [showKeywordConfig, setShowKeywordConfig] = useState(false);
  const [enriching, setEnriching] = useState<Record<number, boolean>>({});

  // --- SPHIN-7 job orchestration state ---
  const [jobs, setJobs] = useState<Job[]>([]);
  const [jobCounts, setJobCounts] = useState<JobCounts>({ pending: 0, running: 0, done: 0, failed: 0 });
  const [showJobs, setShowJobs] = useState(false);

  // --- SPHIN-6 SFTP upload state ---
  const [sftpProfiles, setSftpProfiles] = useState<SftpProfile[]>([]);
  const [showSftpConfig, setShowSftpConfig] = useState(false);
  const [selectedProfileId, setSelectedProfileId] = useState<number | null>(null);
  const [newProfile, setNewProfile] = useState({
    name: "",
    site: "generic" as SftpSite,
    host: "",
    port: 22,
    username: "",
    remote_dir: "/",
    password: "",
  });
  const [uploading, setUploading] = useState<Record<number, boolean>>({});

  const refresh = useCallback(async () => {
    const [rows, count] = await Promise.all([listAssets(200, 0), assetCount()]);
    setAssets(rows);
    setTotal(count);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Backfill stored analyses for any assets we haven't loaded one for yet.
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

  useEffect(() => {
    getAnalysisConfig(PROJECT_ID)
      .then((saved) => {
        if (saved) setConfig({ ...DEFAULT_ANALYSIS_CONFIG, ...saved });
        setConfigReady(true);
        if (!saved) setShowConfig(true);
      })
      .catch(() => setConfigReady(true));
  }, []);

  useEffect(() => {
    listLimiterProfiles()
      .then((list) => {
        if (list.length) setProfiles(list);
      })
      .catch(() => {});
    getLimiterProfile(PROJECT_ID)
      .then((saved) => {
        if (saved) setProfile(saved);
        setProfileReady(true);
      })
      .catch(() => setProfileReady(true));
  }, []);

  const runExiftoolCheck = useCallback(async () => {
    setExiftoolStatus("checking…");
    try {
      const version = await checkExiftool(PROJECT_ID);
      setExiftoolStatus(`found (v${version})`);
    } catch (err) {
      setExiftoolStatus(`not found: ${String(err)}`);
    }
  }, []);

  useEffect(() => {
    getEmbedConfig(PROJECT_ID)
      .then((saved) => {
        setEmbedConfigState(saved ?? DEFAULT_EMBED_CONFIG);
        setEmbedConfigReady(true);
        void runExiftoolCheck();
      })
      .catch(() => setEmbedConfigReady(true));
  }, [runExiftoolCheck]);

  const runFfmpegCheck = useCallback(async () => {
    setFfmpegStatus("checking…");
    try {
      const version = await checkFfmpeg(PROJECT_ID);
      setFfmpegStatus(`found (${version})`);
    } catch (err) {
      setFfmpegStatus(`not found: ${String(err)}`);
    }
  }, []);

  useEffect(() => {
    Promise.all([getVideoConfig(PROJECT_ID), getTranscriptionConfig(PROJECT_ID)])
      .then(([savedVideo, savedTranscription]) => {
        setVideoConfigState(savedVideo ?? DEFAULT_VIDEO_CONFIG);
        setTranscriptionConfigState(savedTranscription ?? DEFAULT_TRANSCRIPTION_CONFIG);
        setVideoConfigReady(true);
        void runFfmpegCheck();
      })
      .catch(() => setVideoConfigReady(true));
  }, [runFfmpegCheck]);

  const refreshSftpProfiles = useCallback(async () => {
    const rows = await listSftpProfiles(PROJECT_ID);
    setSftpProfiles(rows);
    setSelectedProfileId((prev) => prev ?? rows[0]?.id ?? null);
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

  // A completed queue job may have produced a new analysis/metadata result
  // for an asset already marked "loaded" by the backfill effects below, so
  // fetch that asset's latest data directly rather than relying on them.
  const refreshAssetResult = useCallback(async (assetId: number) => {
    const [analysis, meta] = await Promise.all([getAnalysis(assetId), getMetadata(assetId)]);
    if (analysis) setAnalyses((prev) => ({ ...prev, [assetId]: analysis.result }));
    if (meta) setGenerated((prev) => ({ ...prev, [assetId]: meta.result }));
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
    getKeywordConfig(PROJECT_ID)
      .then((saved) => {
        setKeywordConfigState(saved ?? DEFAULT_KEYWORD_CONFIG);
        setKeywordConfigReady(true);
      })
      .catch(() => setKeywordConfigReady(true));
  }, []);

  // Backfill stored metadata for any assets we haven't loaded one for yet.
  const loadedMetadataIds = useRef<Set<number>>(new Set());
  useEffect(() => {
    const missing = assets.filter((a) => !loadedMetadataIds.current.has(a.id));
    if (missing.length === 0) return;
    for (const a of missing) loadedMetadataIds.current.add(a.id);
    void Promise.all(
      missing.map(async (a) => [a.id, (await getMetadata(a.id))?.result ?? null] as const)
    ).then((loaded) => {
      setGenerated((prev) => {
        const next = { ...prev };
        for (const [id, result] of loaded) if (result) next[id] = result;
        return next;
      });
    });
  }, [assets]);

  // Tauri delivers native OS drag-drop with real filesystem paths (unlike the
  // browser's File API, which never exposes a usable path) -- this is what
  // makes drag-drop ingestion of large video files possible without copying
  // them into the webview first.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setIsDragging(true);
        } else if (event.payload.type === "drop") {
          setIsDragging(false);
          void handleIngest(event.payload.paths);
        } else {
          setIsDragging(false);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onAssetsIngested((summary) => {
      setStatus(`Watch folder: ${summaryLine(summary)}`);
      void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refresh]);

  async function handleIngest(paths: string[]) {
    if (paths.length === 0) return;
    setBusy(true);
    try {
      const summary = await ingestFiles(paths);
      setStatus(summaryLine(summary));
      await refresh();
    } catch (err) {
      setStatus(`Ingestion failed: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  }

  async function handlePickFiles() {
    const selection = await open({
      multiple: true,
      filters: [
        { name: "Images", extensions: IMAGE_EXTENSIONS },
        { name: "Video", extensions: VIDEO_EXTENSIONS },
      ],
    });
    if (!selection) return;
    const paths = Array.isArray(selection) ? selection : [selection];
    await handleIngest(paths);
  }

  async function handlePickFolder() {
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

  async function handleToggleWatch() {
    if (watching) {
      await stopWatch();
      setWatching(false);
      setStatus("Folder watch stopped.");
      return;
    }
    const selection = await open({ directory: true });
    if (!selection || Array.isArray(selection)) return;
    await startWatch(selection);
    setWatchDir(selection);
    setWatching(true);
    setStatus(`Watching ${selection} for new files…`);
  }

  function patchConfig(patch: Partial<AnalysisConfig>) {
    setConfig((prev) => ({ ...prev, ...patch }));
    setConfigDirty(true);
  }

  async function handleSaveConfig() {
    try {
      await setAnalysisConfig(PROJECT_ID, config);
      setConfigDirty(false);
      setStatus(`Saved ${PROVIDER_LABELS[config.provider]} configuration.`);
    } catch (err) {
      setStatus(`Could not save configuration: ${String(err)}`);
    }
  }

  const hasKey = config.api_key.trim().length > 0;

  function patchProfile(patch: Partial<LimiterProfile>) {
    setProfile((prev) => ({ ...prev, ...patch }));
    setProfileDirty(true);
  }

  function handlePickProfilePreset(name: string) {
    const preset = profiles.find((p) => p.name === name);
    if (preset) {
      setProfile(preset);
      setProfileDirty(true);
    }
  }

  async function handleSaveProfile() {
    try {
      await setLimiterProfile(PROJECT_ID, profile);
      setProfileDirty(false);
      setStatus(`Saved "${profile.name}" limiter profile.`);
    } catch (err) {
      setStatus(`Could not save limiter profile: ${String(err)}`);
    }
  }

  async function handleGenerateMetadata(asset: Asset) {
    setGenerating((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const { result } = await generateMetadata(PROJECT_ID, asset.id);
      setGenerated((prev) => ({ ...prev, [asset.id]: result }));
      setExpanded((prev) => ({ ...prev, [asset.id]: true }));
      setStatus(`Generated metadata for "${profile.name}".`);
      await refresh();
    } catch (err) {
      setStatus(`Metadata generation failed: ${String(err)}`);
    } finally {
      setGenerating((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  function patchEmbedConfig(patch: Partial<EmbedConfig>) {
    setEmbedConfigState((prev) => ({ ...prev, ...patch }));
    setEmbedConfigDirty(true);
  }

  async function handleSaveEmbedConfig() {
    try {
      await setEmbedConfig(PROJECT_ID, embedConfig);
      setEmbedConfigDirty(false);
      setStatus("Saved exiftool configuration.");
      await runExiftoolCheck();
    } catch (err) {
      setStatus(`Could not save exiftool configuration: ${String(err)}`);
    }
  }

  async function handleEmbed(asset: Asset) {
    setEmbedding((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const outcome = await embedAssetMetadata(PROJECT_ID, asset.id);
      setStatus(
        outcome.updated
          ? `Embedded metadata into ${asset.path.split(/[\\/]/).pop()}.`
          : `Exiftool ran but reported no changes for ${asset.path.split(/[\\/]/).pop()}.`
      );
      await refresh();
    } catch (err) {
      setStatus(`Embedding failed: ${String(err)}`);
    } finally {
      setEmbedding((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  function patchVideoConfig(patch: Partial<VideoConfig>) {
    setVideoConfigState((prev) => ({ ...prev, ...patch }));
    setVideoConfigDirty(true);
  }

  function patchTranscriptionConfig(patch: Partial<TranscriptionConfig>) {
    setTranscriptionConfigState((prev) => ({ ...prev, ...patch }));
    setVideoConfigDirty(true);
  }

  async function handleSaveVideoConfig() {
    try {
      await Promise.all([
        setVideoConfig(PROJECT_ID, videoConfig),
        setTranscriptionConfig(PROJECT_ID, transcriptionConfig),
      ]);
      setVideoConfigDirty(false);
      setStatus("Saved video pipeline configuration.");
      await runFfmpegCheck();
    } catch (err) {
      setStatus(`Could not save video configuration: ${String(err)}`);
    }
  }

  async function handleExportCsv() {
    const assetIds = assets.filter((a) => generated[a.id]).map((a) => a.id);
    if (assetIds.length === 0) {
      setStatus("No assets have generated metadata to export yet.");
      return;
    }
    const targetPath = await save({
      defaultPath: "sphinx-metadata.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!targetPath) return;
    try {
      await exportMetadataCsv(assetIds, targetPath);
      setStatus(`Exported metadata for ${assetIds.length} asset(s) to ${targetPath}.`);
    } catch (err) {
      setStatus(`Export failed: ${String(err)}`);
    }
  }

  function toggleShutterstock(enabled: boolean) {
    setKeywordConfigState((prev) => ({
      ...prev,
      shutterstock: enabled ? (prev.shutterstock ?? { api_key: "", base_url: "" }) : null,
    }));
    setKeywordConfigDirty(true);
  }

  function toggleAdobeStock(enabled: boolean) {
    setKeywordConfigState((prev) => ({
      ...prev,
      adobe_stock: enabled ? (prev.adobe_stock ?? { api_key: "", base_url: "" }) : null,
    }));
    setKeywordConfigDirty(true);
  }

  function patchShutterstockKey(api_key: string) {
    setKeywordConfigState((prev) => ({
      ...prev,
      shutterstock: prev.shutterstock ? { ...prev.shutterstock, api_key } : { api_key, base_url: "" },
    }));
    setKeywordConfigDirty(true);
  }

  function patchAdobeStockKey(api_key: string) {
    setKeywordConfigState((prev) => ({
      ...prev,
      adobe_stock: prev.adobe_stock ? { ...prev.adobe_stock, api_key } : { api_key, base_url: "" },
    }));
    setKeywordConfigDirty(true);
  }

  async function handleSaveKeywordConfig() {
    try {
      await setKeywordConfig(PROJECT_ID, keywordConfig);
      setKeywordConfigDirty(false);
      setStatus("Saved keyword-enrichment configuration.");
    } catch (err) {
      setStatus(`Could not save keyword-enrichment configuration: ${String(err)}`);
    }
  }

  const hasKeywordProvider = Boolean(keywordConfig.shutterstock || keywordConfig.adobe_stock);

  async function handleEnrich(asset: Asset) {
    setEnriching((prev) => ({ ...prev, [asset.id]: true }));
    try {
      const { result, added, errors } = await enrichKeywords(PROJECT_ID, asset.id);
      setGenerated((prev) => ({ ...prev, [asset.id]: result }));
      setExpanded((prev) => ({ ...prev, [asset.id]: true }));
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

  async function handleCreateSftpProfile() {
    if (!newProfile.name.trim() || !newProfile.host.trim() || !newProfile.username.trim()) {
      setStatus("Name, host, and username are required for an SFTP profile.");
      return;
    }
    try {
      const { password, ...rest } = newProfile;
      await createSftpProfile(PROJECT_ID, rest, password);
      setNewProfile({ name: "", site: "generic", host: "", port: 22, username: "", remote_dir: "/", password: "" });
      setStatus(`Added SFTP profile "${rest.name}".`);
      await refreshSftpProfiles();
    } catch (err) {
      setStatus(`Could not add SFTP profile: ${String(err)}`);
    }
  }

  async function handleDeleteSftpProfile(profile: SftpProfile) {
    try {
      await deleteSftpProfile(profile.id);
      setStatus(`Deleted SFTP profile "${profile.name}".`);
      if (selectedProfileId === profile.id) setSelectedProfileId(null);
      await refreshSftpProfiles();
    } catch (err) {
      setStatus(`Could not delete SFTP profile: ${String(err)}`);
    }
  }

  async function handleUpload(asset: Asset) {
    if (!selectedProfileId) {
      setStatus("Select an SFTP profile first.");
      return;
    }
    setUploading((prev) => ({ ...prev, [asset.id]: true }));
    try {
      await uploadAsset(selectedProfileId, asset.id);
      setStatus(`Uploaded ${asset.path.split(/[\\/]/).pop()}.`);
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
        return assets.filter((a) => analyses[a.id] && !generated[a.id]);
      case "embed":
        return assets.filter((a) => generated[a.id]);
      case "enrich_keywords":
        return hasKeywordProvider ? assets.filter((a) => generated[a.id]) : [];
      case "upload":
        return selectedProfileId ? assets.filter((a) => generated[a.id]) : [];
    }
  }

  async function handleEnqueueBatch(jobType: QueueJobType, label: string) {
    const targets = eligibleForBatch(jobType);
    if (targets.length === 0) {
      setStatus(`No assets are eligible to queue for "${label}" right now.`);
      return;
    }
    try {
      await enqueueBatch(
        PROJECT_ID,
        targets.map((a) => a.id),
        jobType,
        jobType === "upload" ? selectedProfileId ?? undefined : undefined
      );
      setStatus(`Queued ${targets.length} asset(s) for "${label}".`);
      setShowJobs(true);
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

  async function handleAnalyze(asset: Asset) {
    setAnalyzing((prev) => ({ ...prev, [asset.id]: true }));
    setStatus(`Analyzing ${asset.path.split(/[\\/]/).pop()}…`);
    try {
      const { result } = await analyzeAsset(PROJECT_ID, asset.id);
      setAnalyses((prev) => ({ ...prev, [asset.id]: result }));
      setExpanded((prev) => ({ ...prev, [asset.id]: true }));
      setStatus(`Analyzed with ${result.provider}/${result.model}.`);
      await refresh();
    } catch (err) {
      setStatus(`Analysis failed: ${String(err)}`);
    } finally {
      setAnalyzing((prev) => ({ ...prev, [asset.id]: false }));
    }
  }

  return (
    <main className="app">
      <header className="app-header">
        <h1>Sphinx</h1>
        <p className="subtitle">
          Ingestion (SPHIN-1) · Media analysis (SPHIN-2) · Metadata generation (SPHIN-3) ·
          Metadata embedding (SPHIN-4) · Keyword enrichment (SPHIN-5) · Job orchestration (SPHIN-7) ·
          Upload & distribution (SPHIN-6) · Video pipeline (SPHIN-8)
        </p>
      </header>

      <div ref={dropRef} className={`dropzone${isDragging ? " dropzone--active" : ""}`}>
        <p>{isDragging ? "Release to ingest" : "Drag & drop images or video"}</p>
        <div className="dropzone-actions">
          <button onClick={handlePickFiles} disabled={busy}>
            Choose files…
          </button>
          <button onClick={handlePickFolder} disabled={busy}>
            Import folder…
          </button>
          <button
            onClick={handleToggleWatch}
            disabled={busy}
            className={watching ? "btn-active" : ""}
          >
            {watching ? `Stop watching (${watchDir})` : "Watch folder…"}
          </button>
        </div>
      </div>

      <p className="status">{status}</p>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            AI vision{" "}
            <span className="count">
              {configReady
                ? hasKey
                  ? `· ${PROVIDER_LABELS[config.provider]}`
                  : "· not configured"
                : "· loading…"}
            </span>
          </h2>
          <button onClick={() => setShowConfig((v) => !v)}>
            {showConfig ? "Hide" : "Configure"}
          </button>
        </div>

        {showConfig && (
          <div className="config-form">
            <label>
              Provider
              <select
                value={config.provider}
                onChange={(e) => patchConfig({ provider: e.target.value as ProviderKind })}
              >
                {(Object.keys(PROVIDER_LABELS) as ProviderKind[]).map((p) => (
                  <option key={p} value={p}>
                    {PROVIDER_LABELS[p]}
                  </option>
                ))}
              </select>
            </label>
            <label>
              API key
              <input
                type="password"
                autoComplete="off"
                placeholder="stored locally in sphinx.db"
                value={config.api_key}
                onChange={(e) => patchConfig({ api_key: e.target.value })}
              />
            </label>
            <label>
              Model <span className="hint">(blank = provider default)</span>
              <input
                type="text"
                placeholder="e.g. gpt-4o, gemini-1.5-pro, claude-3-5-sonnet-latest"
                value={config.model}
                onChange={(e) => patchConfig({ model: e.target.value })}
              />
            </label>
            <label>
              API base URL <span className="hint">(blank = provider default)</span>
              <input
                type="text"
                placeholder="proxy / gateway override"
                value={config.base_url}
                onChange={(e) => patchConfig({ base_url: e.target.value })}
              />
            </label>
            <label>
              Project prompt guidance <span className="hint">(optional)</span>
              <textarea
                rows={2}
                placeholder="e.g. Fine-art nature photography — prefer species names."
                value={config.prompt_extra}
                onChange={(e) => patchConfig({ prompt_extra: e.target.value })}
              />
            </label>
            <div className="config-actions">
              <button onClick={handleSaveConfig} disabled={!configDirty} className="btn-active">
                Save
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            Metadata limits <span className="count">· {profileReady ? profile.name : "loading…"}</span>
          </h2>
          <button onClick={() => setShowProfile((v) => !v)}>
            {showProfile ? "Hide" : "Configure"}
          </button>
        </div>

        {showProfile && (
          <div className="config-form">
            <label>
              Stock site preset
              <select value={profile.name} onChange={(e) => handlePickProfilePreset(e.target.value)}>
                {profiles.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name}
                  </option>
                ))}
                {!profiles.some((p) => p.name === profile.name) && (
                  <option value={profile.name}>{profile.name} (custom)</option>
                )}
              </select>
            </label>
            <label>
              Max title length
              <input
                type="number"
                min={1}
                value={profile.max_title_chars}
                onChange={(e) => patchProfile({ max_title_chars: Number(e.target.value) || 1 })}
              />
            </label>
            <label>
              Max description length
              <input
                type="number"
                min={1}
                value={profile.max_description_chars}
                onChange={(e) =>
                  patchProfile({ max_description_chars: Number(e.target.value) || 1 })
                }
              />
            </label>
            <label>
              Min keywords
              <input
                type="number"
                min={0}
                value={profile.min_keywords}
                onChange={(e) => patchProfile({ min_keywords: Number(e.target.value) || 0 })}
              />
            </label>
            <label>
              Max keywords
              <input
                type="number"
                min={1}
                value={profile.max_keywords}
                onChange={(e) => patchProfile({ max_keywords: Number(e.target.value) || 1 })}
              />
            </label>
            <div className="config-actions">
              <button onClick={handleSaveProfile} disabled={!profileDirty} className="btn-active">
                Save
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            File embedding{" "}
            <span className="count">
              · exiftool {embedConfigReady ? exiftoolStatus : "loading…"}
            </span>
          </h2>
          <button onClick={() => setShowEmbedConfig((v) => !v)}>
            {showEmbedConfig ? "Hide" : "Configure"}
          </button>
        </div>

        {showEmbedConfig && (
          <div className="config-form">
            <label>
              exiftool path <span className="hint">(blank = look up on PATH)</span>
              <input
                type="text"
                placeholder="e.g. C:\Tools\exiftool.exe"
                value={embedConfig.exiftool_path}
                onChange={(e) => patchEmbedConfig({ exiftool_path: e.target.value })}
              />
            </label>
            <div className="config-actions">
              <button onClick={handleSaveEmbedConfig} disabled={!embedConfigDirty} className="btn-active">
                Save
              </button>
              <button onClick={() => runExiftoolCheck()}>Re-check</button>
            </div>
          </div>
        )}
      </section>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            Video pipeline{" "}
            <span className="count">
              · ffmpeg {videoConfigReady ? ffmpegStatus : "loading…"}
            </span>
          </h2>
          <button onClick={() => setShowVideoConfig((v) => !v)}>
            {showVideoConfig ? "Hide" : "Configure"}
          </button>
        </div>

        {showVideoConfig && (
          <div className="config-form">
            <label>
              ffmpeg path <span className="hint">(blank = look up on PATH)</span>
              <input
                type="text"
                placeholder="e.g. C:\Tools\ffmpeg.exe"
                value={videoConfig.ffmpeg_path}
                onChange={(e) => patchVideoConfig({ ffmpeg_path: e.target.value })}
              />
            </label>
            <label>
              Max keyframes per video
              <input
                type="number"
                min={1}
                value={videoConfig.max_keyframes}
                onChange={(e) => patchVideoConfig({ max_keyframes: Number(e.target.value) || 1 })}
              />
            </label>
            <label>
              Scene-change threshold <span className="hint">(0.0-1.0, lower = more frames)</span>
              <input
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={videoConfig.scene_threshold}
                onChange={(e) => patchVideoConfig({ scene_threshold: Number(e.target.value) || 0 })}
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={transcriptionConfig.enabled}
                onChange={(e) => patchTranscriptionConfig({ enabled: e.target.checked })}
              />{" "}
              Transcribe audio (Whisper API) and include it in the analysis prompt
            </label>
            {transcriptionConfig.enabled && (
              <>
                <label>
                  Whisper API key
                  <input
                    type="password"
                    value={transcriptionConfig.api_key}
                    onChange={(e) => patchTranscriptionConfig({ api_key: e.target.value })}
                  />
                </label>
                <label>
                  Base URL <span className="hint">(blank = api.openai.com)</span>
                  <input
                    type="text"
                    placeholder="https://api.openai.com/v1"
                    value={transcriptionConfig.base_url}
                    onChange={(e) => patchTranscriptionConfig({ base_url: e.target.value })}
                  />
                </label>
                <label>
                  Model
                  <input
                    type="text"
                    value={transcriptionConfig.model}
                    onChange={(e) => patchTranscriptionConfig({ model: e.target.value })}
                  />
                </label>
              </>
            )}
            <div className="config-actions">
              <button onClick={handleSaveVideoConfig} disabled={!videoConfigDirty} className="btn-active">
                Save
              </button>
              <button onClick={() => runFfmpegCheck()}>Re-check</button>
            </div>
          </div>
        )}
      </section>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            Keyword enrichment{" "}
            <span className="count">
              ·{" "}
              {keywordConfigReady
                ? hasKeywordProvider
                  ? [keywordConfig.shutterstock && "Shutterstock", keywordConfig.adobe_stock && "Adobe Stock"]
                      .filter(Boolean)
                      .join(" + ")
                  : "not configured"
                : "loading…"}
            </span>
          </h2>
          <button onClick={() => setShowKeywordConfig((v) => !v)}>
            {showKeywordConfig ? "Hide" : "Configure"}
          </button>
        </div>

        {showKeywordConfig && (
          <div className="config-form">
            <label>
              <input
                type="checkbox"
                checked={keywordConfig.shutterstock !== null}
                onChange={(e) => toggleShutterstock(e.target.checked)}
              />{" "}
              Shutterstock
            </label>
            {keywordConfig.shutterstock && (
              <label>
                Shutterstock API token
                <input
                  type="password"
                  autoComplete="off"
                  value={keywordConfig.shutterstock.api_key}
                  onChange={(e) => patchShutterstockKey(e.target.value)}
                />
              </label>
            )}
            <label>
              <input
                type="checkbox"
                checked={keywordConfig.adobe_stock !== null}
                onChange={(e) => toggleAdobeStock(e.target.checked)}
              />{" "}
              Adobe Stock
            </label>
            {keywordConfig.adobe_stock && (
              <label>
                Adobe Stock API key
                <input
                  type="password"
                  autoComplete="off"
                  value={keywordConfig.adobe_stock.api_key}
                  onChange={(e) => patchAdobeStockKey(e.target.value)}
                />
              </label>
            )}
            <div className="config-actions">
              <button onClick={handleSaveKeywordConfig} disabled={!keywordConfigDirty} className="btn-active">
                Save
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            SFTP upload <span className="count">· {sftpProfiles.length} profile(s)</span>
          </h2>
          <button onClick={() => setShowSftpConfig((v) => !v)}>{showSftpConfig ? "Hide" : "Configure"}</button>
        </div>

        {showSftpConfig && (
          <div className="config-form">
            {sftpProfiles.map((p) => (
              <div key={p.id} className="dropzone-actions" style={{ justifyContent: "space-between" }}>
                <span>
                  {p.name} — {p.username}@{p.host}:{p.port}{p.remote_dir} ({p.site})
                  {!p.host_key_fingerprint && <span className="muted"> · not yet connected</span>}
                </span>
                <button className="link-btn" onClick={() => handleDeleteSftpProfile(p)}>
                  Delete
                </button>
              </div>
            ))}
            <label>
              Name
              <input
                type="text"
                placeholder="e.g. Adobe Stock"
                value={newProfile.name}
                onChange={(e) => setNewProfile((p) => ({ ...p, name: e.target.value }))}
              />
            </label>
            <label>
              Site
              <select
                value={newProfile.site}
                onChange={(e) => setNewProfile((p) => ({ ...p, site: e.target.value as SftpSite }))}
              >
                <option value="generic">Generic</option>
                <option value="adobe_stock">Adobe Stock</option>
              </select>
            </label>
            <label>
              Host
              <input
                type="text"
                placeholder="sftp.example.com"
                value={newProfile.host}
                onChange={(e) => setNewProfile((p) => ({ ...p, host: e.target.value }))}
              />
            </label>
            <label>
              Port
              <input
                type="number"
                value={newProfile.port}
                onChange={(e) => setNewProfile((p) => ({ ...p, port: Number(e.target.value) || 22 }))}
              />
            </label>
            <label>
              Username
              <input
                type="text"
                value={newProfile.username}
                onChange={(e) => setNewProfile((p) => ({ ...p, username: e.target.value }))}
              />
            </label>
            <label>
              Remote directory
              <input
                type="text"
                value={newProfile.remote_dir}
                onChange={(e) => setNewProfile((p) => ({ ...p, remote_dir: e.target.value }))}
              />
            </label>
            <label>
              Password <span className="hint">(stored in the OS credential store, not sphinx.db)</span>
              <input
                type="password"
                autoComplete="off"
                value={newProfile.password}
                onChange={(e) => setNewProfile((p) => ({ ...p, password: e.target.value }))}
              />
            </label>
            <div className="config-actions">
              <button onClick={handleCreateSftpProfile} className="btn-active">
                Add profile
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="analysis-config">
        <div className="analysis-config-head">
          <h2>
            Jobs{" "}
            <span className="count">
              · {jobCounts.pending} pending · {jobCounts.running} running · {jobCounts.done} done ·{" "}
              {jobCounts.failed} failed
            </span>
          </h2>
          <button onClick={() => setShowJobs((v) => !v)}>{showJobs ? "Hide" : "Show"}</button>
        </div>

        {showJobs && (
          <div className="config-form">
            <div className="dropzone-actions">
              <button onClick={() => handleEnqueueBatch("analyze", "Analyze")}>Queue: Analyze all new</button>
              <button onClick={() => handleEnqueueBatch("generate_metadata", "Generate metadata")}>
                Queue: Generate metadata
              </button>
              <button onClick={() => handleEnqueueBatch("embed", "Embed")}>Queue: Embed all</button>
              <button onClick={() => handleEnqueueBatch("enrich_keywords", "Enrich keywords")}>
                Queue: Enrich keywords
              </button>
              <select
                value={selectedProfileId ?? ""}
                onChange={(e) => setSelectedProfileId(e.target.value ? Number(e.target.value) : null)}
              >
                <option value="">Select SFTP profile…</option>
                {sftpProfiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
              <button
                disabled={!selectedProfileId}
                onClick={() => handleEnqueueBatch("upload", "Upload")}
              >
                Queue: Upload all
              </button>
            </div>
            <table>
              <thead>
                <tr>
                  <th>Asset</th>
                  <th>Type</th>
                  <th>Status</th>
                  <th>Error</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {jobs.map((job) => {
                  const asset = assets.find((a) => a.id === job.asset_id);
                  return (
                    <tr key={job.id}>
                      <td className="path" title={asset?.path}>
                        {asset ? asset.path.split(/[\\/]/).pop() : `asset ${job.asset_id}`}
                      </td>
                      <td>{job.job_type}</td>
                      <td>{job.status}</td>
                      <td>{job.error ?? "—"}</td>
                      <td>
                        {job.status === "failed" && (
                          <button className="link-btn" onClick={() => handleRetryJob(job)}>
                            Retry
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
                {jobs.length === 0 && (
                  <tr>
                    <td colSpan={5} className="empty">
                      No queued jobs yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="assets">
        <div className="analysis-config-head">
          <h2>
            Assets <span className="count">({total})</span>
          </h2>
          <button onClick={handleExportCsv}>Export CSV…</button>
        </div>
        <table>
          <thead>
            <tr>
              <th>Path</th>
              <th>Type</th>
              <th>Size</th>
              <th>Status</th>
              <th>Ingested</th>
              <th>Analysis</th>
              <th>Metadata</th>
            </tr>
          </thead>
          <tbody>
            {assets.map((asset) => {
              const result = analyses[asset.id];
              const meta = generated[asset.id];
              const isImage = asset.media_type === "image";
              const isVideo = asset.media_type === "video";
              // All ingested videos are already restricted to ffmpeg-supported
              // containers (see VIDEO_EXTENSIONS), so no extra whitelist is
              // needed for them the way ANALYZABLE_EXTENSIONS narrows images.
              const analyzable =
                (isImage && ANALYZABLE_EXTENSIONS.includes(extensionOf(asset.path))) || isVideo;
              return (
                <Fragment key={asset.id}>
                  <tr>
                    <td className="path" title={asset.path}>
                      {asset.path}
                    </td>
                    <td>{asset.media_type}</td>
                    <td>{formatBytes(asset.size)}</td>
                    <td>{asset.status}</td>
                    <td>{new Date(asset.created_at).toLocaleString()}</td>
                    <td className="analysis-cell">
                      {analyzing[asset.id] ? (
                        <span className="muted">analyzing…</span>
                      ) : (
                        <>
                          <button
                            className="link-btn"
                            disabled={!hasKey || !analyzable}
                            title={
                              !hasKey
                                ? "Configure an AI provider first"
                                : !analyzable
                                  ? "Only JPEG/PNG/WebP/GIF images can be analyzed"
                                  : ""
                            }
                            onClick={() => handleAnalyze(asset)}
                          >
                            {result ? "Re-analyze" : "Analyze"}
                          </button>
                          {result && (
                            <button
                              className="link-btn"
                              onClick={() =>
                                setExpanded((p) => ({ ...p, [asset.id]: !p[asset.id] }))
                              }
                            >
                              {expanded[asset.id] ? "Hide" : "View"}
                            </button>
                          )}
                        </>
                      )}
                    </td>
                    <td className="analysis-cell">
                      {generating[asset.id] ? (
                        <span className="muted">generating…</span>
                      ) : (
                        <>
                          <button
                            className="link-btn"
                            disabled={!result}
                            title={!result ? "Analyze the asset first" : ""}
                            onClick={() => handleGenerateMetadata(asset)}
                          >
                            {meta ? "Regenerate" : "Generate"}
                          </button>
                          {meta && (
                            <button
                              className="link-btn"
                              onClick={() =>
                                setExpanded((p) => ({ ...p, [asset.id]: !p[asset.id] }))
                              }
                            >
                              {expanded[asset.id] ? "Hide" : "View"}
                            </button>
                          )}
                          {meta &&
                            (embedding[asset.id] ? (
                              <span className="muted">embedding…</span>
                            ) : (
                              <button className="link-btn" onClick={() => handleEmbed(asset)}>
                                Embed
                              </button>
                            ))}
                          {meta &&
                            (enriching[asset.id] ? (
                              <span className="muted">enriching…</span>
                            ) : (
                              <button
                                className="link-btn"
                                disabled={!hasKeywordProvider}
                                title={!hasKeywordProvider ? "Configure a keyword provider first" : ""}
                                onClick={() => handleEnrich(asset)}
                              >
                                Enrich
                              </button>
                            ))}
                          {meta &&
                            (uploading[asset.id] ? (
                              <span className="muted">uploading…</span>
                            ) : (
                              <button
                                className="link-btn"
                                disabled={!selectedProfileId}
                                title={!selectedProfileId ? "Select an SFTP profile first" : ""}
                                onClick={() => handleUpload(asset)}
                              >
                                Upload
                              </button>
                            ))}
                        </>
                      )}
                    </td>
                  </tr>
                  {(result || meta) && expanded[asset.id] && (
                    <tr className="analysis-detail-row">
                      <td colSpan={7}>
                        {result && <AnalysisDetail result={result} />}
                        {meta && <MetadataDetail meta={meta} />}
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
            {assets.length === 0 && (
              <tr>
                <td colSpan={7} className="empty">
                  No assets ingested yet.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </section>
    </main>
  );
}

function AnalysisDetail({ result }: { result: AnalysisResult }) {
  return (
    <div className="analysis-detail">
      <p className="analysis-description">{result.description}</p>
      <dl>
        <div>
          <dt>Scene</dt>
          <dd>{result.scene || "—"}</dd>
        </div>
        <div>
          <dt>Mood</dt>
          <dd>{result.mood || "—"}</dd>
        </div>
        <div>
          <dt>Subjects</dt>
          <dd>{result.subjects.join(", ") || "—"}</dd>
        </div>
        <div>
          <dt>Colors</dt>
          <dd>{result.colors.join(", ") || "—"}</dd>
        </div>
        <div>
          <dt>Licensing</dt>
          <dd>
            {result.editorial ? (
              <span className="badge badge-warn">editorial only</span>
            ) : (
              <span className="badge badge-ok">commercial-safe</span>
            )}
          </dd>
        </div>
        {result.text_content && (
          <div>
            <dt>Text in image</dt>
            <dd>“{result.text_content}”</dd>
          </div>
        )}
      </dl>
      <div className="keyword-chips">
        {result.keywords.map((kw) => (
          <span key={kw} className="chip">
            {kw}
          </span>
        ))}
      </div>
      <p className="provenance">
        {result.provider} / {result.model} · {result.keywords.length} keywords
      </p>
    </div>
  );
}

function MetadataDetail({ meta }: { meta: GeneratedMetadata }) {
  return (
    <div className="analysis-detail metadata-detail">
      <dl>
        <div>
          <dt>Title</dt>
          <dd>{meta.title}</dd>
        </div>
        <div>
          <dt>Description</dt>
          <dd>{meta.description}</dd>
        </div>
      </dl>
      <div className="keyword-chips">
        {meta.keywords.map((kw) => (
          <span key={kw} className="chip">
            {kw}
          </span>
        ))}
      </div>
      <p className="provenance">
        {meta.profile} profile · {meta.keywords.length} keywords
        {!meta.meets_minimum_keywords && (
          <span className="badge badge-warn" style={{ marginLeft: "0.5rem" }}>
            below site minimum
          </span>
        )}
      </p>
    </div>
  );
}
