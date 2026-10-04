import { NavBar } from "../components/NavBar";
import { StatusStrip, Sep } from "../components/StatusStrip";
import { useApp, PROVIDER_LABELS } from "../state/AppContext";

export function HomeScreen() {
  const app = useApp();
  const failed = app.jobCounts.failed;
  const uploadedTotal = app.assets.filter((a) => a.status === "uploaded").length;
  const enabledSiteCount = app.enabledProfileNames.size;

  function goToTriage() {
    app.setLibraryMode("triage");
    app.setScreen("library");
  }

  function goToActivity(filter?: "failed") {
    if (filter) app.setActivityFilter(filter);
    app.setScreen("activity");
  }

  return (
    <div className="app-shell">
      <NavBar right={<button className="btn-primary" onClick={() => app.setScreen("import")}>Import files…</button>} />
      <div className="home-screen">
        <div className="home-header">
          <div>
            <div className="home-greeting">Good morning</div>
            <div className="home-subline">Here's where things stand in Default project.</div>
          </div>
        </div>

        <div className="stat-grid">
          <div className="home-stat-card">
            <span className="section-label">Needs review</span>
            <span className="home-stat-value">{app.needsReviewIds.length}</span>
            <button className="btn-link" onClick={goToTriage}>
              Start review →
            </button>
          </div>
          <div className="home-stat-card">
            <span className="section-label">In flight</span>
            <span className="home-stat-value">{app.jobCounts.running} running</span>
            <button className="btn-link" onClick={() => goToActivity()}>
              View activity →
            </button>
          </div>
          <div className="home-stat-card">
            <span className="section-label">Failed</span>
            <span className="home-stat-value" style={{ color: failed > 0 ? "var(--danger)" : undefined }}>
              {failed}
            </span>
            <button className="btn-link" onClick={() => goToActivity("failed")}>
              Review failures →
            </button>
          </div>
          <div className="home-stat-card">
            <span className="section-label">Uploaded</span>
            <span className="home-stat-value">{uploadedTotal}</span>
            <span className="home-stat-note">across {enabledSiteCount} site{enabledSiteCount === 1 ? "" : "s"}</span>
          </div>
        </div>

        <div className="home-split">
          <div className="home-attention card">
            <div className="home-attention-head">
              <span className="section-label">Needs your attention</span>
              <button className="btn-link" onClick={() => goToActivity()}>
                View all in Activity →
              </button>
            </div>
            <div className="home-attention-body">
              {app.attentionGroups.length === 0 && (
                <div className="empty-state">Nothing needs attention right now.</div>
              )}
              {app.attentionGroups.map((g) => (
                <div className="attention-row" key={g.title}>
                  <span className={`attention-dot attention-dot--${g.severity}`} />
                  <div className="attention-row-body">
                    <div className="attention-row-title">
                      {g.title}
                      {g.count > 1 ? ` — blocking ${g.count} assets` : ""}
                    </div>
                    <div className="attention-row-sub">{g.subline}</div>
                  </div>
                  <button className="btn-link attention-row-fix" onClick={() => app.setScreen(g.fixRoute)}>
                    {g.fixRoute === "connections"
                      ? "Fix in Connections →"
                      : g.fixRoute === "sites"
                        ? "Fix in Sites →"
                        : "Open in Activity →"}
                  </button>
                </div>
              ))}
            </div>
          </div>

          <div className="home-side">
            <div className="card home-side-card">
              <span className="section-label">Tool &amp; provider health</span>
              <HealthRow ok={app.hasKey} checking={false} label={PROVIDER_LABELS[app.analysisConfig.provider]} detail={app.hasKey ? "connected" : "not configured"} />
              <HealthRow ok={app.exiftool.ok} checking={app.exiftool.checking} label="exiftool" detail={app.exiftool.checking ? "checking…" : app.exiftool.ok ? app.exiftool.detail : "not found"} />
              <HealthRow ok={app.ffmpeg.ok} checking={app.ffmpeg.checking} label="ffmpeg" detail={app.ffmpeg.checking ? "checking…" : app.ffmpeg.ok ? app.ffmpeg.detail : "not found"} />
              <HealthRow
                ok={app.analysisConfig.provider === "ollama" && app.ollama.ok}
                checking={false}
                neutral={app.analysisConfig.provider !== "ollama"}
                label="Ollama (local)"
                detail={app.analysisConfig.provider !== "ollama" ? "not in use" : app.ollama.ok ? app.ollama.detail : "not reachable"}
              />
              <button className="btn-link" onClick={() => app.setScreen("connections")}>
                Manage in Connections →
              </button>
            </div>

            <div className="card home-side-card">
              <span className="section-label">Watch folder</span>
              {app.watch ? (
                <>
                  <div className="home-watch-path">
                    <span className={`attention-dot attention-dot--${app.watch.active ? "ok" : "degraded"}`} />
                    {app.watch.path}
                  </div>
                  <div className="home-watch-note">{app.watch.active ? "watching" : "paused"}{app.lastIngestNote ? ` · ${app.lastIngestNote}` : ""}</div>
                </>
              ) : (
                <div className="home-watch-note">No watch folder configured yet.</div>
              )}
              <button className="btn-link" onClick={() => app.setScreen("import")}>
                Open Import →
              </button>
            </div>
          </div>
        </div>
      </div>
      <StatusStrip>
        {app.assetTotal} assets in Default project
        <Sep />
        {app.status}
      </StatusStrip>
    </div>
  );
}

function HealthRow({
  ok,
  checking,
  neutral,
  label,
  detail,
}: {
  ok: boolean;
  checking: boolean;
  neutral?: boolean;
  label: string;
  detail: string;
}) {
  const color = checking ? "var(--off)" : neutral ? "var(--off)" : ok ? "var(--ok)" : "var(--warn)";
  const textColor = checking ? "var(--faint)" : neutral ? "var(--faint)" : ok ? "var(--ok-ink)" : "var(--warn-ink)";
  return (
    <div className="health-row">
      <span className="health-row-label">
        <span className="health-dot" style={{ background: color }} />
        {label}
      </span>
      <span className="health-row-detail" style={{ color: textColor }}>
        {detail}
      </span>
    </div>
  );
}
