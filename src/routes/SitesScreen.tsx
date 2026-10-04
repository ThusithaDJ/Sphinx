import { useEffect, useState, type MouseEvent } from "react";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { Toggle } from "../components/Toggle";
import { useApp, deliveryProfileFor, BLANK_SFTP_DRAFT, type SftpDraft } from "../state/AppContext";
import type { LimiterProfile } from "../lib/api";
import { CSV_SOURCES, csvFormatFor, type CsvColumn, type CsvSource } from "../lib/csvFormats";
import { deliverySupportFor } from "../lib/delivery";

export function SitesScreen() {
  const app = useApp();
  const [selectedName, setSelectedName] = useState("");
  const [draftProfile, setDraftProfile] = useState<LimiterProfile | null>(null);
  const [saving, setSaving] = useState(false);
  const [addingSite, setAddingSite] = useState(false);
  const [newSiteName, setNewSiteName] = useState("");

  async function handleAddSite() {
    const name = newSiteName.trim();
    if (!name) return;
    const added = await app.createSiteProfile(name);
    if (!added) return;
    setNewSiteName("");
    setAddingSite(false);
    setSelectedName(added);
  }

  async function handleRemoveSite(e: MouseEvent, name: string) {
    e.stopPropagation();
    const transport = deliveryProfileFor(app.sftpProfiles, name);
    const message =
      `Remove the "${name}" site profile?` +
      (transport ? " Its delivery settings and saved password are removed too." : "") +
      " This does not affect assets already targeting it.";
    if (!window.confirm(message)) return;
    await app.deleteSiteProfile(name);
    if (selectedName === name) setSelectedName("");
  }

  const selected = app.limiterPresets.find((p) => p.name === selectedName) ?? app.limiterPresets[0] ?? null;

  useEffect(() => {
    if (!selectedName && app.limiterPresets.length) {
      setSelectedName(app.activeProfile?.name ?? app.limiterPresets[0].name);
    }
  }, [app.limiterPresets, app.activeProfile, selectedName]);

  useEffect(() => {
    if (!selected) return;
    setDraftProfile(selected);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected?.name]);

  if (!selected || !draftProfile) {
    return (
      <div className="app-shell">
        <NavBar />
        <div className="empty-state">Loading site profiles…</div>
        <StatusStrip />
      </div>
    );
  }

  const isDefault = app.activeProfile?.name === selected.name;
  const isEnabled = app.enabledProfileNames.has(selected.name);
  const enabledCount = app.enabledProfileNames.size;

  async function saveProfile() {
    setSaving(true);
    try {
      await app.saveActiveProfile(draftProfile!);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="app-shell">
      <NavBar
        right={
          <button className="btn-primary" onClick={() => setAddingSite((v) => !v)}>
            Add site profile
          </button>
        }
      />
      <div className="sites-screen">
        <div className="sites-list">
          {app.limiterPresets.map((p) => {
            const enabled = app.enabledProfileNames.has(p.name);
            return (
              <div
                key={p.name}
                className={`site-row${p.name === selected.name ? " site-row--selected" : ""}`}
                onClick={() => setSelectedName(p.name)}
              >
                <span className="site-status-dot" style={{ background: enabled ? "var(--ok)" : "var(--off)" }} />
                <div className="site-row-body">
                  <span className="site-row-name">
                    {p.name} <span className="site-row-status">{enabled ? "enabled" : "off"}</span>
                  </span>
                  <span className="site-row-limits">
                    title ≤ {p.max_title_chars} · desc ≤ {p.max_description_chars} · {p.min_keywords}–{p.max_keywords} keywords
                  </span>
                </div>
                <button
                  className="site-row-remove"
                  title={`Remove "${p.name}"`}
                  onClick={(e) => void handleRemoveSite(e, p.name)}
                >
                  ✕
                </button>
              </div>
            );
          })}
          <p className="sites-list-note">Metadata is validated against every enabled profile before a file can be approved.</p>
          {addingSite && (
            <div className="add-site-form">
              <input
                autoFocus
                placeholder="Site name, e.g. Getty Images"
                value={newSiteName}
                onChange={(e) => setNewSiteName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void handleAddSite()}
              />
              <button className="btn-primary" onClick={() => void handleAddSite()} disabled={!newSiteName.trim()}>
                Add
              </button>
              <button
                className="btn-secondary"
                onClick={() => {
                  setAddingSite(false);
                  setNewSiteName("");
                }}
              >
                Cancel
              </button>
            </div>
          )}
        </div>

        <div className="sites-detail">
          <div className="card">
            <div className="site-card-head">
              <h3 className="card-title">{selected.name}</h3>
              {isDefault && <span className="chip-tag chip-tag--ok">default target</span>}
              <span className="navbar-spacer" />
              <span className="field-label" style={{ margin: 0 }}>
                Enabled
              </span>
              <Toggle checked={isEnabled} onChange={() => app.toggleProfileEnabled(selected.name)} />
            </div>
            <div className="field-grid" style={{ gridTemplateColumns: "repeat(3, 1fr)" }}>
              <div className="field">
                <span className="field-label">Title max</span>
                <input
                  type="number"
                  value={draftProfile.max_title_chars}
                  onChange={(e) => setDraftProfile({ ...draftProfile, max_title_chars: Number(e.target.value) || 1 })}
                />
              </div>
              <div className="field">
                <span className="field-label">Description max</span>
                <input
                  type="number"
                  value={draftProfile.max_description_chars}
                  onChange={(e) => setDraftProfile({ ...draftProfile, max_description_chars: Number(e.target.value) || 1 })}
                />
              </div>
              <div className="field">
                <span className="field-label">Keyword range</span>
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <input
                    type="number"
                    value={draftProfile.min_keywords}
                    onChange={(e) => setDraftProfile({ ...draftProfile, min_keywords: Number(e.target.value) || 0 })}
                  />
                  <span>–</span>
                  <input
                    type="number"
                    value={draftProfile.max_keywords}
                    onChange={(e) => setDraftProfile({ ...draftProfile, max_keywords: Number(e.target.value) || 1 })}
                  />
                </div>
              </div>
            </div>
            <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 8 }}>
              <button className="btn-primary" style={{ alignSelf: "flex-start" }} onClick={() => void saveProfile()} disabled={saving}>
                {isDefault ? "Save" : "Set as active & save"}
              </button>
              {!isDefault && (
                <span className="hint" style={{ fontSize: 11.5 }}>
                  Only one profile can be the project's active target today — saving switches it to {selected.name}.
                </span>
              )}
            </div>
          </div>

          <DeliveryCard siteName={selected.name} />

          <CsvFormatCard siteName={selected.name} />

          <p className="sites-list-note">
            Keyword-demand API keys for this site live in{" "}
            <button className="btn-link" style={{ display: "inline" }} onClick={() => app.setScreen("connections")}>
              Connections →
            </button>
          </p>
        </div>
      </div>
      <StatusStrip>
        {enabledCount} of {app.limiterPresets.length} profiles enabled · strictest limit wins when several are enabled
      </StatusStrip>
    </div>
  );
}

/** Editable CSV export layout for one site: add, remove, rename, reorder
 * (drag or arrows) and pick what fills each column. */
function CsvFormatCard({ siteName }: { siteName: string }) {
  const app = useApp();
  const format = csvFormatFor(siteName, app.csvLayouts);
  const [draft, setDraft] = useState<CsvColumn[]>(format.columns);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(csvFormatFor(siteName, app.csvLayouts).columns.map((c) => ({ ...c })));
  }, [siteName, app.csvLayouts]);

  const dirty = JSON.stringify(draft) !== JSON.stringify(format.columns);
  const headers = draft.map((c) => c.header.trim());
  const hasEmpty = headers.some((h) => !h);
  const hasDuplicate = new Set(headers.map((h) => h.toLowerCase())).size !== headers.length;
  const invalid = draft.length === 0 || hasEmpty || hasDuplicate;

  function patch(index: number, next: Partial<CsvColumn>) {
    setDraft((d) => d.map((c, i) => (i === index ? { ...c, ...next } : c)));
  }

  function move(from: number, to: number) {
    if (to < 0 || to >= draft.length || from === to) return;
    setDraft((d) => {
      const next = [...d];
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return next;
    });
  }

  async function save(columns: CsvColumn[] | null) {
    setSaving(true);
    try {
      await app.saveCsvLayout(siteName, columns && columns.map((c) => ({ ...c, header: c.header.trim() })));
    } finally {
      setSaving(false);
    }
  }

  function reset() {
    if (!window.confirm(`Reset the ${siteName} CSV layout to its built-in default? Your column edits are lost.`)) return;
    void save(null);
  }

  return (
    <div className="card">
      <div className="site-card-head">
        <h3 className="card-title">CSV format</h3>
        <span className="chip-tag chip-tag--neutral">{draft.length} columns</span>
        {format.customized && <span className="chip-tag chip-tag--ok">customized</span>}
      </div>
      <div className="csv-editor">
        {draft.map((c, i) => (
          <div
            key={i}
            className={`csv-editor-row${dragIndex === i ? " csv-editor-row--dragging" : ""}`}
            draggable
            onDragStart={(e) => {
              setDragIndex(i);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              e.preventDefault();
              if (dragIndex !== null && dragIndex !== i) {
                move(dragIndex, i);
                setDragIndex(i);
              }
            }}
            onDragEnd={() => setDragIndex(null)}
          >
            <span className="csv-editor-handle" title="Drag to reorder">
              ⠿
            </span>
            <span className="csv-editor-index">{i + 1}</span>
            <input
              className={`mono${!c.header.trim() ? " input--invalid" : ""}`}
              value={c.header}
              placeholder="Column header"
              onChange={(e) => patch(i, { header: e.target.value })}
            />
            <select value={c.source} onChange={(e) => patch(i, { source: e.target.value as CsvSource })}>
              {CSV_SOURCES.map((s) => (
                <option key={s.value} value={s.value}>
                  {s.label}
                </option>
              ))}
            </select>
            <button className="btn-quiet" title="Move up" disabled={i === 0} onClick={() => move(i, i - 1)}>
              ↑
            </button>
            <button className="btn-quiet" title="Move down" disabled={i === draft.length - 1} onClick={() => move(i, i + 1)}>
              ↓
            </button>
            <button
              className="btn-quiet"
              title="Remove column"
              style={{ color: "var(--danger)" }}
              onClick={() => setDraft((d) => d.filter((_, j) => j !== i))}
            >
              ✕
            </button>
          </div>
        ))}
        {draft.length === 0 && <div className="hint">No columns — add at least one.</div>}
      </div>
      <div style={{ marginTop: 10, display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button className="btn-secondary" onClick={() => setDraft((d) => [...d, { header: "", source: "blank" }])}>
          + Add column
        </button>
        <span className="navbar-spacer" />
        {dirty && (
          <button className="btn-quiet" onClick={() => setDraft(format.columns.map((c) => ({ ...c })))} disabled={saving}>
            Discard
          </button>
        )}
        {format.customized && (
          <button className="btn-quiet" onClick={reset} disabled={saving}>
            Reset to default
          </button>
        )}
        <button className="btn-primary" onClick={() => void save(draft)} disabled={!dirty || invalid || saving}>
          Save layout
        </button>
      </div>
      {(hasEmpty || hasDuplicate) && (
        <p className="hint" style={{ fontSize: 11.5, marginTop: 8, color: "var(--danger)" }}>
          {hasEmpty ? "Every column needs a header." : "Column headers must be unique."}
        </p>
      )}
      <p className="hint" style={{ fontSize: 11.5, marginTop: 10 }}>
        Each column is filled from the chosen metadata field; "Blank" columns are left empty for you to complete
        before uploading. Keywords are joined with "{format.keywordSeparator}". Comma-separated, UTF-8, one row per
        file.{format.note ? ` ${format.note}` : ""} Export it from the Library pipeline's Export CSV step.
      </p>
    </div>
  );
}

/** SFTP/FTPS delivery settings for one site (moved here from Connections). */
function DeliveryCard({ siteName }: { siteName: string }) {
  const app = useApp();
  const support = deliverySupportFor(siteName);
  const existing = deliveryProfileFor(app.sftpProfiles, siteName);
  const [draft, setDraft] = useState<SftpDraft>(BLANK_SFTP_DRAFT);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    setDraft(
      existing
        ? {
            name: existing.name,
            site: existing.site,
            protocol: existing.protocol,
            host: existing.host,
            port: existing.port,
            username: existing.username,
            remote_dir: existing.remote_dir,
            password: "",
          }
        : { ...BLANK_SFTP_DRAFT, ...support.defaults, name: siteName }
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [siteName, app.sftpProfiles]);

  async function save() {
    setSaving(true);
    try {
      if (existing) await app.editSftpProfile(existing.id, draft);
      else await app.addSftpProfile(draft);
    } catch (err) {
      app.setStatus(`Could not save delivery settings: ${String(err)}`, "error");
    } finally {
      setSaving(false);
    }
  }

  async function test() {
    if (!existing) return;
    setTesting(true);
    try {
      await app.testSftpConnection(existing);
    } finally {
      setTesting(false);
    }
  }

  async function remove() {
    if (!existing) return;
    if (!window.confirm(`Remove the delivery settings for "${siteName}"? The saved password is removed too.`)) return;
    await app.removeSftpProfile(existing);
  }

  return (
    <div className="card">
      <div className="site-card-head">
        <h3 className="card-title">Delivery</h3>
        {support.supported && existing && <span className="chip-tag chip-tag--ok">{existing.protocol.toUpperCase()} configured</span>}
        {!support.supported && <span className="chip-tag chip-tag--neutral">not supported</span>}
      </div>
      <p className="hint" style={{ fontSize: 11.5, marginTop: 0 }}>
        {support.note}
      </p>
      {support.supported && (
        <>
          <div className="pill-select" style={{ marginTop: 8 }}>
            {(["sftp", "ftps"] as const).map((proto) => (
              <button
                key={proto}
                className={`pill${draft.protocol === proto ? " pill--active" : ""}`}
                onClick={() =>
                  setDraft((d) => ({
                    ...d,
                    protocol: proto,
                    port: d.port === (proto === "sftp" ? 21 : 22) ? (proto === "sftp" ? 22 : 21) : d.port,
                  }))
                }
              >
                {proto.toUpperCase()}
              </button>
            ))}
          </div>
          <div className="transport-grid" style={{ marginTop: 12 }}>
            <div className="field">
              <span className="field-label">Host</span>
              <input
                className="mono"
                value={draft.host}
                onChange={(e) => setDraft({ ...draft, host: e.target.value })}
                placeholder={draft.protocol === "ftps" ? "ftps.example.com" : "sftp.example.com"}
              />
            </div>
            <div className="field">
              <span className="field-label">Port</span>
              <input
                type="number"
                value={draft.port}
                onChange={(e) => setDraft({ ...draft, port: Number(e.target.value) || (draft.protocol === "ftps" ? 21 : 22) })}
              />
            </div>
            <div className="field">
              <span className="field-label">Username</span>
              <input value={draft.username} onChange={(e) => setDraft({ ...draft, username: e.target.value })} />
            </div>
            <div className="field">
              <span className="field-label">Remote directory</span>
              <input value={draft.remote_dir} onChange={(e) => setDraft({ ...draft, remote_dir: e.target.value })} />
            </div>
            <div className="field">
              <span className="field-label">Password {existing && <span className="hint">(blank = keep existing)</span>}</span>
              <input
                type="password"
                autoComplete="off"
                value={draft.password}
                onChange={(e) => setDraft({ ...draft, password: e.target.value })}
              />
            </div>
          </div>
          <div style={{ marginTop: 12, display: "flex", gap: 8, alignItems: "center" }}>
            <button className="btn-primary" onClick={() => void save()} disabled={saving || !draft.host || !draft.username}>
              {existing ? "Save delivery" : "Add delivery"}
            </button>
            <button
              className="btn-secondary"
              onClick={() => void test()}
              disabled={!existing || testing}
              title={!existing ? "Save the delivery settings first" : ""}
            >
              {testing ? "Testing…" : "Test connection"}
            </button>
            {existing && (
              <button className="btn-quiet" style={{ color: "var(--danger)" }} onClick={() => void remove()}>
                Remove
              </button>
            )}
          </div>
          {existing && (
            <div className="test-result">
              <span className="test-result-dot" style={{ background: existing.host_key_fingerprint ? "var(--ok)" : "var(--off)" }} />
              <span style={{ color: existing.host_key_fingerprint ? "var(--ok-ink)" : "var(--faint)" }}>
                {existing.host_key_fingerprint
                  ? existing.protocol === "ftps"
                    ? "TLS certificate verified — connected successfully."
                    : `Host key verified · fingerprint ${existing.host_key_fingerprint.slice(0, 16)}…`
                  : "Not yet connected — verifies on first upload."}
              </span>
            </div>
          )}
        </>
      )}
    </div>
  );
}
