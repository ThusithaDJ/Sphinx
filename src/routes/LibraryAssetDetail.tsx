import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { EditableChip, AddKeywordChip, RejectedChip } from "../components/KeywordChip";
import { ComplianceTableCondensed } from "../components/ComplianceTable";
import { SectionLabel, CharCounter } from "../components/Toggle";
import {
  HEAT_LEGEND,
  USER_ADDED_COLORS,
  estimateKeywordScores,
  insertKeywordsByHeat,
  keywordHeat,
  sortKeywordsByHeat,
  type Keyword,
} from "../lib/heat";
import { useApp, formatBytes, PROJECT_ID, ASSET_STATUS_LABEL } from "../state/AppContext";
import {
  deleteSiteMetadata,
  enrichKeywords,
  listSiteMetadata,
  setSiteMetadata,
  type GeneratedMetadata,
} from "../lib/api";

/** Library's "Asset detail" view (was the standalone Asset Editor screen).
 * Rendered by LibraryScreen when `editingAssetId` is set; "Back" just clears
 * that id, returning to whichever Library mode/scroll was active. */
export function LibraryAssetDetail() {
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
  /** Saved per-site drafts for this asset, keyed by site name. A site with no
   * entry uses the AI-generated default (`savedMeta`). */
  const [siteDrafts, setSiteDrafts] = useState<Record<string, GeneratedMetadata>>({});
  const [autosave, setAutosave] = useState<"idle" | "saving" | "saved">("idle");
  const [pendingSwitch, setPendingSwitch] = useState<{ from: string; to: string } | null>(null);

  /** The AI-generated metadata every site starts from. */
  const defaultMeta = useMemo(
    () => ({
      title: savedMeta?.title ?? "",
      description: savedMeta?.description ?? "",
      keywords: savedMeta?.keywords ?? analysis?.keywords ?? [],
    }),
    [savedMeta, analysis]
  );

  /** Words the AI (or enrichment) produced; anything else was typed by the user. */
  const aiWords = useMemo(() => new Set(defaultMeta.keywords.map((w) => w.toLowerCase())), [defaultMeta]);

  /** Scores AI words; marks the rest user-added (green, unscored). */
  function toKeywords(words: string[], aiSet: Set<string> = aiWords): Keyword[] {
    return estimateKeywordScores(words).map((k) =>
      aiSet.has(k.word.toLowerCase()) ? k : { word: k.word, confidence: 1, userAdded: true }
    );
  }

  function loadFields(meta: { title: string; description: string; keywords: string[] }) {
    setTitle(meta.title);
    setDescription(meta.description);
    setKeywords(sortKeywordsByHeat(toKeywords(meta.keywords)));
    setRejected([]);
    setDirty(false);
  }

  useEffect(() => {
    if (!asset) return;
    const site = previewSite || app.activeProfile?.name || app.limiterPresets[0]?.name || "";
    setPreviewSite(site);
    setSiteDrafts({});
    setAutosave("idle");
    setEnrichedAdded(null);
    loadFields(defaultMeta);
    let cancelled = false;
    void listSiteMetadata(asset.id)
      .then((rows) => {
        if (cancelled) return;
        const map: Record<string, GeneratedMetadata> = {};
        for (const r of rows) map[r.site_name] = r.metadata;
        setSiteDrafts(map);
        if (map[site]) loadFields(map[site]);
      })
      .catch((err) => app.setStatus(`Could not load site metadata: ${String(err)}`, "error"));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [asset?.id]);

  // A regenerated default flows into the fields while this site has no edits.
  useEffect(() => {
    if (!dirty && !siteDrafts[previewSite]) loadFields(defaultMeta);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [defaultMeta]);

  const sitePreset = app.limiterPresets.find((p) => p.name === previewSite) ?? null;
  const avgHeat = useMemo(() => {
    const scored = keywords.filter((k) => keywordHeat(k) !== null);
    if (scored.length === 0) return 0;
    const total = scored.reduce((sum, k) => sum + (keywordHeat(k) ?? 0), 0);
    return Math.round(total / scored.length);
  }, [keywords]);
  const belowFloor = keywords.filter((k) => !k.userAdded && (k.demand ?? 0) < app.demandFloor).length;
  const userAddedCount = keywords.filter((k) => k.userAdded).length;
  const topByHeatWords = useMemo(() => {
    const sorted = [...keywords].sort((a, b) => (keywordHeat(b) ?? 0) - (keywordHeat(a) ?? 0));
    return new Set(sorted.slice(0, 8).map((k) => k.word));
  }, [keywords]);

  function draftFor(site: string): GeneratedMetadata {
    const preset = app.limiterPresets.find((x) => x.name === site);
    return {
      title,
      description,
      keywords: keywords.map((k) => k.word),
      profile: site,
      meets_minimum_keywords: preset ? keywords.length >= preset.min_keywords : true,
    };
  }

  // Edits autosave to the current "Preview as" site shortly after typing stops.
  const autosaveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // Bumped on every edit, so a save that finishes after further typing
  // doesn't mark those newer edits as saved.
  const editVersion = useRef(0);
  const removedKeywords = useRef(new Map<string, Keyword>());
  const persistDraft = useCallback(
    async (site: string, draft: GeneratedMetadata) => {
      if (!asset || !site) return;
      setAutosave("saving");
      const version = editVersion.current;
      try {
        await setSiteMetadata(asset.id, site, draft);
        setSiteDrafts((prev) => ({ ...prev, [site]: draft }));
        setAutosave("saved");
        if (editVersion.current === version) setDirty(false);
      } catch (err) {
        setAutosave("idle");
        app.setStatus(`Could not save ${site} metadata: ${String(err)}`, "error");
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [asset?.id]
  );

  useEffect(() => {
    if (!dirty) return;
    clearTimeout(autosaveTimer.current);
    const site = previewSite;
    const draft = draftFor(site);
    autosaveTimer.current = setTimeout(() => void persistDraft(site, draft), 600);
    return () => clearTimeout(autosaveTimer.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [title, description, keywords, dirty, previewSite]);

  /** Saves pending edits immediately (before switching site or embedding). */
  async function flushDraft() {
    if (!dirty) return;
    clearTimeout(autosaveTimer.current);
    await persistDraft(previewSite, draftFor(previewSite));
  }

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
    editVersion.current += 1;
    setDirty(true);
  }

  // State updaters stay pure (StrictMode runs them twice), so the rejected
  // list is updated alongside, never from inside, the keywords updater.
  function removeKeyword(i: number) {
    const kw = keywords[i];
    if (!kw) return;
    const word = kw.word;
    removedKeywords.current.set(word, kw);
    setKeywords((prev) => prev.filter((k) => k.word !== word));
    setRejected((prev) => (prev.includes(word) ? prev : [...prev, word]));
    markDirty();
  }

  function restoreRejected(word: string) {
    setRejected((prev) => prev.filter((w) => w !== word));
    // Bring back the exact chip that was removed, score included.
    const original = removedKeywords.current.get(word);
    setKeywords((prev) =>
      prev.some((k) => k.word.toLowerCase() === word.toLowerCase())
        ? prev
        : insertKeywordsByHeat(prev, original ? [original] : toKeywords([word]))
    );
    markDirty();
  }

  /** `fromAi` marks enrichment results as scored terms; typed words are green. */
  function addKeywords(words: string[], fromAi = false) {
    const aiSet = fromAi ? new Set([...aiWords, ...words.map((w) => w.toLowerCase())]) : aiWords;
    setKeywords((prev) => {
      const existing = new Set(prev.map((k) => k.word.toLowerCase()));
      const fresh = [...new Set(words.map((w) => w.trim()).filter((w) => w && !existing.has(w.toLowerCase())))];
      return insertKeywordsByHeat(prev, toKeywords(fresh, aiSet));
    });
    const added = new Set(words.map((w) => w.toLowerCase()));
    setRejected((prev) => prev.filter((w) => !added.has(w.toLowerCase())));
    markDirty();
  }

  function sortByHeat() {
    setKeywords((prev) => sortKeywordsByHeat(prev));
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

  async function handleSaveAndEmbed() {
    if (!asset) return;
    setSaving(true);
    try {
      await flushDraft();
      await app.handleEmbed(asset, previewSite);
    } finally {
      setSaving(false);
    }
  }

  async function handleApproveAndNext() {
    if (!asset) return;
    setSaving(true);
    try {
      await flushDraft();
      // Stay on this asset if embedding failed so the error can be fixed.
      if (!(await app.handleEmbed(asset, previewSite))) return;
      const next = app.assets[index + 1];
      if (next) app.openEditor(next.id);
      else app.closeEditor();
    } catch (err) {
      app.setStatus(`Could not approve: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  /** Drops this site's edits so it falls back to the AI-generated default. */
  async function handleRevert() {
    if (!asset) return;
    clearTimeout(autosaveTimer.current);
    try {
      if (siteDrafts[previewSite]) await deleteSiteMetadata(asset.id, previewSite);
      setSiteDrafts((prev) => {
        const next = { ...prev };
        delete next[previewSite];
        return next;
      });
      loadFields(defaultMeta);
      setAutosave("idle");
    } catch (err) {
      app.setStatus(`Could not revert: ${String(err)}`, "error");
    }
  }

  /** Switching "Preview as": a site with its own saved edits loads them; a
   * fresh site asks whether to carry over the current site's edits. */
  async function switchSite(target: string) {
    if (target === previewSite) return;
    await flushDraft();
    if (siteDrafts[target]) {
      loadFields(siteDrafts[target]);
      setPreviewSite(target);
    } else if (siteDrafts[previewSite] || dirty) {
      setPendingSwitch({ from: previewSite, to: target });
    } else {
      loadFields(defaultMeta);
      setPreviewSite(target);
    }
  }

  async function resolveSwitch(choice: "edited" | "default") {
    if (!pendingSwitch || !asset) return;
    const { to } = pendingSwitch;
    setPendingSwitch(null);
    setPreviewSite(to);
    if (choice === "default") {
      loadFields(defaultMeta);
      return;
    }
    // Keep the fields as they are and save them as the new site's draft.
    const draft = draftFor(to);
    setDirty(false);
    try {
      await setSiteMetadata(asset.id, to, draft);
      setSiteDrafts((prev) => ({ ...prev, [to]: draft }));
      setAutosave("saved");
    } catch (err) {
      app.setStatus(`Could not copy metadata to ${to}: ${String(err)}`, "error");
    }
  }

  async function handleRegenerate() {
    if (!asset) return;
    await app.handleGenerateMetadata(asset);
  }

  async function handleEnrichClick() {
    if (!asset || !app.hasKeywordProvider) return;
    setEnrichBusy(true);
    try {
      const { added, errors } = await enrichKeywords(PROJECT_ID, asset.id);
      addKeywords(added, true);
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
          needsReview: app.assetFlags.get(asset.id) === "review",
          index: index + 1,
          total: app.assets.length,
          onBack: () => void flushDraft().then(() => app.closeEditor()),
        }}
        right={
          <div className="navbar-right">
            <span style={{ fontSize: 12, color: "var(--faint)" }}>
              {autosave === "saving" || dirty
                ? "Saving…"
                : siteDrafts[previewSite]
                  ? `Edits saved for ${previewSite}`
                  : `${previewSite || "Site"} · AI default`}
            </span>
            <button
              className="btn-secondary"
              onClick={() => void handleRevert()}
              disabled={!siteDrafts[previewSite] && !dirty}
              title={`Discard ${previewSite}'s edits and load the AI-generated metadata`}
            >
              Revert to AI
            </button>
            <button className="btn-primary" onClick={() => void handleSaveAndEmbed()} disabled={saving}>
              {saving ? "Embedding…" : "Save & embed"}
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
              <dd>{ASSET_STATUS_LABEL[asset.status] ?? asset.status}</dd>
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
              {sitePreset && <CharCounter value={title.length} limit={sitePreset.max_title_chars} siteName={sitePreset.name} />}
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
              {sitePreset && (
                <CharCounter value={description.length} limit={sitePreset.max_description_chars} siteName={sitePreset.name} />
              )}
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
                title={!app.hasKeywordProvider ? "Configure a keyword provider under Connections first" : ""}
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
              <div className="kw-legend">
                {HEAT_LEGEND.map((b) => (
                  <span key={b.label} className="kw-legend-item">
                    <span className="kw-legend-swatch" style={{ background: b.colors.bg, borderColor: b.colors.border }} />
                    {b.label} {b.range}
                  </span>
                ))}
                <span className="kw-legend-item">
                  <span
                    className="kw-legend-swatch"
                    style={{ background: USER_ADDED_COLORS.bg, borderColor: USER_ADDED_COLORS.border }}
                  />
                  Added by you · not scored
                </span>
              </div>
              <div>
                Score = model confidence + site demand (0–100) · avg heat {avgHeat} · {belowFloor} below demand floor{" "}
                {app.demandFloor}
                {userAddedCount > 0 && ` · ${userAddedCount} added by you`}
              </div>
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
                  onClick={() => void switchSite(p.name)}
                  title={siteDrafts[p.name] ? `${p.name} has its own edited metadata` : `${p.name} uses the AI-generated metadata`}
                >
                  {p.name}
                  {siteDrafts[p.name] && <span className="site-edited-dot" aria-label="edited" />}
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
      {pendingSwitch && (
        <div className="modal-backdrop" onClick={() => setPendingSwitch(null)}>
          <div className="modal-card" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
            <h3 className="card-title">Switch to {pendingSwitch.to}</h3>
            <p className="modal-body">
              You've edited the metadata for <strong>{pendingSwitch.from}</strong>. {pendingSwitch.to} doesn't have its own
              edits yet. What should it start from?
            </p>
            <div className="modal-actions">
              <button className="btn-primary" onClick={() => void resolveSwitch("edited")}>
                Use edits from {pendingSwitch.from}
              </button>
              <button className="btn-secondary" onClick={() => void resolveSwitch("default")}>
                Load AI-generated metadata
              </button>
              <button className="btn-link" onClick={() => setPendingSwitch(null)}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
      <StatusStrip>Edits autosave per site · Preview as switches site · Approve embeds {previewSite || "this site"}'s metadata</StatusStrip>
    </div>
  );
}
