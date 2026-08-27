import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import {
  type Asset,
  type IngestSummary,
  assetCount,
  ingestFiles,
  ingestFolder,
  listAssets,
  onAssetsIngested,
  startWatch,
  stopWatch,
} from "./lib/api";

const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff", "webp", "bmp", "heic"];
const VIDEO_EXTENSIONS = ["mp4", "mov", "mkv", "avi", "webm", "m4v"];

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

export default function App() {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [total, setTotal] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [status, setStatus] = useState<string>("Drop images or video here, or use the buttons below.");
  const [watching, setWatching] = useState(false);
  const [watchDir, setWatchDir] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dropRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    const [rows, count] = await Promise.all([listAssets(200, 0), assetCount()]);
    setAssets(rows);
    setTotal(count);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

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

  return (
    <main className="app">
      <header className="app-header">
        <h1>Sphinx</h1>
        <p className="subtitle">Ingestion &amp; data layer (SPHIN-1)</p>
      </header>

      <div
        ref={dropRef}
        className={`dropzone${isDragging ? " dropzone--active" : ""}`}
      >
        <p>{isDragging ? "Release to ingest" : "Drag &amp; drop images or video"}</p>
        <div className="dropzone-actions">
          <button onClick={handlePickFiles} disabled={busy}>
            Choose files…
          </button>
          <button onClick={handlePickFolder} disabled={busy}>
            Import folder…
          </button>
          <button onClick={handleToggleWatch} disabled={busy} className={watching ? "btn-active" : ""}>
            {watching ? `Stop watching (${watchDir})` : "Watch folder…"}
          </button>
        </div>
      </div>

      <p className="status">{status}</p>

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
              <th>Hash</th>
              <th>Status</th>
              <th>Ingested</th>
            </tr>
          </thead>
          <tbody>
            {assets.map((asset) => (
              <tr key={asset.id}>
                <td className="path" title={asset.path}>
                  {asset.path}
                </td>
                <td>{asset.media_type}</td>
                <td>{formatBytes(asset.size)}</td>
                <td className="hash" title={asset.hash}>
                  {asset.hash.slice(0, 10)}…
                </td>
                <td>{asset.status}</td>
                <td>{new Date(asset.created_at).toLocaleString()}</td>
              </tr>
            ))}
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
