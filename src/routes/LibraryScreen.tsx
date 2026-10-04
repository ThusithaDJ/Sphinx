import { useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { NavBar } from "../components/NavBar";
import { StatusStrip, Sep } from "../components/StatusStrip";
import { StageDots } from "../components/StageDots";
import { ResizeHandle, usePanelWidth } from "../components/ResizeHandle";
import { CompactChip } from "../components/KeywordChip";
import { ComplianceTable } from "../components/ComplianceTable";
import { SectionLabel } from "../components/Toggle";
import { estimateKeywordScores, sortKeywordsByHeat } from "../lib/heat";
import {
  useApp,
  deliveryProfileFor,
  extensionOf,
  formatBytes,
  stageOf,
  ASSET_FLAG_LABEL,
  pipelineStops,
  stopLabel,
  PIPELINE_STEP_NAMES,
  ANALYZABLE_EXTENSIONS,
  type LibraryMode,
  type PipelineStop,
} from "../state/AppContext";
import { LibraryTriage } from "./LibraryTriage";
import { LibraryAssetDetail } from "./LibraryAssetDetail";

const INSPECTOR_MIN_WIDTH = 392;

type FilterKey = "all" | "new" | "review" | "ready" | "failed";

function ModeSwitch({ mode, onChange }: { mode: LibraryMode; onChange: (m: LibraryMode) => void }) {
  return (
    <div className="mode-switch">
      <button className={`mode-switch-btn${mode === "browse" ? " mode-switch-btn--active" : ""}`} onClick={() => onChange("browse")}>
        {mode === "triage" ? "← Browse" : "Browse"}
      </button>
      <button className={`mode-switch-btn${mode === "triage" ? " mode-switch-btn--active" : ""}`} onClick={() => onChange("triage")}>
        Triage
      </button>
    </div>
  );
}

export function LibraryScreen() {
  const app = useApp();

  if (app.editingAssetId != null) return <LibraryAssetDetail />;
  if (app.libraryMode === "triage") return <LibraryTriageShell />;
  return <LibraryBrowse />;
}

function LibraryTriageShell() {
  const app = useApp();
  const [progress, setProgress] = useState({ reviewed: 0, total: 0 });

  return (
    <div className="app-shell">
      <NavBar
        right={
          <div className="navbar-right">
            <span style={{ fontSize: 12, color: "var(--muted)" }}>
              {progress.reviewed} of {progress.total} reviewed
            </span>
            <div className="review-progress-track">
              <div
                className="review-progress-fill"
                style={{ width: `${progress.total ? (progress.reviewed / progress.total) * 100 : 0}%` }}
              />
            </div>
          </div>
        }
      />
      <div className="library-mode-bar">
        <ModeSwitch mode="triage" onChange={app.setLibraryMode} />
      </div>
      <LibraryTriage onProgress={(reviewed, total) => setProgress({ reviewed, total })} />
      <StatusStrip>J / K move · ⏎ approve · R reject · {Math.max(0, progress.total - progress.reviewed)} assets left in queue</StatusStrip>
    </div>
  );
}

function LibraryBrowse() {
  const app = useApp();
  const [query, setQuery] = useState("");
  // Freshly imported assets surface first when the Library opens.
  const [filter, setFilter] = useState<FilterKey>(() =>
    app.assets.some((a) => app.assetFlags.get(a.id) === "new") ? "new" : "review"
  );
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [inspectedId, setInspectedId] = useState<number | null>(null);
  const [targetSite, setTargetSite] = useState<string>("");
  const [pipelineExpanded, setPipelineExpanded] = useState(false);

  // Every candidate must still be a listed site, since any profile can be deleted.
  const targetName =
    [targetSite, app.activeProfile?.name].find((n) => n && app.limiterPresets.some((p) => p.name === n)) ??
    app.limiterPresets[0]?.name ??
    "";

  const flags = app.assetFlags;
  const inspectorPanel = usePanelWidth("sphinx.library.inspectorWidth", INSPECTOR_MIN_WIDTH);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return app.assets.filter((a) => {
      if (q) {
        const title = app.metadata[a.id]?.title.toLowerCase() ?? "";
        const kws = app.metadata[a.id]?.keywords.join(" ").toLowerCase() ?? "";
        if (!a.path.toLowerCase().includes(q) && !title.includes(q) && !kws.includes(q)) return false;
      }
      if (filter === "all") return true;
      if (filter === "new") return flags.get(a.id) === "new";
      if (filter === "failed") return flags.get(a.id) === "failed";
      if (filter === "ready") return flags.get(a.id) === "ready";
      if (filter === "review") return flags.get(a.id) === "review";
      return true;
    });
  }, [app.assets, app.metadata, query, filter, flags]);

  const counts = useMemo(() => {
    let fresh = 0,
      review = 0,
      ready = 0,
      failed = 0;
    for (const a of app.assets) {
      const f = flags.get(a.id);
      if (f === "new") fresh++;
      else if (f === "review") review++;
      else if (f === "ready") ready++;
      else if (f === "failed") failed++;
    }
    return { fresh, review, ready, failed };
  }, [app.assets, flags]);

  function toggleSelect(id: number, additive: boolean) {
    setSelected((prev) => {
      const next = additive ? new Set(prev) : new Set<number>();
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    setInspectedId(id);
  }

  const inspected = app.assets.find((a) => a.id === inspectedId) ?? null;
  const inspectedMeta = inspected ? app.metadata[inspected.id] : undefined;
  const inspectedAnalysis = inspected ? app.analyses[inspected.id] : undefined;
  const inspectedKeywords = inspectedMeta ? sortKeywordsByHeat(estimateKeywordScores(inspectedMeta.keywords)) : [];

  const selectedIds = selected.size > 0 ? Array.from(selected) : inspected ? [inspected.id] : [];
  const pipelineSelection = app.assets.filter((a) => selectedIds.includes(a.id));
  const stops = pipelineStops(app, selectedIds, targetName);
  const targetTransport = deliveryProfileFor(app.sftpProfiles, targetName);

  function runStop(stop: PipelineStop) {
    if (stop.step === "export_csv") {
      void app.handleExportCsv(selectedIds, targetName);
      return;
    }
    if (stop.step === "upload") {
      if (!targetTransport) return;
      void app.handleEnqueueBatch("upload", `Upload to ${targetName}`, selectedIds, targetTransport.id);
      return;
    }
    void app.handleEnqueueBatch(stop.step, PIPELINE_STEP_NAMES[stop.step], selectedIds, undefined, targetName);
  }

  const nextStop = stops.find((st) => st.state === "next") ?? null;
  const doneStops = stops.filter((st) => st.state === "done").length;

  function renderStop(stop: PipelineStop) {
    return (
      <div className={`pipeline-stop pipeline-stop--${stop.state}`} key={stop.step}>
        <span className={`pipeline-stop-circle pipeline-stop-circle--${stop.state}`}>
          {stop.state === "done" ? "✓" : stop.state === "next" ? stopIcon(stop.step) : "·"}
        </span>
        <div className="pipeline-stop-body">
          <div className="pipeline-stop-name">{PIPELINE_STEP_NAMES[stop.step]}</div>
          <div className="pipeline-stop-sub">
            {stopSubline(stop, targetName, targetTransport?.protocol.toUpperCase())}
          </div>
        </div>
        <button
          className={`pipeline-stop-button ${stop.state === "next" ? "btn-primary" : "btn-secondary"}`}
          disabled={
            stop.state === "locked" ||
            selectedIds.length === 0 ||
            (stop.step === "upload" && Boolean(stop.blockedReason))
          }
          onClick={() => runStop(stop)}
        >
          {stopLabel(stop.step, stop.state)}
        </button>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <NavBar
        right={
          <button className="btn-primary" onClick={() => app.setScreen("import")}>
            Import…
          </button>
        }
      />
      <div className="library-screen">
        <div className="toolbar">
          <ModeSwitch mode="browse" onChange={app.setLibraryMode} />
          <div className="search-box">
            <span className="search-glyph">⌕</span>
            <input placeholder="Search title, keyword, path" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          <div className="pill-select">
            <button className={`pill${filter === "new" ? " pill--active" : ""}`} onClick={() => setFilter("new")}>
              New {counts.fresh}
            </button>
            <button className={`pill${filter === "review" ? " pill--active" : ""}`} onClick={() => setFilter("review")}>
              Needs review {counts.review}
            </button>
            <button className={`pill${filter === "ready" ? " pill--active" : ""}`} onClick={() => setFilter("ready")}>
              Ready {counts.ready}
            </button>
            <button className={`pill${filter === "failed" ? " pill--active" : ""}`} onClick={() => setFilter("failed")}>
              Failed {counts.failed}
            </button>
            <button className={`pill${filter === "all" ? " pill--active" : ""}`} onClick={() => setFilter("all")}>
              All {app.assets.length}
            </button>
          </div>
          <span className="toolbar-spacer" />
          <div className="target-select">
            Target
            <select value={targetName} onChange={(e) => setTargetSite(e.target.value)}>
              {app.limiterPresets.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="library-body">
          <div className="library-grid-area">
            <div className="grid-header">
              <span>
                <strong>{selected.size}</strong> selected · <strong>{filtered.length}</strong> assets in Default
                project
              </span>
              <div className="grid-header-actions">
                <span className="grid-header-hint">Pipeline actions moved to the panel →</span>
                <span className="grid-header-divider" />
                <button className="btn-quiet" disabled title="Multiple projects aren't supported yet">
                  Move to project…
                </button>
                <button
                  className="btn-quiet"
                  style={{ color: "var(--danger)" }}
                  disabled={selectedIds.length === 0}
                  onClick={() => {
                    if (selectedIds.length === 0) return;
                    if (!window.confirm(`Remove ${selectedIds.length} asset(s) from the library? Files on disk are not deleted.`)) return;
                    for (const id of selectedIds) {
                      const a = app.assets.find((x) => x.id === id);
                      if (a) void app.removeAsset(a);
                    }
                    setSelected(new Set());
                  }}
                >
                  Delete
                </button>
              </div>
            </div>

            <div className="asset-grid">
              {filtered.map((asset) => {
                const flag = flags.get(asset.id) ?? "new";
                const meta = app.metadata[asset.id];
                const analysis = app.analyses[asset.id];
                const stage = stageOf(asset, Boolean(analysis), Boolean(meta));
                const analyzable =
                  (asset.media_type === "image" && ANALYZABLE_EXTENSIONS.includes(extensionOf(asset.path))) ||
                  asset.media_type === "video";
                return (
                  <button
                    key={asset.id}
                    className={`asset-card${selected.has(asset.id) ? " asset-card--selected" : ""}`}
                    onClick={(e) => toggleSelect(asset.id, e.metaKey || e.ctrlKey || e.shiftKey)}
                    onDoubleClick={() => app.openEditor(asset.id)}
                  >
                    <div className="asset-thumb">
                      {asset.media_type === "image" && (
                        <img src={convertFileSrc(asset.path)} alt="" loading="lazy" />
                      )}
                      {asset.media_type === "video" && (
                        <video src={`${convertFileSrc(asset.path)}#t=0.1`} muted preload="metadata" />
                      )}
                      <span className="thumb-badge thumb-badge--kind">{asset.media_type}</span>
                      <span className={`thumb-badge thumb-badge--flag flag-${flag}`}>{ASSET_FLAG_LABEL[flag]}</span>
                      <span
                        className="asset-delete-btn"
                        role="button"
                        aria-label="Remove from library"
                        title="Remove from library"
                        onClick={(e) => {
                          e.stopPropagation();
                          if (window.confirm(`Remove "${asset.path.split(/[\\/]/).pop()}" from the library? The file on disk is not deleted.`)) {
                            void app.removeAsset(asset);
                          }
                        }}
                      >
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                          <polyline points="3 6 5 6 21 6" />
                          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
                          <path d="M10 11v6" />
                          <path d="M14 11v6" />
                          <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
                        </svg>
                      </span>
                    </div>
                    <div className="asset-body">
                      <div className="asset-filename" title={asset.path}>
                        {asset.path.split(/[\\/]/).pop()}
                      </div>
                      <div className="asset-title-preview">
                        {meta?.title || (analyzable ? "Not yet processed." : "Not analyzable.")}
                      </div>
                      <div className="asset-footer">
                        <StageDots stage={stage} showLabel />
                        <span className="asset-kw-count">{meta ? `${meta.keywords.length} kw` : "—"}</span>
                      </div>
                    </div>
                  </button>
                );
              })}
              {filtered.length === 0 && (
                <div className="empty-state" style={{ gridColumn: "1 / -1" }}>
                  No assets match this view.
                </div>
              )}
            </div>
          </div>

          <ResizeHandle width={inspectorPanel.width} minWidth={INSPECTOR_MIN_WIDTH} onResize={inspectorPanel.setWidth} />
          <div className="inspector" style={{ width: inspectorPanel.width }}>
            {!inspected ? (
              <div className="empty-state">Select an asset to inspect it here.</div>
            ) : (
              <>
                <div className="inspector-preview">
                  {inspected.media_type === "image" && (
                    <img src={convertFileSrc(inspected.path)} alt="" />
                  )}
                  {inspected.media_type === "video" && (
                    <video src={convertFileSrc(inspected.path)} controls preload="metadata" />
                  )}
                </div>
                <div className="inspector-header">
                  <div className="inspector-header-meta">
                    <div className="inspector-filename" title={inspected.path}>
                      {inspected.path.split(/[\\/]/).pop()}
                    </div>
                    <div className="inspector-sub">
                      {inspected.media_type.toUpperCase()} · {formatBytes(inspected.size)}
                    </div>
                    <div className="inspector-chips">
                      {inspectedAnalysis && !inspectedAnalysis.editorial && (
                        <span className="chip-tag chip-tag--ok">commercial-safe</span>
                      )}
                      {inspectedAnalysis && (
                        <span className="chip-tag chip-tag--neutral">{inspectedAnalysis.model}</span>
                      )}
                    </div>
                  </div>
                </div>

                <div className="pipeline-section">
                  <div className="pipeline-section-head">
                    <span className="sl">Pipeline</span>
                    <span className="pipeline-section-note">
                      applies to {pipelineSelection.length || 1} selected · ☁ AI call · ⚡ local
                    </span>
                  </div>
                  {pipelineExpanded ? (
                    <div className="pipeline-rail">{stops.map(renderStop)}</div>
                  ) : nextStop ? (
                    <div className="pipeline-rail pipeline-rail--single">{renderStop(nextStop)}</div>
                  ) : (
                    <div className="pipeline-all-done">✓ All pipeline steps done for this selection</div>
                  )}
                  <button className="pipeline-toggle" onClick={() => setPipelineExpanded((v) => !v)}>
                    {pipelineExpanded ? "▴ Show next step only" : `▾ Show full pipeline · ${doneStops} of ${stops.length} done`}
                  </button>
                </div>

                <div className="inspector-body">
                  <div className="inspector-section">
                    <div className="inspector-section-head">
                      <SectionLabel>Title</SectionLabel>
                      {inspectedMeta && (
                        <span className="char-counter">
                          {inspectedMeta.title.length} /{" "}
                          {app.limiterPresets.find((p) => p.name === targetName)?.max_title_chars ?? "—"}
                        </span>
                      )}
                    </div>
                    {inspectedMeta ? (
                      <div className="readonly-box">{inspectedMeta.title}</div>
                    ) : (
                      <div className="readonly-box readonly-box--dashed">No metadata yet — Analyze fills title and keywords</div>
                    )}
                  </div>
                  <div className="inspector-section">
                    <div className="inspector-section-head">
                      <SectionLabel count={inspectedMeta?.keywords.length}>Keywords</SectionLabel>
                      <button className="btn-link" onClick={() => app.openEditor(inspected.id)}>
                        Open editor →
                      </button>
                    </div>
                    <div className="chip-wrap">
                      {inspectedKeywords.slice(0, 18).map((kw) => (
                        <CompactChip key={kw.word} kw={kw} />
                      ))}
                      {inspectedKeywords.length === 0 && <span className="empty-note">No keywords yet.</span>}
                    </div>
                  </div>
                  <div className="inspector-section">
                    <SectionLabel>Site compliance</SectionLabel>
                    <ComplianceTable
                      sites={app.enabledSites}
                      titleLength={inspectedMeta?.title.length ?? 0}
                      keywordCount={inspectedMeta?.keywords.length ?? 0}
                    />
                  </div>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
      <StatusStrip>
        {app.watch?.active ? (
          <>
            Watching {app.watch.path}
            <Sep />
          </>
        ) : null}
        Queue: {app.jobCounts.running} running · {app.jobCounts.pending} pending
      </StatusStrip>
    </div>
  );
}

function stopIcon(step: PipelineStop["step"]): string {
  if (step === "analyze") return "☁";
  if (step === "generate_metadata") return "⚡";
  if (step === "embed") return "✦";
  if (step === "export_csv") return "⇩";
  return "☁";
}

function stopSubline(stop: PipelineStop, targetName: string, protocol?: string): string {
  if (stop.state === "locked") {
    const deps: Record<string, string> = {
      analyze: "",
      generate_metadata: "needs Analyze first",
      embed: "needs Generate metadata first",
      upload: "needs Embed first",
      export_csv: "needs Generate metadata first",
    };
    return deps[stop.step] ?? "";
  }
  if (stop.blockedReason) return stop.blockedReason;
  if (stop.state === "done") {
    if (stop.step === "analyze") return `done for ${stop.doneCount} of ${stop.total}`;
    if (stop.step === "generate_metadata") return "done";
    if (stop.step === "embed") return "done";
    return "done";
  }
  if (stop.step === "analyze") return "~6s per asset · not run yet";
  if (stop.step === "generate_metadata") return "instant · local formatting";
  if (stop.step === "embed") return "writes XMP · review first";
  if (stop.step === "export_csv") return `${targetName} layout · see Sites`;
  return `delivers to ${targetName}${protocol ? ` over ${protocol}` : ""}`;
}
