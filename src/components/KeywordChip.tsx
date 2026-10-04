import { USER_ADDED_COLORS, heatColors, keywordHeat, type Keyword } from "../lib/heat";

/** Compact chip for inspectors/triage: word + a small mono score. */
export function CompactChip({ kw }: { kw: Keyword }) {
  const score = keywordHeat(kw);
  const colors = heatColors(score ?? 0);
  return (
    <span
      className="kw-chip kw-chip--compact"
      style={{ background: colors.bg, borderColor: colors.border, color: colors.text }}
    >
      {kw.word}
      <span className="kw-score">{score ?? "—"}</span>
    </span>
  );
}

/** Draggable, removable chip for the asset editor's keyword field. */
export function EditableChip({
  kw,
  index,
  isTopByHeat,
  onRemove,
  dragProps,
}: {
  kw: Keyword;
  index: number;
  isTopByHeat: boolean;
  onRemove: () => void;
  dragProps: React.HTMLAttributes<HTMLSpanElement>;
}) {
  const score = keywordHeat(kw);
  const colors = kw.userAdded ? USER_ADDED_COLORS : heatColors(score ?? 0);
  // Merge rather than let dragProps.className (e.g. "kw-dragging") replace
  // the chip's own classes -- spreading it last used to wipe all chip styling.
  const { className: extraClass, ...rest } = dragProps;
  return (
    <span
      {...rest}
      className={`kw-chip kw-chip--editable${extraClass ? ` ${extraClass}` : ""}`}
      style={{ background: colors.bg, borderColor: colors.border, color: colors.text }}
      data-index={index}
    >
      <span className={`kw-handle${isTopByHeat ? " kw-handle--top" : ""}`}>⠿</span>
      <span className="kw-word">{kw.word}</span>
      {kw.userAdded ? null : <span className="kw-score">{score ?? "—"}</span>}
      <button className="kw-remove" onClick={onRemove} aria-label={`Remove ${kw.word}`}>
        ×
      </button>
    </span>
  );
}

export function RejectedChip({ word, onRestore }: { word: string; onRestore: () => void }) {
  return (
    <button className="kw-chip kw-chip--rejected" onClick={onRestore} title="Click to restore">
      {word}
    </button>
  );
}

export function AddKeywordChip({
  value,
  onChange,
  onAdd,
}: {
  value: string;
  onChange: (v: string) => void;
  onAdd: (words: string[]) => void;
}) {
  function commit() {
    const words = value
      .split(",")
      .map((w) => w.trim())
      .filter(Boolean);
    if (words.length) onAdd(words);
    onChange("");
  }
  return (
    <input
      className="kw-add"
      placeholder="+ add keyword"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === ",") {
          e.preventDefault();
          commit();
        }
      }}
      onBlur={commit}
    />
  );
}
