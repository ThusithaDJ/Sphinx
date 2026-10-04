import { useEffect, useState } from "react";
import { getTauriVersion, getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { useApp, type ToolStatus } from "../state/AppContext";

const GITHUB_URL = "https://github.com/ThusithaDJ";
const COFFEE_URL = "https://www.buymeacoffee.com/thusithajaw";

/** Links open in the system browser; inside the Tauri webview a plain
 * target="_blank" link would go nowhere. */
function openExternal(e: React.MouseEvent<HTMLAnchorElement>, url: string) {
  e.preventDefault();
  void openUrl(url).catch(() => window.open(url, "_blank", "noopener"));
}

const PIPELINE = [
  { name: "Import", detail: "Drag-drop, file picker or folder watch; BLAKE3 hashing de-duplicates files." },
  { name: "Analyze", detail: "Cloud (OpenAI, Gemini, Anthropic) or local (Ollama) vision models describe each asset." },
  { name: "Generate", detail: "Title, description and keywords shaped to each stock site's limits." },
  { name: "Enrich", detail: "Optional keyword demand lookups against stock-site APIs." },
  { name: "Embed", detail: "IPTC/XMP written into the file with exiftool, or exported as CSV." },
  { name: "Upload", detail: "Delivered to each site over SFTP or FTPS with retry and backoff." },
];

function ToolLine({ name, status }: { name: string; status: ToolStatus }) {
  const color = status.checking ? "var(--faint)" : status.ok ? "var(--ok-ink)" : "var(--danger)";
  return (
    <div className="tool-row">
      <span className="tool-status-dot" style={{ background: color }} />
      <span style={{ fontWeight: 600, minWidth: 80 }}>{name}</span>
      <span style={{ color: "var(--muted)", fontSize: 13 }}>{status.detail}</span>
    </div>
  );
}

export function AboutScreen() {
  const app = useApp();
  const [version, setVersion] = useState("…");
  const [tauriVersion, setTauriVersion] = useState("…");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion("unknown"));
    getTauriVersion().then(setTauriVersion).catch(() => setTauriVersion("unknown"));
  }, []);

  return (
    <div className="app-shell">
      <NavBar />
      <div className="about-screen">
        <div className="about-content">
          <div className="about-hero">
            <span className="about-wordmark">Sphinx</span>
            <span className="about-version">Version {version}</span>
            <p className="about-tagline">
              AI-assisted metadata for stock photo and video contributors: import local media, analyze it with
              vision models, generate titles, descriptions and keywords, embed IPTC/XMP, and deliver to stock sites
              from one desktop app.
            </p>
          </div>

          <div className="card">
            <h3 className="card-title" style={{ marginBottom: 12 }}>
              Pipeline
            </h3>
            <ol className="about-pipeline">
              {PIPELINE.map((step) => (
                <li key={step.name}>
                  <span className="about-step-name">{step.name}</span>
                  <span className="about-step-detail">{step.detail}</span>
                </li>
              ))}
            </ol>
          </div>

          <div className="card" style={{ padding: 0 }}>
            <h3 className="card-title" style={{ padding: "16px 18px 4px" }}>
              Local tools
            </h3>
            <ToolLine name="ffmpeg" status={app.ffmpeg} />
            <ToolLine name="exiftool" status={app.exiftool} />
          </div>

          <div className="card">
            <h3 className="card-title" style={{ marginBottom: 12 }}>
              System
            </h3>
            <dl className="about-facts">
              <dt>App version</dt>
              <dd>{version}</dd>
              <dt>Tauri runtime</dt>
              <dd>{tauriVersion}</dd>
              <dt>GPU</dt>
              <dd>{app.gpu ? (app.gpu.available ? `${app.gpu.backend} · ${app.gpu.detail}` : "not detected") : "checking…"}</dd>
              <dt>Library</dt>
              <dd>{app.assetTotal} assets</dd>
            </dl>
          </div>

          <div className="card">
            <h3 className="card-title" style={{ marginBottom: 12 }}>
              Developer
            </h3>
            <div className="about-links">
              <a className="about-github" href={GITHUB_URL} onClick={(e) => openExternal(e, GITHUB_URL)}>
                <svg width="18" height="18" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                  <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
                </svg>
                github.com/ThusithaDJ
              </a>
              <a href={COFFEE_URL} onClick={(e) => openExternal(e, COFFEE_URL)} title="Support Sphinx on Buy Me a Coffee">
                <img
                  src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png"
                  alt="Buy Me a Coffee"
                  className="about-coffee"
                />
              </a>
            </div>
          </div>

          <p className="about-footer">Built with Tauri, React and Rust. © {new Date().getFullYear()} Thusitha Jayasundara.</p>
        </div>
      </div>
      <StatusStrip />
    </div>
  );
}
