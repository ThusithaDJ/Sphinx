import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { NavBar } from "../components/NavBar";
import { StatusStrip } from "../components/StatusStrip";
import { CompactChip } from "../components/KeywordChip";
import { SectionLabel, CharCounter } from "../components/Toggle";
import { estimateKeywordScores } from "../lib/heat";
import { gradeAllSites, strictestTitleLimit } from "../lib/limits";
import { useApp } from "../state/AppContext";

const REWORK_FLAGS = ["wrong subject", "too generic", "trademark risk", "needs release", "editorial only"];

type Decision = "approved" | "rejected";

export function ReviewScreen() {
  const app = useApp();
  const queueIds = app.needsReviewIds;
  const [cursor, setCursor] = useState(0);
  const [decisions, setDecisions] = useState<Record<number, Decision>>({});
  const [flags, setFlags] = useState<Record<number, Set<string>>>({});
  const flagRailRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (cursor >= queueIds.length && queueIds.length > 0) setCursor(0);
  }, [queueIds.length, cursor]);

  const currentId = queueIds[cursor];
  const asset = app.assets.find((a) => a.id === currentId) ?? null;
  const meta = asset ? app.metadata[asset.id] : undefined;
  const keywords = useMemo(() => estimateKeywordScores(meta?.keywords ?? []), [meta]);
  const compliance = useMemo(
    () => (meta ? gradeAllSites(app.enabledSites, meta.title.length, meta.keywords.length) : []),
    [meta, app.enabledSites]
  );
  const titleLimit = useMemo(() => strictestTitleLimit(app.enabledSites), [app.enabledSites]);
  const belowFloor = keywords.filter((k) => (k.demand ?? 0) < app.demandFloor).length;
  const selectedFlags = currentId ? flags[currentId] ?? new Set<string>() : new Set<string>();

  function toggleFlag(id: number, flag: string) {
    setFlags((prev) => {
      const next = new Set(prev[id] ?? []);
      if (next.has(flag)) next.delete(flag);
      else next.add(flag);
      return { ...prev, [id]: next };
    });
  }

  async function approveCurrent() {
    if (!asset || !meta) return;
    await app.handleEmbed(asset);
    setDecisions((prev) => ({ ...prev, [asset.id]: "approved" }));
    advance();
  }

  function rejectCurrent() {
    if (!asset) return;
    if (selectedFlags.size === 0) {
      flagRailRef.current?.focus();
      app.setStatus("Select at least one rework flag before rejecting.");
      return;
    }
    setDecisions((prev) => ({ ...prev, [asset.id]: "rejected" }));
    advance();
  }

  function skipCurrent() {
    advance();
  }

  function advance() {
    setCursor((c) => Math.min(c + 1, queueIds.length));
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!asset) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        setCursor((c) => Math.min(c + 1, queueIds.length - 1));
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        setCursor((c) => Math.max(c - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        void approveCurrent();
      } else if (e.key === "e" || e.key === "E") {
        e.preventDefault();
        app.openEditor(asset.id);
      } else if (e.key === "r" || e.key === "R") {
        e.preventDefault();
        rejectCurrent();
      } else if (e.key === "s" || e.key === "S") {
        e.preventDefault();
        skipCurrent();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  });

  const enabledSiteNames = app.enabledSites.map((s) => s.name);
  const reviewedCount = Object.keys(decisions).length;

  return (
    <div className="app-shell">
      <NavBar
        right={
          <div className="navbar-right">
            <span style={{ fontSize: 12, color: "var(--muted)" }}>
              {reviewedCount} of {queueIds.length} reviewed
            </span>
            <div className="review-progress-track">
              <div
                className="review-progress-fill"
                style={{ width: `${queueIds.length ? (reviewedCount / queueIds.length) * 100 : 0}%` }}
              />
            </div>
          </div>
        }
      />
      <div className="review-screen">
        <div className="review-rail">
          {queueIds.map((id, i) => {
            const a = app.assets.find((x) => x.id === id);
            const decision = decisions[id];
            return (
              <div
                key={id}
                className={`review-thumb-wrap${i === cursor ? " review-thumb-wrap--current" : ""}`}
                onClick={() => setCursor(i)}
              >
                <div className="review-thumb">
                  {a?.media_type === "image" && <img src={convertFileSrc(a.path)} alt="" />}
                </div>
                {decision === "approved" && <span className="review-status-disc review-status-disc--approved">✓</span>}
                {i === cursor && !decision && <span className="review-status-disc review-status-disc--current">•</span>}
              </div>
            );
          })}
        </div>

        {!asset || !meta ? (
          <div className="review-center">
            <div className="empty-state" style={{ flex: 1, flexDirection: "column" }}>
              {queueIds.length === 0
                ? "Nothing needs review right now."
                : `${reviewedCount} reviewed · nothing left in the queue`}
            </div>
          </div>
        ) : (
          <div className="review-center">
            <div className="review-preview">
              {asset.media_type === "image" && <img src={convertFileSrc(asset.path)} alt="" />}
              <span className="review-caption">
                {asset.path.split(/[\\/]/).pop()} · {analysisSize(asset)} · {app.analyses[asset.id]?.model ?? "—"}
              </span>
            </div>
            <div className="proposal-card">
              <div className="editor-field-head">
                <SectionLabel>Proposed title</SectionLabel>
                {titleLimit && <CharCounter value={meta.title.length} limit={titleLimit.limit} siteName={titleLimit.site} />}
              </div>
              <p className="proposal-title">{meta.title}</p>
              <div className="chip-wrap">
                {keywords.slice(0, 12).map((kw) => (
                  <CompactChip key={kw.word} kw={kw} />
                ))}
                {keywords.length > 12 && <span className="empty-note">+{keywords.length - 12} more</span>}
              </div>
            </div>
          </div>
        )}

        <div className="review-checks" ref={flagRailRef} tabIndex={-1}>
          {asset && meta && (
            <>
              <div>
                <SectionLabel>Pre-approval checks</SectionLabel>
                <div style={{ display: "flex", flexDirection: "column", gap: 9, marginTop: 8 }}>
                  <CheckRow
                    label={`Title within all ${enabledSiteNames.length} enabled site limits`}
                    state={compliance.every((r) => r.title.verdict !== "fail") ? "pass" : "fail"}
                  />
                  <CheckRow
                    label={`${meta.keywords.length} keywords · above minimum of ${app.activeProfile?.min_keywords ?? 0}`}
                    state={meta.meets_minimum_keywords ? "pass" : "fail"}
                  />
                  <CheckRow
                    label={`${belowFloor} keyword${belowFloor === 1 ? "" : "s"} below demand floor`}
                    state={belowFloor === 0 ? "pass" : "warn"}
                  />
                  <CheckRow
                    label={app.analyses[asset.id]?.editorial ? "Flagged editorial-only, not commercial-safe" : "Commercial-safe per analysis"}
                    state={app.analyses[asset.id]?.editorial ? "warn" : "pass"}
                  />
                </div>
              </div>

              <div>
                <SectionLabel>Flag for rework</SectionLabel>
                <div className="flag-pills" style={{ marginTop: 8 }}>
                  {REWORK_FLAGS.map((f) => (
                    <button
                      key={f}
                      className={`outline-pill${selectedFlags.has(f) ? " outline-pill--active" : ""}`}
                      onClick={() => toggleFlag(asset.id, f)}
                    >
                      {f}
                    </button>
                  ))}
                </div>
              </div>

              <div className="consequence-note">
                Approving embeds IPTC/XMP into the file and queues the upload for{" "}
                {enabledSiteNames.length ? enabledSiteNames.map((n, i) => <strong key={n}>{i ? ", " + n : n}</strong>) : "no enabled sites"}
                .
              </div>

              <div className="review-actions">
                <button className="btn-primary" onClick={() => void approveCurrent()}>
                  Approve & next ⏎
                </button>
                <div className="review-actions-row">
                  <button className="btn-secondary" onClick={() => app.openEditor(asset.id)}>
                    Edit  E
                  </button>
                  <button className="btn-secondary btn-destructive" onClick={rejectCurrent}>
                    Reject  R
                  </button>
                  <button className="btn-secondary" onClick={skipCurrent}>
                    Skip  S
                  </button>
                </div>
              </div>
            </>
          )}
        </div>
      </div>
      <StatusStrip>J / K move · ⏎ approve · R reject · {Math.max(0, queueIds.length - cursor)} assets left in queue</StatusStrip>
    </div>
  );
}

function analysisSize(asset: { size: number }): string {
  return `${(asset.size / (1024 * 1024)).toFixed(1)} MB`;
}

function CheckRow({ label, state }: { label: string; state: "pass" | "warn" | "fail" }) {
  const color = state === "pass" ? "var(--ok)" : state === "warn" ? "var(--warn)" : "var(--danger)";
  return (
    <div className="check-row">
      <span className="check-dot" style={{ background: color }} />
      <span className="check-label">{label}</span>
      <span className="check-state" style={{ color }}>
        {state}
      </span>
    </div>
  );
}
