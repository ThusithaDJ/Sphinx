import type { ReactNode } from "react";

/** 26px status strip. Screen-specific, and hidden entirely when there's nothing true to say. */
export function StatusStrip({ children }: { children?: ReactNode }) {
  if (!children) return <div className="status-strip" />;
  return <div className="status-strip">{children}</div>;
}

export function Sep() {
  return <span className="status-sep">·</span>;
}
