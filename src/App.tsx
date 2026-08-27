import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import {
  type AnalysisConfig,
  type AnalysisResult,
  type Asset,
  type IngestSummary,
  type ProviderKind,
  DEFAULT_ANALYSIS_CONFIG,
  analyzeAsset,
  assetCount,
  getAnalysis,
  getAnalysisConfig,
  ingestFiles,
  ingestFolder,
  listAssets,
  onAssetsIngested,
  setAnalysisConfig,
  startWatch,
  stopWatch,
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
        <p className="subtitle">Ingestion (SPHIN-1) · Media analysis (SPHIN-2)</p>
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

      <section className="assets">
        <h2>
          Assets <span className="count">({total})</span>
        </h2>
        <table>
          <thead>
            <tr>
              <th>Path</th>
              <th>Type</th>
              <th>Size</th>
              <th>Status</th>
              <th>Ingested</th>
              <th>Analysis</th>
            </tr>
          </thead>
          <tbody>
            {assets.map((asset) => {
              const result = analyses[asset.id];
              const isImage = asset.media_type === "image";
              const analyzable =
                isImage && ANALYZABLE_EXTENSIONS.includes(extensionOf(asset.path));
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
                  </tr>
                  {result && expanded[asset.id] && (
                    <tr className="analysis-detail-row">
                      <td colSpan={6}>
                        <AnalysisDetail result={result} />
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
            {assets.length === 0 && (
              <tr>
                <td colSpan={6} className="empty">
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
