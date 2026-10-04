import { useEffect, useRef, useState } from "react";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { useApp } from "../state/AppContext";

const SIDEBAR_ITEMS = [
  { id: "prompt-guidance", label: "Prompt & guidance" },
  { id: "video-pipeline", label: "Video pipeline" },
  { id: "storage", label: "Storage & data" },
  { id: "shortcuts", label: "Shortcuts" },
];

export function SettingsScreen() {
  const app = useApp();
  const [active, setActive] = useState("prompt-guidance");
  const contentRef = useRef<HTMLDivElement>(null);
  const sectionRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const [savingAnalysis, setSavingAnalysis] = useState(false);
  const [savingVideo, setSavingVideo] = useState(false);
  const [maxKeyframes, setMaxKeyframes] = useState(app.videoConfig.max_keyframes);
  const [sceneThreshold, setSceneThreshold] = useState(app.videoConfig.scene_threshold);
  const [transcribe, setTranscribe] = useState(app.transcriptionConfig.enabled);

  useEffect(() => {
    setMaxKeyframes(app.videoConfig.max_keyframes);
    setSceneThreshold(app.videoConfig.scene_threshold);
    setTranscribe(app.transcriptionConfig.enabled);
  }, [app.videoConfig, app.transcriptionConfig]);

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

  async function handleSaveVideo() {
    setSavingVideo(true);
    try {
      await app.saveVideoConfig(
        { ...app.videoConfig, max_keyframes: maxKeyframes, scene_threshold: sceneThreshold },
        { ...app.transcriptionConfig, enabled: transcribe }
      );
      app.setStatus("Saved video pipeline settings.");
    } finally {
      setSavingVideo(false);
    }
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
            id="video-pipeline"
            ref={(el) => {
              sectionRefs.current["video-pipeline"] = el;
            }}
          >
            <h3 className="card-title" style={{ marginBottom: 14 }}>
              Video pipeline
            </h3>
            <div className="field-grid" style={{ gridTemplateColumns: "repeat(2, 1fr)" }}>
              <div className="field">
                <span className="field-label">
                  Max keyframes <span className="hint">(per video, at scene changes)</span>
                </span>
                <input
                  type="number"
                  min={1}
                  value={maxKeyframes}
                  onChange={(e) => setMaxKeyframes(Number(e.target.value) || 1)}
                />
              </div>
              <div className="field">
                <span className="field-label">
                  Scene threshold <span className="hint">(0–1, lower = more keyframes)</span>
                </span>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={sceneThreshold}
                  onChange={(e) => setSceneThreshold(Number(e.target.value) || 0)}
                />
              </div>
            </div>
            <label className="embed-target-row" style={{ marginTop: 12 }}>
              <input type="checkbox" checked={transcribe} onChange={(e) => setTranscribe(e.target.checked)} />
              Transcribe audio alongside keyframes
            </label>
            <div style={{ marginTop: 14 }}>
              <button className="btn-primary" onClick={() => void handleSaveVideo()} disabled={savingVideo}>
                {savingVideo ? "Saving…" : "Save"}
              </button>
            </div>
            <p className="settings-footer-note">ffmpeg's own path is managed under Connections → Local tools & runtime.</p>
          </div>

          <div
            className="card"
            id="storage"
            style={{ padding: "15px 0 11px" }}
            ref={(el) => {
              sectionRefs.current["storage"] = el;
            }}
          >
            <h3 className="card-title" style={{ padding: "0 18px" }}>
              Storage & data
            </h3>
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

          <div
            className="card"
            id="shortcuts"
            ref={(el) => {
              sectionRefs.current["shortcuts"] = el;
            }}
          >
            <h3 className="card-title" style={{ marginBottom: 14 }}>
              Shortcuts
            </h3>
            <div className="field-grid" style={{ gridTemplateColumns: "1fr 1fr" }}>
              <div className="field">
                <span className="field-label">Library → Triage</span>
                <span className="settings-footer-note" style={{ margin: 0 }}>
                  J / K move · Enter approve · E edit · R reject · S skip
                </span>
              </div>
              <div className="field">
                <span className="field-label">Library → Asset detail</span>
                <span className="settings-footer-note" style={{ margin: 0 }}>
                  Ctrl+S save · Ctrl+Enter approve and next · Alt+1–4 preview site
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
      <StatusStrip>Sphinx 0.1.0 · Tauri 2.0 · settings written on change</StatusStrip>
    </div>
  );
}
