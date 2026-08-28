import { useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { NavBar } from "../components/NavBar";
import { StatusStrip, Sep } from "../components/StatusStrip";
import { StageDots } from "../components/StageDots";
import { CompactChip } from "../components/KeywordChip";
import { ComplianceTable } from "../components/ComplianceTable";
import { SectionLabel } from "../components/Toggle";
import { estimateKeywordScores } from "../lib/heat";
import { gradeAllSites } from "../lib/limits";
import { useApp, extensionOf, formatBytes, stageOf, ANALYZABLE_EXTENSIONS } from "../state/AppContext";
import type { Asset } from "../lib/api";

type Flag = "review" | "ready" | "blocked" | "failed";

function flagOf(app: ReturnType<typeof useApp>, asset: Asset): Flag {
  if (app.analyzeErrors[asset.id]) return "failed";
  if (asset.media_type === "video" && !app.ffmpeg.ok && !app.analyses[asset.id]) return "blocked";
  const meta = app.metadata[asset.id];
  if (meta && app.enabledSites.length > 0) {
    const rows = gradeAllSites(app.enabledSites, meta.title.length, meta.keywords.length);
    if (rows.every((r) => r.overall === "pass")) return "ready";
  }
  return "review";
}

type FilterKey = "all" | "review" | "ready" | "failed";

export function LibraryScreen() {
  const app = useApp();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<FilterKey>("all");
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [inspectedId, setInspectedId] = useState<number | null>(null);
  const [targetSite, setTargetSite] = useState<string>("");

  const targetName = targetSite || app.activeProfile?.name || app.limiterPresets[0]?.name || "";

  const flags = useMemo(() => {
    const map = new Map<number, Flag>();
    for (const a of app.assets) map.set(a.id, flagOf(app, a));
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [app.assets, app.analyses, app.metadata, app.analyzeErrors, app.enabledSites, app.ffmpeg.ok]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return app.assets.filter((a) => {
      if (q) {
        const title = app.metadata[a.id]?.title.toLowerCase() ?? "";
        const kws = app.metadata[a.id]?.keywords.join(" ").toLowerCase() ?? "";
        if (!a.path.toLowerCase().includes(q) && !title.includes(q) && !kws.includes(q)) return false;
      }
      if (filter === "all") return true;
      if (filter === "failed") return flags.get(a.id) === "failed";
      if (filter === "ready") return flags.get(a.id) === "ready";
      if (filter === "review") return flags.get(a.id) === "review";
      return true;
    });
  }, [app.assets, app.metadata, query, filter, flags]);

  const counts = useMemo(() => {
    let review = 0,
      ready = 0,
      failed = 0;
    for (const a of app.assets) {
      const f = flags.get(a.id);
      if (f === "review") review++;
      else if (f === "ready") ready++;
      else if (f === "failed") failed++;
    }
    return { review, ready, failed };
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
  const inspectedKeywords = inspectedMeta ? estimateKeywordScores(inspectedMeta.keywords) : [];
  const inspectedCompliance =
    inspected && inspectedMeta ? gradeAllSites(app.enabledSites, inspectedMeta.title.length, inspectedMeta.keywords.length) : [];
  const canApprove = inspected && inspectedMeta && inspectedCompliance.every((r) => r.overall !== "fail");

  const selectedIds = Array.from(selected);

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
          <div className="search-box">
            <span className="search-glyph">⌕</span>
            <input placeholder="Search title, keyword, path" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          <div className="pill-select">
            <button className={`pill${filter === "all" ? " pill--active" : ""}`} onClick={() => setFilter("all")}>
              All {app.assets.length}
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
                <button
                  className="btn-quiet"
                  disabled={selectedIds.length === 0 || !app.hasKey}
                  title={!app.hasKey ? "Configure an AI provider first" : ""}
                  onClick={() => void app.handleEnqueueBatch("analyze", "Analyze", selectedIds)}
                >
                  Analyze
                </button>
                <button
                  className="btn-quiet"
                  disabled={selectedIds.length === 0}
                  onClick={() => void app.handleEnqueueBatch("generate_metadata", "Generate metadata", selectedIds)}
                >
                  Generate metadata
                </button>
                <button
                  className="btn-quiet"
                  disabled={selectedIds.length === 0 || !app.exiftool.ok}
                  title={!app.exiftool.ok ? "exiftool not found (Settings)" : ""}
                  onClick={() => void app.handleEnqueueBatch("embed", "Embed", selectedIds)}
                >
                  Embed
                </button>
                <button
                  className="btn-quiet"
                  disabled={selectedIds.length === 0 || !app.selectedSftpProfileId}
                  title={!app.selectedSftpProfileId ? "Select an SFTP profile under Sites first" : ""}
                  onClick={() => void app.handleEnqueueBatch("upload", "Upload", selectedIds)}
                >
                  Upload
                </button>
              </div>
            </div>

            <div className="asset-grid">
              {filtered.map((asset) => {
                const flag = flags.get(asset.id) ?? "review";
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
                      <span className={`thumb-badge thumb-badge--flag flag-${flag}`}>{flag}</span>
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

          <div className="inspector">
            {!inspected ? (
              <div className="empty-state">Select an asset to inspect it here.</div>
            ) : (
              <>
                <div className="inspector-header">
                  <div className="inspector-thumb">
                    {inspected.media_type === "image" && (
                      <img src={convertFileSrc(inspected.path)} alt="" />
                    )}
                    {inspected.media_type === "video" && (
                      <video src={convertFileSrc(inspected.path)} controls preload="metadata" />
                    )}
                  </div>
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
                    <div className="readonly-box">{inspectedMeta?.title ?? "Not generated yet."}</div>
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
                <div className="inspector-footer">
                  <button
                    className="btn-primary"
                    style={{ flex: 1 }}
                    disabled={!canApprove || app.embedding[inspected.id]}
                    onClick={async () => {
                      await app.handleEmbed(inspected);
                      const next = filtered.find((a) => a.id !== inspected.id && flags.get(a.id) !== "ready");
                      if (next) setInspectedId(next.id);
                    }}
                  >
                    Approve & embed
                  </button>
                  <button
                    className="btn-secondary"
                    disabled={!inspectedAnalysis || app.generating[inspected.id]}
                    onClick={() => void app.handleGenerateMetadata(inspected)}
                  >
                    Regenerate
                  </button>
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
