import { useEffect, useState } from "react";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { Toggle, SectionLabel } from "../components/Toggle";
import { useApp, BLANK_SFTP_DRAFT, type SftpDraft } from "../state/AppContext";
import type { LimiterProfile, SftpSite } from "../lib/api";

function matchSftp(sites: ReturnType<typeof useApp>["sftpProfiles"], name: string) {
  const asSite: SftpSite = name === "Adobe Stock" ? "adobe_stock" : "generic";
  return (
    sites.find((s) => asSite === "adobe_stock" && s.site === "adobe_stock") ??
    sites.find((s) => s.name.toLowerCase() === name.toLowerCase())
  );
}

export function SitesScreen() {
  const app = useApp();
  const [selectedName, setSelectedName] = useState("");
  const [draftProfile, setDraftProfile] = useState<LimiterProfile | null>(null);
  const [sftpDraft, setSftpDraft] = useState<SftpDraft>(BLANK_SFTP_DRAFT);
  const [saving, setSaving] = useState(false);

  const selected = app.limiterPresets.find((p) => p.name === selectedName) ?? app.limiterPresets[0] ?? null;

  useEffect(() => {
    if (!selectedName && app.limiterPresets.length) {
      setSelectedName(app.activeProfile?.name ?? app.limiterPresets[0].name);
    }
  }, [app.limiterPresets, app.activeProfile, selectedName]);

  useEffect(() => {
    if (!selected) return;
    setDraftProfile(selected);
    const match = matchSftp(app.sftpProfiles, selected.name);
    setSftpDraft(
      match
        ? {
            name: match.name,
            site: match.site,
            host: match.host,
            port: match.port,
            username: match.username,
            remote_dir: match.remote_dir,
            password: "",
          }
        : { ...BLANK_SFTP_DRAFT, name: selected.name, site: selected.name === "Adobe Stock" ? "adobe_stock" : "generic" }
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected?.name, app.sftpProfiles]);

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
  const existingSftp = matchSftp(app.sftpProfiles, selected.name);
  const keywordCreds =
    selected.name === "Shutterstock"
      ? app.keywordConfig.shutterstock
      : selected.name === "Adobe Stock"
        ? app.keywordConfig.adobe_stock
        : undefined;
  const supportsKeywordProvider = selected.name === "Shutterstock" || selected.name === "Adobe Stock";

  async function saveProfile() {
    setSaving(true);
    try {
      await app.saveActiveProfile(draftProfile!);
    } finally {
      setSaving(false);
    }
  }

  async function saveTransport() {
    setSaving(true);
    try {
      if (existingSftp) await app.editSftpProfile(existingSftp.id, sftpDraft);
      else await app.addSftpProfile(sftpDraft);
    } catch (err) {
      app.setStatus(`Could not save transport: ${String(err)}`);
    } finally {
      setSaving(false);
    }
  }

  function toggleKeywordProvider(enabled: boolean) {
    const next = { ...app.keywordConfig };
    if (selected.name === "Shutterstock") next.shutterstock = enabled ? next.shutterstock ?? { api_key: "", base_url: "" } : null;
    if (selected.name === "Adobe Stock") next.adobe_stock = enabled ? next.adobe_stock ?? { api_key: "", base_url: "" } : null;
    void app.saveKeywordConfig(next);
  }

  function patchKeywordKey(api_key: string) {
    const next = { ...app.keywordConfig };
    if (selected.name === "Shutterstock") next.shutterstock = { api_key, base_url: "" };
    if (selected.name === "Adobe Stock") next.adobe_stock = { api_key, base_url: "" };
    void app.saveKeywordConfig(next);
  }

  const enabledCount = app.enabledProfileNames.size;

  return (
    <div className="app-shell">
      <NavBar right={<button className="btn-primary" disabled title="Custom site profiles aren't supported yet">Add site profile</button>} />
      <div className="sites-screen">
        <div className="sites-list">
          {app.limiterPresets.map((p) => {
            const sftp = matchSftp(app.sftpProfiles, p.name);
            const enabled = app.enabledProfileNames.has(p.name);
            const statusColor = !enabled ? "var(--off)" : sftp?.host_key_fingerprint ? "var(--ok)" : sftp ? "var(--warn)" : "var(--off)";
            const statusWord = !enabled ? "off" : sftp?.host_key_fingerprint ? "connected" : sftp ? "not yet connected" : "no transport";
            return (
              <div
                key={p.name}
                className={`site-row${p.name === selected.name ? " site-row--selected" : ""}`}
                onClick={() => setSelectedName(p.name)}
              >
                <span className="site-status-dot" style={{ background: statusColor }} />
                <div className="site-row-body">
                  <span className="site-row-name">
                    {p.name} <span className="site-row-status">{statusWord}</span>
                  </span>
                  <span className="site-row-limits">
                    title ≤ {p.max_title_chars} · desc ≤ {p.max_description_chars} · {p.min_keywords}–{p.max_keywords} keywords
                  </span>
                  <span className="site-row-transport">{sftp ? `SFTP · ${sftp.host}` : "not configured"}</span>
                </div>
              </div>
            );
          })}
          <p className="sites-list-note">Metadata is validated against every enabled profile before a file can be approved.</p>
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

          <div className="card">
            <SectionLabel>Upload transport</SectionLabel>
            <div className="transport-grid" style={{ marginTop: 12 }}>
              <div className="field">
                <span className="field-label">Host</span>
                <input
                  className="mono"
                  value={sftpDraft.host}
                  onChange={(e) => setSftpDraft({ ...sftpDraft, host: e.target.value })}
                  placeholder="sftp.example.com"
                />
              </div>
              <div className="field">
                <span className="field-label">Port</span>
                <input
                  type="number"
                  value={sftpDraft.port}
                  onChange={(e) => setSftpDraft({ ...sftpDraft, port: Number(e.target.value) || 22 })}
                />
              </div>
              <div className="field">
                <span className="field-label">Username</span>
                <input value={sftpDraft.username} onChange={(e) => setSftpDraft({ ...sftpDraft, username: e.target.value })} />
              </div>
              <div className="field">
                <span className="field-label">Remote directory</span>
                <input value={sftpDraft.remote_dir} onChange={(e) => setSftpDraft({ ...sftpDraft, remote_dir: e.target.value })} />
              </div>
              <div className="field">
                <span className="field-label">Password {existingSftp && <span className="hint">(blank = keep existing)</span>}</span>
                <input
                  type="password"
                  autoComplete="off"
                  value={sftpDraft.password}
                  onChange={(e) => setSftpDraft({ ...sftpDraft, password: e.target.value })}
                />
              </div>
            </div>
            <div style={{ marginTop: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button className="btn-primary" onClick={() => void saveTransport()} disabled={saving || !sftpDraft.host || !sftpDraft.username}>
                {existingSftp ? "Save transport" : "Add transport"}
              </button>
              <button className="btn-secondary" disabled title="Connection testing verifies on first upload for now">
                Test connection
              </button>
            </div>
            {existingSftp && (
              <div className="test-result">
                <span
                  className="test-result-dot"
                  style={{ background: existingSftp.host_key_fingerprint ? "var(--ok)" : "var(--off)" }}
                />
                <span style={{ color: existingSftp.host_key_fingerprint ? "var(--ok-ink)" : "var(--faint)" }}>
                  {existingSftp.host_key_fingerprint ? `Host key verified · fingerprint ${existingSftp.host_key_fingerprint.slice(0, 16)}…` : "Not yet connected — verifies on first upload."}
                </span>
              </div>
            )}
          </div>

          <div className="card">
            <SectionLabel>Demand data used for keyword heat</SectionLabel>
            {!supportsKeywordProvider ? (
              <p className="demand-note" style={{ marginTop: 10 }}>
                Keyword suggestions aren't available for {selected.name} yet — only Shutterstock and Adobe Stock are
                wired up.
              </p>
            ) : (
              <>
                <div style={{ display: "flex", alignItems: "center", gap: 10, marginTop: 10 }}>
                  <span className={`pill${keywordCreds ? " pill--active" : ""}`}>{selected.name} search volume</span>
                  <Toggle checked={Boolean(keywordCreds)} onChange={toggleKeywordProvider} />
                </div>
                {keywordCreds && (
                  <div className="field" style={{ marginTop: 10, maxWidth: 320 }}>
                    <span className="field-label">API key</span>
                    <input type="password" autoComplete="off" value={keywordCreds.api_key} onChange={(e) => patchKeywordKey(e.target.value)} />
                  </div>
                )}
                <p className="demand-note" style={{ marginTop: 10 }}>Used for keyword heat's demand half and the "Enrich +" action.</p>
              </>
            )}
          </div>
        </div>
      </div>
      <StatusStrip>
        {enabledCount} of {app.limiterPresets.length} profiles enabled · strictest limit wins when several are enabled
      </StatusStrip>
    </div>
  );
}
