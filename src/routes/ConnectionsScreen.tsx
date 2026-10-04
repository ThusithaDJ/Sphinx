import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { Toggle, SectionLabel } from "../components/Toggle";
import { useApp, PROVIDER_LABELS } from "../state/AppContext";
import type { ProviderKind } from "../lib/api";

const SIDEBAR_ITEMS = [
  { id: "ai-provider", label: "AI provider" },
  { id: "local-tools", label: "Local tools & runtime" },
  { id: "keyword-apis", label: "Keyword demand APIs" },
];

/** Sites with a keyword-demand API wired up, keyed into `keywordConfig`. */
const KEYWORD_PROVIDERS = [
  { key: "shutterstock", name: "Shutterstock" },
  { key: "adobe_stock", name: "Adobe Stock" },
] as const;

type KeywordProviderKey = (typeof KEYWORD_PROVIDERS)[number]["key"];

export function ConnectionsScreen() {
  const app = useApp();
  const isOllama = app.analysisConfig.provider === "ollama";
  const [active, setActive] = useState("ai-provider");
  const contentRef = useRef<HTMLDivElement>(null);
  const sectionRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const [savingAnalysis, setSavingAnalysis] = useState(false);

  useEffect(() => {
    const root = contentRef.current;
    if (!root) return;
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (visible[0]) setActive(visible[0].target.id);
      },
      { root, threshold: 0.3 }
    );
    for (const id of Object.keys(sectionRefs.current)) {
      const el = sectionRefs.current[id];
      if (el) observer.observe(el);
    }
    return () => observer.disconnect();
  }, []);

  function scrollTo(id: string) {
    sectionRefs.current[id]?.scrollIntoView({ behavior: "smooth", block: "start" });
    setActive(id);
  }

  async function handleSaveAnalysis() {
    setSavingAnalysis(true);
    try {
      await app.saveAnalysisConfig();
    } finally {
      setSavingAnalysis(false);
    }
  }

  async function pickExiftool() {
    const selection = await open({ multiple: false });
    if (!selection || Array.isArray(selection)) return;
    await app.saveEmbedConfig({ exiftool_path: selection });
  }

  async function pickFfmpeg() {
    const selection = await open({ multiple: false });
    if (!selection || Array.isArray(selection)) return;
    await app.saveVideoConfig({ ...app.videoConfig, ffmpeg_path: selection }, app.transcriptionConfig);
  }

  function toggleKeywordProvider(key: KeywordProviderKey, enabled: boolean) {
    const next = { ...app.keywordConfig };
    next[key] = enabled ? next[key] ?? { api_key: "", base_url: "" } : null;
    void app.saveKeywordConfig(next);
  }

  function patchKeywordKey(key: KeywordProviderKey, api_key: string) {
    const next = { ...app.keywordConfig };
    next[key] = { api_key, base_url: "" };
    void app.saveKeywordConfig(next);
  }

  const accessRows = [
    { label: `${PROVIDER_LABELS[app.analysisConfig.provider]} key`, note: "Vision analysis · every asset", connected: app.hasKey },
    ...app.sftpProfiles.map((p) => ({
      label: `${p.name} ${p.protocol.toUpperCase()}`,
      note: "Upload delivery · configured in Sites",
      connected: Boolean(p.host_key_fingerprint),
    })),
    ...(app.keywordConfig.shutterstock ? [{ label: "Shutterstock demand key", note: "Keyword enrich", connected: Boolean(app.keywordConfig.shutterstock.api_key) }] : []),
    ...(app.keywordConfig.adobe_stock ? [{ label: "Adobe Stock demand key", note: "Keyword enrich", connected: Boolean(app.keywordConfig.adobe_stock.api_key) }] : []),
  ];

  return (
    <div className="app-shell">
      <NavBar right={<span style={{ fontSize: 12, color: "var(--faint)" }}>Saved automatically to sphinx.db</span>} />
      <div className="settings-screen">
        <div className="settings-sidebar">
          {SIDEBAR_ITEMS.map((item) => (
            <button
              key={item.id}
              className={`settings-sidebar-item${active === item.id ? " settings-sidebar-item--active" : ""}`}
              onClick={() => scrollTo(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div className="settings-content settings-content--full" ref={contentRef}>
          <div className="card">
            <span className="section-label" style={{ display: "block", marginBottom: 10 }}>
              What has access
            </span>
            <div className="access-summary">
              {accessRows.map((row) => (
                <div className="access-summary-row" key={row.label}>
                  <span className="access-summary-label">{row.label}</span>
                  <span className="access-summary-note">{row.note}</span>
                  <span style={{ color: row.connected ? "var(--ok-ink)" : "var(--danger)" }}>
                    {row.connected ? "connected" : "not configured"}
                  </span>
                </div>
              ))}
              {accessRows.length === 0 && <div className="empty-note">Nothing configured yet.</div>}
            </div>
          </div>

          <div
            className="card"
            id="ai-provider"
            ref={(el) => {
              sectionRefs.current["ai-provider"] = el;
            }}
          >
            <div className="settings-card-head">
              <h3 className="card-title">AI provider</h3>
              {app.hasKey && <span className="chip-tag chip-tag--ok">connected</span>}
            </div>
            <div className="field-grid" style={{ gridTemplateColumns: "1fr 1fr" }}>
              <div className="field">
                <span className="field-label">Provider</span>
                <select
                  value={app.analysisConfig.provider}
                  onChange={(e) => app.switchAnalysisProvider(e.target.value as ProviderKind)}
                >
                  {(Object.keys(PROVIDER_LABELS) as ProviderKind[]).map((p) => (
                    <option key={p} value={p}>
                      {PROVIDER_LABELS[p]}
                      {app.savedAnalysisConfigs[p] ? " (configured)" : ""}
                    </option>
                  ))}
                </select>
              </div>
              <div className="field">
                <span className="field-label">
                  Vision model <span className="hint">(blank = provider default)</span>
                </span>
                <input
                  value={app.analysisConfig.model}
                  onChange={(e) => app.patchAnalysisConfig({ model: e.target.value })}
                  placeholder={isOllama ? "e.g. llava, qwen2-vl" : "e.g. gpt-4o"}
                />
              </div>
              <div className="field">
                <span className="field-label">
                  API key {isOllama && <span className="hint">(optional — only for a proxied/authenticated server)</span>}
                </span>
                <input
                  type="password"
                  autoComplete="off"
                  value={app.analysisConfig.api_key}
                  onChange={(e) => app.patchAnalysisConfig({ api_key: e.target.value })}
                  placeholder={isOllama ? "leave blank for a local server" : "sk-live-…"}
                />
              </div>
              <div className="field">
                <span className="field-label">
                  {isOllama ? "Server URL" : "API base URL"} <span className="hint">(blank = provider default)</span>
                </span>
                <input
                  value={app.analysisConfig.base_url}
                  onChange={(e) => app.patchAnalysisConfig({ base_url: e.target.value })}
                  placeholder={isOllama ? "http://localhost:11434" : "proxy / gateway override"}
                />
              </div>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 14 }}>
              <button className="btn-primary" onClick={() => void handleSaveAnalysis()} disabled={savingAnalysis}>
                {savingAnalysis ? "Saving…" : "Save"}
              </button>
              <button className="btn-secondary" disabled title="A live test request isn't wired up yet">
                Test request
              </button>
            </div>
            {isOllama ? (
              <>
                <p className="settings-footer-note">
                  {app.ollama.checking
                    ? "Checking the local Ollama server…"
                    : app.ollama.ok
                    ? `Ollama reachable — ${app.ollama.detail}`
                    : `Ollama not reachable at this server URL — ${app.ollama.detail}`}
                </p>
                {app.gpu && !app.gpu.available && (
                  <p className="settings-footer-note" style={{ color: "var(--warn-ink)" }}>
                    {app.gpu.detail}
                  </p>
                )}
              </>
            ) : (
              <p className="settings-footer-note">
                Key is stored locally in sphinx.db and never leaves this machine except to the provider.
              </p>
            )}
          </div>

          <div
            className="card"
            id="local-tools"
            style={{ padding: "15px 0 11px" }}
            ref={(el) => {
              sectionRefs.current["local-tools"] = el;
            }}
          >
            <h3 className="card-title" style={{ padding: "0 18px" }}>
              Local tools &amp; runtime
            </h3>
            <div className="tool-row">
              <span className="tool-status-dot" style={{ background: app.exiftool.checking ? "var(--off)" : app.exiftool.ok ? "var(--ok)" : "var(--warn)" }} />
              <div className="tool-row-body">
                <span className="tool-name">exiftool</span>
                <span className="tool-detail">{app.embedConfig.exiftool_path || "on PATH"} · {app.exiftool.detail}</span>
              </div>
              <button className="btn-quiet" onClick={() => void pickExiftool()}>
                Change
              </button>
            </div>
            <div className="tool-row">
              <span className="tool-status-dot" style={{ background: app.ffmpeg.checking ? "var(--off)" : app.ffmpeg.ok ? "var(--ok)" : "var(--warn)" }} />
              <div className="tool-row-body">
                <span className="tool-name">
                  ffmpeg <span style={{ fontSize: 11.5, color: "var(--warn-ink)", fontWeight: 400 }}>required for video</span>
                </span>
                <span className="tool-detail">
                  {app.ffmpeg.ok ? `${app.videoConfig.ffmpeg_path || "on PATH"} · ${app.ffmpeg.detail}` : "not found in PATH — video assets stay blocked"}
                </span>
              </div>
              <button className="btn-secondary" style={{ color: "var(--accent)" }} onClick={() => void pickFfmpeg()}>
                Locate…
              </button>
            </div>
            <div className="tool-row">
              <span
                className="tool-status-dot"
                style={{ background: app.analysisConfig.provider !== "ollama" ? "var(--off)" : app.ollama.ok ? "var(--ok)" : "var(--warn)" }}
              />
              <div className="tool-row-body">
                <span className="tool-name">Ollama (local model)</span>
                <span className="tool-detail">
                  {app.analysisConfig.provider !== "ollama" ? "not in use" : app.ollama.ok ? app.ollama.detail : app.ollama.detail}
                  {app.gpu && !app.gpu.available ? " · no GPU detected on this machine" : ""}
                </span>
              </div>
              <button className="btn-quiet" onClick={() => scrollTo("ai-provider")}>
                Connect…
              </button>
            </div>
          </div>

          <div
            className="card"
            id="keyword-apis"
            ref={(el) => {
              sectionRefs.current["keyword-apis"] = el;
            }}
          >
            <SectionLabel>Keyword demand APIs</SectionLabel>
            {KEYWORD_PROVIDERS.map(({ key, name }) => {
              const creds = app.keywordConfig[key];
              return (
                <div key={key} style={{ marginTop: 10 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span className={`pill${creds ? " pill--active" : ""}`}>{name} search volume</span>
                    <Toggle checked={Boolean(creds)} onChange={(enabled) => toggleKeywordProvider(key, enabled)} />
                  </div>
                  {creds && (
                    <div className="field" style={{ marginTop: 10, maxWidth: 320 }}>
                      <span className="field-label">API key</span>
                      <input type="password" autoComplete="off" value={creds.api_key} onChange={(e) => patchKeywordKey(key, e.target.value)} />
                    </div>
                  )}
                </div>
              );
            })}
            <p className="demand-note" style={{ marginTop: 10 }}>
              Used for keyword heat's demand half and the "Enrich +" action. Only Shutterstock and Adobe Stock are wired up
              so far. Upload delivery (SFTP/FTPS) is configured per site in{" "}
              <button className="btn-link" style={{ display: "inline" }} onClick={() => app.setScreen("sites")}>
                Sites →
              </button>
            </p>
          </div>
        </div>
      </div>
      <StatusStrip>Sphinx 0.1.0 · Tauri 2.0 · settings written on change</StatusStrip>
    </div>
  );
}
