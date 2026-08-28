import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { EditableChip, AddKeywordChip, RejectedChip } from "../components/KeywordChip";
import { ComplianceTableCondensed } from "../components/ComplianceTable";
import { SectionLabel, CharCounter } from "../components/Toggle";
import { estimateKeywordScores, keywordHeat, type Keyword } from "../lib/heat";
import { strictestTitleLimit, strictestDescriptionLimit } from "../lib/limits";
import { useApp, formatBytes, PROJECT_ID } from "../state/AppContext";
import { enrichKeywords } from "../lib/api";

export function AssetEditorScreen() {
  const app = useApp();
  const asset = app.assets.find((a) => a.id === app.editingAssetId) ?? null;
  const index = asset ? app.assets.findIndex((a) => a.id === asset.id) : -1;

  const savedMeta = asset ? app.metadata[asset.id] : undefined;
  const analysis = asset ? app.analyses[asset.id] : undefined;

  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [keywords, setKeywords] = useState<Keyword[]>([]);
  const [rejected, setRejected] = useState<string[]>([]);
  const [addValue, setAddValue] = useState("");
  const [previewSite, setPreviewSite] = useState("");
  const [dirty, setDirty] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [enrichBusy, setEnrichBusy] = useState(false);
  const [enrichedAdded, setEnrichedAdded] = useState<number | null>(null);
  const [embedTargets, setEmbedTargets] = useState({ iptc: true, xmp: true, exif: false });

  useEffect(() => {
    if (!asset) return;
    const source = savedMeta ?? (analysis ? { title: "", description: "", keywords: analysis.keywords } : null);
    setTitle(savedMeta?.title ?? "");
    setDescription(savedMeta?.description ?? "");
    setKeywords(estimateKeywordScores(source?.keywords ?? []));
    setRejected([]);
    setDirty(false);
    setEnrichedAdded(null);
    setPreviewSite((prev) => prev || app.activeProfile?.name || app.limiterPresets[0]?.name || "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [asset?.id]);

  const titleLimit = useMemo(() => strictestTitleLimit(app.enabledSites), [app.enabledSites]);
  const descLimit = useMemo(() => strictestDescriptionLimit(app.enabledSites), [app.enabledSites]);
  const avgHeat = useMemo(() => {
    if (keywords.length === 0) return 0;
    const total = keywords.reduce((sum, k) => sum + (keywordHeat(k) ?? 0), 0);
    return Math.round(total / keywords.length);
  }, [keywords]);
  const belowFloor = keywords.filter((k) => (k.demand ?? 0) < app.demandFloor).length;
  const topByHeatWords = useMemo(() => {
    const sorted = [...keywords].sort((a, b) => (keywordHeat(b) ?? 0) - (keywordHeat(a) ?? 0));
    return new Set(sorted.slice(0, 8).map((k) => k.word));
  }, [keywords]);

  if (!asset) {
    return (
      <div className="app-shell">
        <NavBar />
        <div className="empty-state">No asset selected. Pick one from the Library.</div>
        <StatusStrip />
      </div>
    );
  }

  function markDirty() {
    setDirty(true);
  }

  function removeKeyword(i: number) {
    setKeywords((prev) => {
      const word = prev[i].word;
      setRejected((r) => [...r, word]);
      return prev.filter((_, idx) => idx !== i);
    });
    markDirty();
  }

  function restoreRejected(word: string) {
    setRejected((prev) => prev.filter((w) => w !== word));
    setKeywords((prev) => [...prev, ...estimateKeywordScores([word])]);
    markDirty();
  }

  function addKeywords(words: string[]) {
    setKeywords((prev) => {
      const existing = new Set(prev.map((k) => k.word.toLowerCase()));
      const fresh = words.filter((w) => w && !existing.has(w.toLowerCase()));
      return [...prev, ...estimateKeywordScores(fresh)];
    });
    markDirty();
  }

  function sortByHeat() {
    setKeywords((prev) => [...prev].sort((a, b) => (keywordHeat(b) ?? 0) - (keywordHeat(a) ?? 0)));
    markDirty();
  }

  function onChipDrop(dropIndex: number) {
    if (dragIndex === null || dragIndex === dropIndex) {
      setDragIndex(null);
      setOverIndex(null);
      return;
    }
    setKeywords((prev) => {
      const next = [...prev];
      const [moved] = next.splice(dragIndex, 1);
      const insertAt = dragIndex < dropIndex ? dropIndex - 1 : dropIndex;
      next.splice(insertAt, 0, moved);
      return next;
    });
    setDragIndex(null);
    setOverIndex(null);
    markDirty();
  }

  function currentDraft() {
    return {
      title,
      description,
      keywords: keywords.map((k) => k.word),
      profile: app.activeProfile?.name ?? "custom",
      meets_minimum_keywords: app.activeProfile ? keywords.length >= app.activeProfile.min_keywords : true,
    };
  }

  async function handleSave() {
    if (!asset) return;
    setSaving(true);
    try {
      await app.handleSaveMetadata(asset.id, currentDraft());
      setDirty(false);
      app.setStatus("Saved edits.");
    } catch (err) {
      app.setStatus(`Could not save: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  async function handleApproveAndNext() {
    if (!asset) return;
    setSaving(true);
    try {
      await app.handleSaveMetadata(asset.id, currentDraft());
      await app.handleEmbed(asset);
      setDirty(false);
      const next = app.assets[index + 1];
      if (next) app.openEditor(next.id);
      else app.setScreen("library");
    } catch (err) {
      app.setStatus(`Could not approve: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  async function handleRevert() {
    if (!asset) return;
    setTitle(savedMeta?.title ?? "");
    setDescription(savedMeta?.description ?? "");
    setKeywords(estimateKeywordScores(savedMeta?.keywords ?? analysis?.keywords ?? []));
    setRejected([]);
    setDirty(false);
  }

  async function handleRegenerate() {
    if (!asset) return;
    await app.handleGenerateMetadata(asset);
  }

  async function handleEnrichClick() {
    if (!asset || !app.hasKeywordProvider) return;
    setEnrichBusy(true);
    try {
      await app.handleSaveMetadata(asset.id, currentDraft());
      const { result, added, errors } = await enrichKeywords(PROJECT_ID, asset.id);
      setKeywords(estimateKeywordScores(result.keywords));
      setEnrichedAdded(added.length);
      await app.refreshAssetResult(asset.id);
      app.setStatus(
        errors.length ? `Enriched: +${added.length} keywords, errors: ${errors.join("; ")}` : `Enriched: +${added.length} keywords.`
      );
    } catch (err) {
      app.setStatus(`Enrichment failed: ${String(err)}`);
    } finally {
      setEnrichBusy(false);
    }
  }

  const analyzing = app.analyzing[asset.id];
  const generating = app.generating[asset.id];

  return (
    <div className="app-shell">
      <NavBar
        editor={{
          fileName: asset.path.split(/[\\/]/).pop() ?? asset.path,
          needsReview: asset.status !== "uploaded",
          index: index + 1,
          total: app.assets.length,
          onBack: () => app.setScreen("library"),
        }}
        right={
          <div className="navbar-right">
            {dirty && <span style={{ fontSize: 12, color: "var(--faint)" }}>Unsaved changes</span>}
            <button className="btn-secondary" onClick={handleRevert} disabled={!dirty}>
              Revert to AI
            </button>
            <button className="btn-primary" onClick={() => void handleSave()} disabled={!dirty || saving}>
              {saving ? "Saving…" : "Save & embed"}
            </button>
          </div>
        }
      />
      <div className="editor-screen">
        <div className="editor-left">
          <div className="editor-preview">
            {asset.media_type === "image" && <img src={convertFileSrc(asset.path)} alt="" />}
            {asset.media_type === "video" && (
              <video src={convertFileSrc(asset.path)} controls preload="metadata" />
            )}
          </div>
          <div className="meta-grid">
            <div className="meta-row">
              <dt>Format</dt>
              <dd>
                {asset.media_type} · {formatBytes(asset.size)}
              </dd>
            </div>
            <div className="meta-row">
              <dt>Ingested</dt>
              <dd>{new Date(asset.created_at).toLocaleString()}</dd>
            </div>
            <div className="meta-row">
              <dt>Status</dt>
              <dd>{asset.status}</dd>
            </div>
            <div className="meta-row">
              <dt>Hash</dt>
              <dd className="mono" title={asset.hash}>
                {asset.hash.slice(0, 10)}…
              </dd>
            </div>
          </div>
          <div className="pipeline-block">
            <SectionLabel>Pipeline</SectionLabel>
            <div className="pipeline-row">
              <span className="check-dot" style={{ background: analysis ? "var(--accent)" : "var(--field)" }} />
              <span>Analyze</span>
              <span className="pipeline-detail">
                {analyzing ? "analyzing…" : analysis ? `${analysis.provider}/${analysis.model}` : "pending"}
              </span>
            </div>
            <div className="pipeline-row">
              <span className="check-dot" style={{ background: savedMeta ? "var(--accent)" : "var(--field)" }} />
              <span>Generate</span>
              <span className="pipeline-detail">
                {generating ? "generating…" : savedMeta ? `${savedMeta.keywords.length} kw` : "pending"}
              </span>
            </div>
            <div className="pipeline-row">
              <span className="check-dot" style={{ background: enrichedAdded !== null ? "var(--accent)" : "var(--field)" }} />
              <span>Enrich</span>
              <span className="pipeline-detail">
                {enrichBusy ? "enriching…" : enrichedAdded !== null ? `+${enrichedAdded} terms` : "optional"}
              </span>
            </div>
            <div className="pipeline-row">
              <span
                className="check-dot"
                style={{ background: asset.status === "embedded" || asset.status === "uploaded" ? "var(--accent)" : "var(--field)" }}
              />
              <span>Embed</span>
              <span className="pipeline-detail">
                {app.embedding[asset.id] ? "embedding…" : asset.status === "embedded" || asset.status === "uploaded" ? "done" : "pending"}
              </span>
            </div>
            <div className="pipeline-row">
              <span className="check-dot" style={{ background: asset.status === "uploaded" ? "var(--accent)" : "var(--field)" }} />
              <span>Upload</span>
              <span className="pipeline-detail">
                {app.uploading[asset.id] ? "uploading…" : asset.status === "uploaded" ? "done" : "pending"}
              </span>
            </div>
          </div>
        </div>

        <div className="editor-center">
          <div>
            <div className="editor-field-head">
              <SectionLabel>Title</SectionLabel>
              {titleLimit && <CharCounter value={title.length} limit={titleLimit.limit} siteName={titleLimit.site} />}
            </div>
            <input
              className="title-input"
              value={title}
              onChange={(e) => {
                setTitle(e.target.value);
                markDirty();
              }}
              placeholder="Descriptive, keyword-rich title"
            />
          </div>

          <div>
            <div className="editor-field-head">
              <SectionLabel>Description</SectionLabel>
              {descLimit && <CharCounter value={description.length} limit={descLimit.limit} siteName={descLimit.site} />}
            </div>
            <textarea
              className="description-input"
              value={description}
              onChange={(e) => {
                setDescription(e.target.value);
                markDirty();
              }}
              placeholder="One or two sentences describing the scene"
            />
          </div>

          <div className="keyword-field">
            <div className="keyword-field-header">
              <SectionLabel count={keywords.length}>Keyword field</SectionLabel>
              <span className="keyword-field-hint">drag to reorder priority · warm = high blended demand</span>
              <span className="navbar-spacer" />
              <button
                className="btn-link"
                disabled={!app.hasKeywordProvider || enrichBusy}
                title={!app.hasKeywordProvider ? "Configure a keyword provider under Sites first" : ""}
                onClick={() => void handleEnrichClick()}
              >
                Enrich +
              </button>
              <button className="btn-link" style={{ color: "var(--muted)" }} onClick={sortByHeat}>
                Sort by heat
              </button>
            </div>
            <div className="keyword-field-body" onDragOver={(e) => e.preventDefault()} onDrop={() => onChipDrop(keywords.length)}>
              {keywords.map((kw, i) => (
                <span key={kw.word} style={{ display: "inline-flex", alignItems: "center" }}>
                  {overIndex === i && dragIndex !== null && dragIndex !== i && <span className="kw-drop-marker" />}
                  <EditableChip
                    kw={kw}
                    index={i}
                    isTopByHeat={topByHeatWords.has(kw.word)}
                    onRemove={() => removeKeyword(i)}
                    dragProps={{
                      draggable: true,
                      onDragStart: () => setDragIndex(i),
                      onDragOver: (e) => {
                        e.preventDefault();
                        setOverIndex(i);
                      },
                      onDrop: (e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        onChipDrop(i);
                      },
                      onDragEnd: () => {
                        setDragIndex(null);
                        setOverIndex(null);
                      },
                      className: dragIndex === i ? "kw-dragging" : "",
                    }}
                  />
                </span>
              ))}
              <AddKeywordChip value={addValue} onChange={setAddValue} onAdd={addKeywords} />
            </div>
            <div className="keyword-field-footer">
              avg heat {avgHeat} · {belowFloor} terms below demand floor {app.demandFloor} · order is stored but not sent to sites
            </div>
          </div>
        </div>

        <div className="editor-right">
          <div>
            <SectionLabel>Preview as</SectionLabel>
            <div className="pill-select" style={{ flexWrap: "wrap", marginTop: 6 }}>
              {app.limiterPresets.map((p) => (
                <button
                  key={p.name}
                  className={`outline-pill${previewSite === p.name ? " outline-pill--active" : ""}`}
                  onClick={() => setPreviewSite(p.name)}
                >
                  {p.name}
                </button>
              ))}
            </div>
          </div>

          <div className="preview-card">
            <span className="preview-caption">{previewSite || "Site"} submission</span>
            <span className="preview-title">{title || "Untitled"}</span>
            <span className="preview-sub">
              {keywords.length} keywords · {analysis?.editorial ? "editorial" : "commercial"} · no release required
            </span>
          </div>

          <div className="inspector-section">
            <SectionLabel>Limits</SectionLabel>
            <ComplianceTableCondensed sites={app.enabledSites} titleLength={title.length} keywordCount={keywords.length} />
          </div>

          <div className="inspector-section">
            <SectionLabel>Embed targets</SectionLabel>
            <div className="embed-targets">
              <label className="embed-target-row">
                <input
                  type="checkbox"
                  checked={embedTargets.iptc}
                  onChange={(e) => setEmbedTargets((t) => ({ ...t, iptc: e.target.checked }))}
                />
                IPTC Keywords + Headline
              </label>
              <label className="embed-target-row">
                <input
                  type="checkbox"
                  checked={embedTargets.xmp}
                  onChange={(e) => setEmbedTargets((t) => ({ ...t, xmp: e.target.checked }))}
                />
                XMP dc:subject / dc:title
              </label>
              <label className="embed-target-row">
                <input
                  type="checkbox"
                  checked={embedTargets.exif}
                  onChange={(e) => setEmbedTargets((t) => ({ ...t, exif: e.target.checked }))}
                />
                EXIF ImageDescription
              </label>
            </div>
          </div>

          {rejected.length > 0 && (
            <div className="inspector-section">
              <SectionLabel count={rejected.length}>Rejected by you</SectionLabel>
              <div className="chip-wrap">
                {rejected.map((word) => (
                  <RejectedChip key={word} word={word} onRestore={() => restoreRejected(word)} />
                ))}
              </div>
            </div>
          )}

          <span className="navbar-spacer" />
          <div className="inspector-footer" style={{ padding: 0, borderTop: "none" }}>
            <button className="btn-primary" style={{ flex: 1 }} onClick={() => void handleApproveAndNext()} disabled={saving}>
              Approve & next →
            </button>
            <button className="btn-secondary" onClick={() => void handleRegenerate()} disabled={generating || !analysis}>
              Regenerate
            </button>
          </div>
        </div>
      </div>
      <StatusStrip>Ctrl+S save · Ctrl+Enter approve and next · Alt+1–4 switch preview site</StatusStrip>
    </div>
  );
}
