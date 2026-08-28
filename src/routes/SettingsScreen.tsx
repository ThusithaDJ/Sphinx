import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { useApp, PROVIDER_LABELS } from "../state/AppContext";
import type { ProviderKind } from "../lib/api";

const SIDEBAR_ITEMS = [
  { id: "ai-provider", label: "AI provider" },
  { id: "prompt-guidance", label: "Prompt & guidance" },
  { id: "local-tools", label: "Local tools" },
];

export function SettingsScreen() {
  const app = useApp();
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

        <div className="settings-content" ref={contentRef}>
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
                  placeholder="e.g. gpt-4o"
                />
              </div>
              <div className="field">
                <span className="field-label">API key</span>
                <input
                  type="password"
                  autoComplete="off"
                  value={app.analysisConfig.api_key}
                  onChange={(e) => app.patchAnalysisConfig({ api_key: e.target.value })}
                  placeholder="sk-live-…"
                />
              </div>
              <div className="field">
                <span className="field-label">
                  API base URL <span className="hint">(blank = provider default)</span>
                </span>
                <input
                  value={app.analysisConfig.base_url}
                  onChange={(e) => app.patchAnalysisConfig({ base_url: e.target.value })}
                  placeholder="proxy / gateway override"
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
            <p className="settings-footer-note">
              Key is stored locally in sphinx.db and never leaves this machine except to the provider.
            </p>
          </div>

          <div
            className="card"
            id="prompt-guidance"
            ref={(el) => {
              sectionRefs.current["prompt-guidance"] = el;
            }}
          >
            <h3 className="card-title" style={{ marginBottom: 14 }}>
              Prompt & guidance
            </h3>
            <div className="field">
              <span className="field-label">
                Project guidance <span className="hint">(appended to every request)</span>
              </span>
              <textarea
                rows={3}
                value={app.analysisConfig.prompt_extra}
                onChange={(e) => app.patchAnalysisConfig({ prompt_extra: e.target.value })}
                placeholder="e.g. Fine-art nature photography. Prefer species and place names over mood words."
              />
            </div>
            <div className="field-grid" style={{ gridTemplateColumns: "repeat(3, 1fr)", marginTop: 12 }}>
              <div className="field">
                <span className="field-label">Demand floor</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={app.demandFloor}
                  onChange={(e) => app.setDemandFloor(Number(e.target.value) || 0)}
                />
              </div>
              <div className="field">
                <span className="field-label">Max keywords (active profile)</span>
                <input value={app.activeProfile?.max_keywords ?? "—"} disabled />
              </div>
              <div className="field">
                <span className="field-label">Min keywords (active profile)</span>
                <input value={app.activeProfile?.min_keywords ?? "—"} disabled />
              </div>
            </div>
            <div style={{ marginTop: 14 }}>
              <button className="btn-primary" onClick={() => void handleSaveAnalysis()} disabled={savingAnalysis}>
                Save
              </button>
            </div>
            <p className="settings-footer-note">
              Keyword count limits are edited per site under Sites. Demand floor is session-only for now.
            </p>
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
              Local tools
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
              <span className="tool-status-dot" style={{ background: "var(--ok)" }} />
              <div className="tool-row-body">
                <span className="tool-name">sphinx.db</span>
                <span className="tool-detail">{app.assetTotal} assets</span>
              </div>
              <button className="btn-quiet" disabled title="The database path isn't exposed to the UI yet">
                Reveal
              </button>
            </div>
          </div>
        </div>
      </div>
      <StatusStrip>Sphinx 0.1.0 · Tauri 2.0 · settings written on change</StatusStrip>
    </div>
  );
}
