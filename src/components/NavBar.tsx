import type { ReactNode } from "react";
import { useApp, type Screen } from "../state/AppContext";

const DESTINATIONS: { id: Screen; label: string }[] = [
  { id: "import", label: "Import" },
  { id: "library", label: "Library" },
  { id: "review", label: "Review" },
  { id: "queue", label: "Queue" },
  { id: "sites", label: "Sites" },
  { id: "settings", label: "Settings" },
];

interface EditorHeader {
  fileName: string;
  needsReview: boolean;
  index: number;
  total: number;
  onBack: () => void;
}

export function NavBar({ right, editor }: { right?: ReactNode; editor?: EditorHeader }) {
  const app = useApp();

  return (
    <nav className="navbar">
      {editor ? (
        <div className="navbar-editor-head">
          <button className="link-action" onClick={editor.onBack}>
            ← Library
          </button>
          <span className="navbar-divider" />
          <span className="navbar-filename">{editor.fileName}</span>
          {editor.needsReview && <span className="chip-tag chip-tag--accent">needs review</span>}
          <span className="navbar-meta">
            asset {editor.index} of {editor.total}
          </span>
        </div>
      ) : (
        <>
          <div className="navbar-brand">
            <span className="navbar-wordmark">Sphinx</span>
            <span className="navbar-project">Default project</span>
          </div>
          <span className="navbar-divider" />
          <div className="navbar-destinations">
            {DESTINATIONS.map((d) => (
              <button
                key={d.id}
                className={`nav-dest${app.screen === d.id ? " nav-dest--active" : ""}`}
                onClick={() => app.setScreen(d.id)}
              >
                {d.label}
                {d.id === "library" && <span className="nav-count"> {app.assetTotal}</span>}
                {d.id === "review" && app.needsReviewIds.length > 0 && (
                  <span className="nav-count"> {app.needsReviewIds.length}</span>
                )}
                {d.id === "queue" && (app.jobCounts.running || app.jobCounts.pending) > 0 && (
                  <span className="nav-count"> {app.jobCounts.running + app.jobCounts.pending}</span>
                )}
              </button>
            ))}
          </div>
        </>
      )}
      <span className="navbar-spacer" />
      {right}
    </nav>
  );
}
