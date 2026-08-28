import { NavBar } from "../components/NavBar";
import { StatusStrip, Sep } from "../components/StatusStrip";
import { SectionLabel } from "../components/Toggle";
import { useApp } from "../state/AppContext";
import type { IngestVerdict } from "../state/AppContext";

const VERDICT_COLOR: Record<IngestVerdict, string> = {
  ready: "var(--ok)",
  duplicate: "var(--warn)",
  "needs-ffmpeg": "var(--warn)",
  unsupported: "var(--danger)",
};

const VERDICT_TEXT: Record<IngestVerdict, string> = {
  ready: "ready",
  duplicate: "duplicate hash",
  "needs-ffmpeg": "needs ffmpeg",
  unsupported: "unsupported",
};

export function ImportScreen() {
  const app = useApp();
  const readyCount = app.incoming.filter((f) => f.verdict === "ready").length;

  return (
    <div className="app-shell">
      <NavBar
        right={
          <div className="navbar-right">
            <span className="status-pill">
              <span
                className="status-pill-dot"
                style={{ background: app.hasKey ? "var(--ok)" : "var(--warn)" }}
              />
              {app.hasKey && app.activeAnalysisConfig
                ? `${app.activeAnalysisConfig.provider} vision`
                : "AI vision not configured"}
            </span>
            <span className="status-pill">
              <span
                className="status-pill-dot"
                style={{ background: app.ffmpeg.checking ? "var(--off)" : app.ffmpeg.ok ? "var(--ok)" : "var(--warn)" }}
              />
              {app.ffmpeg.checking ? "checking ffmpeg…" : app.ffmpeg.ok ? "ffmpeg ready" : "ffmpeg missing"}
            </span>
          </div>
        }
      />
      <div className="import-screen">
        <div className="import-left">
          <div
            className={`dropzone${app.isDragging ? " dropzone--active" : ""}`}
            onDragOver={(e) => e.preventDefault()}
          >
            <div className="dropzone-icon">↓</div>
            <p className="dropzone-headline">
              {app.isDragging ? "Release to add to Incoming" : "Drop images, videos or folders here"}
            </p>
            <p className="dropzone-sub">
              JPEG, PNG, TIFF (flattened), MP4, MOV · files are hashed on ingest so duplicates never
              re-bill the model
            </p>
            <div className="dropzone-actions">
              <button className="btn-primary" onClick={() => void app.pickFiles()} disabled={app.busy}>
                Choose files…
              </button>
              <button className="btn-secondary" onClick={() => void app.pickFolder()} disabled={app.busy}>
                Import folder…
              </button>
            </div>
          </div>

          <div className="panel watch-panel">
            <div className="panel-header">
              <SectionLabel>Watch folders</SectionLabel>
              <span className="navbar-spacer" />
              {app.watch && (
                <button className="btn-link" onClick={() => void app.toggleWatch()}>
                  Scan all now
                </button>
              )}
            </div>
            <div className="panel-body">
              {app.watch ? (
                <div className="watch-row">
                  <span
                    className="watch-dot"
                    style={{ background: app.watch.active ? "var(--ok)" : "var(--off)" }}
                  />
                  <span className="watch-path">{app.watch.path}</span>
                  <span className="watch-meta">{app.watch.active ? "watching" : "paused"}</span>
                  <button className="outline-pill" onClick={() => void app.toggleWatch()}>
                    {app.watch.active ? "On" : "Off"}
                  </button>
                  <button className="btn-link" style={{ color: "var(--faint)" }} onClick={app.removeWatchFolder}>
                    Remove
                  </button>
                </div>
              ) : (
                <div className="empty-state" style={{ height: "100%" }}>
                  No watch folder configured. Sphinx currently watches one folder at a time.
                </div>
              )}
            </div>
            {!app.watch && (
              <div className="panel-footer" style={{ borderTop: "none" }}>
                <button className="btn-secondary" onClick={() => void app.addWatchFolder()}>
                  Add watch folder…
                </button>
              </div>
            )}
          </div>
        </div>

        <div className="import-right">
          <div className="panel-header">
            <span style={{ fontSize: 13, fontWeight: 700 }}>Incoming</span>
            <span style={{ fontSize: 12, color: "var(--faint)" }}>{app.incoming.length} files</span>
            <span className="navbar-spacer" />
            <span style={{ fontSize: 11.5, color: "var(--faint)" }}>pre-flight</span>
          </div>
          <div className="panel-body">
            {app.incoming.length === 0 && (
              <div className="empty-state">Drop files or use Choose files… to see them here first.</div>
            )}
            {app.incoming.map((f, i) => (
              <div key={`${f.path}-${i}`} className="incoming-row">
                <div className="incoming-thumb" />
                <div className="incoming-info">
                  <div className="incoming-name" title={f.path}>
                    {f.name}
                  </div>
                  <div className="incoming-meta">{f.detail}</div>
                </div>
                <span className="verdict" style={{ color: VERDICT_COLOR[f.verdict] }}>
                  <span className="verdict-glyph" style={{ background: VERDICT_COLOR[f.verdict] }} />
                  {VERDICT_TEXT[f.verdict]}
                </span>
              </div>
            ))}
          </div>
          <div className="panel-footer" style={{ flexDirection: "column", alignItems: "stretch", gap: 9 }}>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={app.queueAfterIngest}
                onChange={(e) => app.setQueueAfterIngest(e.target.checked)}
              />
              Queue analysis immediately after ingest
            </label>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                className="btn-primary"
                style={{ flex: 1 }}
                disabled={readyCount === 0 || app.busy}
                onClick={() => void app.commitIngest()}
              >
                Ingest {readyCount} eligible
              </button>
              <button className="btn-secondary" onClick={app.clearIncoming} disabled={app.incoming.length === 0}>
                Clear
              </button>
            </div>
          </div>
        </div>
      </div>
      <StatusStrip>
        {app.lastIngestNote && (
          <>
            {app.lastIngestNote}
            <Sep />
          </>
        )}
        sphinx.db
      </StatusStrip>
    </div>
  );
}
