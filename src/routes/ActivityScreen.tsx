import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { NavBar } from "../components/NavBar";
import { StatusStrip, Sep } from "../components/StatusStrip";
import { SectionLabel } from "../components/Toggle";
import { ResizeHandle, usePanelWidth } from "../components/ResizeHandle";
import { useApp } from "../state/AppContext";
import { listJobEvents, type Job, type JobEvent } from "../lib/api";

const STEP_LABELS: Record<string, string> = {
  analyze: "Analyze (vision)",
  generate_metadata: "Generate metadata",
  enrich_keywords: "Enrich keywords",
  embed: "Embed (exiftool)",
  upload: "Upload (SFTP)",
};

const STATE_COLORS: Record<string, { bar: string; bg: string; fg: string }> = {
  running: { bar: "var(--accent)", bg: "rgba(47,111,237,.14)", fg: "var(--accent)" },
  pending: { bar: "var(--field)", bg: "#f0f2f5", fg: "var(--muted)" },
  failed: { bar: "#e0b3ae", bg: "rgba(179,38,30,.1)", fg: "var(--danger)" },
  done: { bar: "#9ecfab", bg: "var(--ok-tint)", fg: "var(--ok-ink)" },
  cancelled: { bar: "var(--faint)", bg: "#ececec", fg: "var(--faint)" },
};

type FilterKey = "all" | "running" | "done" | "failed";

const DETAIL_MIN_WIDTH = 376;

const isFinished = (status: string) => status === "done" || status === "failed" || status === "cancelled";

export function ActivityScreen() {
  const app = useApp();
  const [filter, setFilter] = useState<FilterKey>(app.activityFilter === "failed" ? "failed" : "all");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const detailPanel = usePanelWidth("sphinx.activity.detailWidth", DETAIL_MIN_WIDTH);

  useEffect(() => {
    if (app.activityFilter === "failed") {
      setFilter("failed");
      app.setActivityFilter(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [app.activityFilter]);

  const filtered = useMemo(() => {
    if (filter === "all") return app.jobs;
    return app.jobs.filter((j) => j.status === filter);
  }, [app.jobs, filter]);

  const selected = app.jobs.find((j) => j.id === selectedId) ?? null;
  const selectedAsset = selected ? app.assets.find((a) => a.id === selected.asset_id) : null;
  const failedCountForAsset = selected
    ? app.jobs.filter((j) => j.asset_id === selected.asset_id && j.status === "failed").length
    : 0;

  const [events, setEvents] = useState<JobEvent[]>([]);
  useEffect(() => {
    if (!selected) {
      setEvents([]);
      return;
    }
    let cancelled = false;
    void listJobEvents(selected.id).then((rows) => {
      if (!cancelled) setEvents(rows);
    });
    return () => {
      cancelled = true;
    };
    // Re-fetch whenever this job's own status/updated_at changes (a new
    // transition means a new event row), not just when selection changes.
  }, [selected?.id, selected?.updated_at]);

  function openAssetInLibrary(assetId: number) {
    app.setLibraryMode("browse");
    app.setScreen("library");
    app.openEditor(assetId);
  }

  return (
    <div className="app-shell">
      <NavBar
        right={
          <div className="navbar-right">
            <button className="btn-secondary" disabled title="Pausing the queue isn't supported yet">
              Pause queue
            </button>
            <button
              className="btn-primary"
              disabled={app.jobCounts.failed === 0}
              onClick={() => {
                for (const j of app.jobs.filter((x) => x.status === "failed")) void app.handleRetryJob(j);
              }}
            >
              Retry failed ({app.jobCounts.failed})
            </button>
          </div>
        }
      />
      <div className="queue-screen">
        <div className="stat-row">
          <StatCard value={String(app.jobCounts.running)} label="running" />
          <StatCard value={String(app.jobCounts.pending)} label="pending" />
          <StatCard value={String(app.jobCounts.done)} label="done" />
          <StatCard value={String(app.jobCounts.failed)} label="failed" danger={app.jobCounts.failed > 0} />
          <StatCard value={String(app.jobCounts.cancelled)} label="cancelled" />
          <StatCard value={String(app.jobs.length)} label="jobs total" />
        </div>

        <div className="queue-body">
          <div className="jobs-panel">
            <div className="jobs-header">
              <SectionLabel>Jobs</SectionLabel>
              <span className="navbar-spacer" />
              <div className="pill-select">
                <button className={`pill${filter === "all" ? " pill--active" : ""}`} onClick={() => setFilter("all")}>
                  All
                </button>
                <button className={`pill${filter === "running" ? " pill--active" : ""}`} onClick={() => setFilter("running")}>
                  Running
                </button>
                <button className={`pill${filter === "done" ? " pill--active" : ""}`} onClick={() => setFilter("done")}>
                  Successful {app.jobCounts.done}
                </button>
                <button className={`pill${filter === "failed" ? " pill--active" : ""}`} onClick={() => setFilter("failed")}>
                  Failed
                </button>
              </div>
              {filter === "done" && (
                <button
                  className="btn-secondary"
                  disabled={app.jobCounts.done === 0}
                  onClick={() => {
                    setSelectedId(null);
                    void app.handleRemoveJobs({ status: "done" });
                  }}
                >
                  Remove all successful
                </button>
              )}
            </div>
            <div className="jobs-body">
              {filtered.map((job) => {
                const asset = app.assets.find((a) => a.id === job.asset_id);
                const colors = STATE_COLORS[job.status] ?? STATE_COLORS.pending;
                const width = job.status === "done" || job.status === "failed" ? 100 : job.status === "running" ? 55 : 4;
                return (
                  <div
                    key={job.id}
                    className={`job-row${selectedId === job.id ? " job-row--selected" : ""}`}
                    onClick={() => setSelectedId(job.id)}
                  >
                    <div
                      className="job-thumb"
                      onDoubleClick={(e) => {
                        e.stopPropagation();
                        openAssetInLibrary(job.asset_id);
                      }}
                      title="Open in Library"
                    >
                      {asset?.media_type === "image" && <img src={convertFileSrc(asset.path)} alt="" style={{ width: "100%", height: "100%", objectFit: "cover", borderRadius: 5 }} />}
                      {asset?.media_type === "video" && (
                        <video
                          src={`${convertFileSrc(asset.path)}#t=0.1`}
                          muted
                          preload="metadata"
                          style={{ width: "100%", height: "100%", objectFit: "cover", borderRadius: 5 }}
                        />
                      )}
                    </div>
                    <div className="job-info">
                      <div className="job-name" title={asset?.path}>
                        {asset ? asset.path.split(/[\\/]/).pop() : `asset ${job.asset_id}`}
                      </div>
                      <div className="job-step">{STEP_LABELS[job.job_type] ?? job.job_type}</div>
                    </div>
                    <div className="job-progress">
                      <div className="job-progress-track">
                        <div className="job-progress-fill" style={{ width: `${width}%`, background: colors.bar }} />
                      </div>
                      <span className="job-progress-label">
                        {job.status === "running" ? `attempt ${job.attempts}` : job.status}
                      </span>
                    </div>
                    <span className="job-state" style={{ background: colors.bg, color: colors.fg }}>
                      {job.status}
                    </span>
                    <button
                      className="job-cancel"
                      disabled={job.status !== "pending" && job.status !== "running"}
                      onClick={(e) => {
                        e.stopPropagation();
                        void app.handleCancelJob(job);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                );
              })}
              {filtered.length === 0 && <div className="empty-state">No jobs in this view.</div>}
            </div>
          </div>

          <ResizeHandle width={detailPanel.width} minWidth={DETAIL_MIN_WIDTH} onResize={detailPanel.setWidth} />
          <div className="failure-panel" style={{ width: detailPanel.width }}>
            {!selected ? (
              <div className="empty-state">Select a job to see its log</div>
            ) : (
              <>
                <div className="failure-header">
                  <span
                    className="verdict-glyph"
                    style={{ background: STATE_COLORS[selected.status]?.bar ?? "var(--field)" }}
                  />
                  <button
                    className="btn-link"
                    style={{ fontSize: 12.5, fontWeight: 700, color: "var(--ink)" }}
                    onClick={() => openAssetInLibrary(selected.asset_id)}
                    title="Open in Library"
                  >
                    {selectedAsset ? selectedAsset.path.split(/[\\/]/).pop() : `asset ${selected.asset_id}`}
                  </button>
                  {selected.status === "failed" && failedCountForAsset > 1 && (
                    <span style={{ fontSize: 11, color: "var(--faint)" }}>failed {failedCountForAsset}×</span>
                  )}
                </div>
                <div className="failure-body">
                  {selected.status === "failed" && selected.error && (
                    <div className="error-callout">
                      <strong>{STEP_LABELS[selected.job_type] ?? selected.job_type} failed</strong>
                      <div>{selected.error}</div>
                    </div>
                  )}
                  <div>
                    <SectionLabel count={events.length || undefined}>Log</SectionLabel>
                    <div className="log-block" style={{ marginTop: 6 }}>
                      {events.length === 0 && <div>{selected.created_at} · job #{selected.id} created ({selected.source})</div>}
                      {events.map((ev) => (
                        <div key={ev.id} className={ev.status === "failed" ? "log-line--error" : undefined}>
                          {ev.at} · {ev.status} → {ev.message}
                        </div>
                      ))}
                    </div>
                  </div>
                  {selected.status === "failed" && (
                    <div>
                      <SectionLabel>Fix</SectionLabel>
                      <p style={{ fontSize: 12.5, color: "var(--muted)", marginTop: 6 }}>{fixSuggestion(selected)}</p>
                      {selected.job_type === "upload" && (
                        <button className="btn-secondary" onClick={() => app.setScreen("sites")}>
                          Open Sites
                        </button>
                      )}
                      {(selected.job_type === "analyze" || selected.job_type === "embed" || selected.job_type === "enrich_keywords") && (
                        <button className="btn-secondary" onClick={() => app.setScreen("connections")}>
                          Open Connections
                        </button>
                      )}
                    </div>
                  )}
                </div>
                <div className="failure-footer">
                  <button
                    className="btn-primary"
                    style={{ flex: 1 }}
                    disabled={selected.status !== "failed"}
                    onClick={() => void app.handleRetryJob(selected)}
                  >
                    Retry job
                  </button>
                  <button
                    className="btn-secondary"
                    disabled={!isFinished(selected.status)}
                    title={isFinished(selected.status) ? "Remove this job from Activity" : "Only finished jobs can be removed"}
                    onClick={() => {
                      setSelectedId(null);
                      void app.handleRemoveJobs({ jobIds: [selected.id] });
                    }}
                  >
                    Remove
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
      <StatusStrip>
        exiftool {app.exiftool.checking ? "checking…" : app.exiftool.ok ? "ready" : "missing"}
        <Sep />
        ffmpeg {app.ffmpeg.checking ? "checking…" : app.ffmpeg.ok ? "ready" : "missing"}
      </StatusStrip>
    </div>
  );
}

function StatCard({ value, label, danger }: { value: string; label: string; danger?: boolean }) {
  return (
    <div className="stat-card">
      <span className="stat-value" style={{ color: danger ? "var(--danger)" : "var(--ink)" }}>
        {value}
      </span>
      <span className="stat-label">{label}</span>
    </div>
  );
}

function fixSuggestion(job: Job): string {
  if (job.job_type === "analyze") return "Check the AI provider's API key and model under Connections → AI provider, then retry.";
  if (job.job_type === "embed") return "Point Sphinx at an exiftool binary in Connections → Local tools, then retry.";
  if (job.job_type === "upload") return "Check the site's delivery host/credentials under Sites → Delivery, then retry.";
  if (job.job_type === "enrich_keywords") return "Check the keyword-provider API keys under Connections → Keyword demand APIs, then retry.";
  return "Check the error above and retry.";
}
