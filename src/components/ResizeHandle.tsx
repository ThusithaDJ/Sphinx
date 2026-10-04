import { useCallback, useEffect, useState } from "react";

function readStored(key: string): number | null {
  try {
    const raw = localStorage.getItem(key);
    const n = raw == null ? NaN : Number(raw);
    return Number.isFinite(n) ? n : null;
  } catch {
    return null;
  }
}

/** Width of a side panel that the user can drag wider, from `minWidth` up to
 * half the window. The chosen width is remembered per `storageKey`. */
export function usePanelWidth(storageKey: string, minWidth: number) {
  const clamp = useCallback(
    (w: number) => Math.round(Math.min(Math.max(w, minWidth), Math.max(minWidth, window.innerWidth / 2))),
    [minWidth]
  );
  const [width, setWidth] = useState(() => clamp(readStored(storageKey) ?? minWidth));

  useEffect(() => {
    const onResize = () => setWidth((w) => clamp(w));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [clamp]);

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(width));
    } catch {
      // Per-viewer convenience only; fine to lose.
    }
  }, [storageKey, width]);

  return { width, setWidth: (w: number) => setWidth(clamp(w)) };
}

/** Vertical drag bar placed on the left edge of a right-hand panel. Dragging
 * left widens the panel; double-click resets it to `minWidth`. */
export function ResizeHandle({
  width,
  minWidth,
  onResize,
}: {
  width: number;
  minWidth: number;
  onResize: (w: number) => void;
}) {
  const [dragging, setDragging] = useState(false);

  function onPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = width;
    setDragging(true);
    document.body.classList.add("is-resizing");
    const move = (ev: PointerEvent) => onResize(startWidth + (startX - ev.clientX));
    const up = () => {
      setDragging(false);
      document.body.classList.remove("is-resizing");
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  return (
    <div
      className={`resize-handle${dragging ? " resize-handle--active" : ""}`}
      role="separator"
      aria-orientation="vertical"
      title="Drag to resize · double-click to reset"
      onPointerDown={onPointerDown}
      onDoubleClick={() => onResize(minWidth)}
    />
  );
}
