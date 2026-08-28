export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`toggle${checked ? " toggle--on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle-knob" />
    </button>
  );
}

export function CharCounter({
  value,
  limit,
  siteName,
}: {
  value: number;
  limit: number;
  siteName?: string;
}) {
  const ratio = limit > 0 ? value / limit : 0;
  const cls = value > limit ? "char-counter--over" : ratio >= 0.9 ? "char-counter--near" : "";
  return (
    <span className={`char-counter ${cls}`}>
      {value} / {limit}
      {siteName ? ` ${siteName} limit` : ""}
    </span>
  );
}

export function SectionLabel({ children, count }: { children: React.ReactNode; count?: number | string }) {
  return (
    <span className="section-label">
      {children}
      {count !== undefined && <span className="section-label-count"> {count}</span>}
    </span>
  );
}
