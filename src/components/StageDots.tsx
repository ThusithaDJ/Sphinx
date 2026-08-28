import { STAGE_NAMES, type Stage } from "../state/AppContext";

const TRACK = ["Analyze", "Generate", "Enrich", "Embed", "Upload"];

export function StageDots({
  stage,
  blocked = false,
  showLabel = true,
}: {
  stage: Stage;
  blocked?: boolean;
  showLabel?: boolean;
}) {
  return (
    <span className="stage-dots">
      <span className="stage-dots-track">
        {TRACK.map((name, i) => {
          const done = i < stage;
          return (
            <span
              key={name}
              className={`stage-dot${done ? (blocked ? " stage-dot--blocked" : " stage-dot--done") : ""}`}
              title={name}
            />
          );
        })}
      </span>
      {showLabel && <span className="stage-label">{STAGE_NAMES[stage]}</span>}
    </span>
  );
}
