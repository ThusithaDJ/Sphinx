// Blended keyword heat score and the colour ramp used to render it.
// See new_ui_design/README.md "Derived values" for the spec this implements.

export type HeatBand = "hot" | "warm" | "cool" | "cold";

/** confidence: 0-1 (model's own confidence in the term), demand: 0-100 (site search-volume index). */
export function heat(confidence: number, demand: number): number {
  return Math.round(50 * confidence + 0.5 * demand);
}

export function heatBand(score: number): HeatBand {
  if (score > 78) return "hot";
  if (score > 55) return "warm";
  if (score > 32) return "cool";
  return "cold";
}

export interface HeatColors {
  bg: string;
  border: string;
  text: string;
}

const HEAT_RAMP: Record<HeatBand, HeatColors> = {
  hot: { bg: "rgba(217,98,43,.14)", border: "rgba(217,98,43,.42)", text: "#8f3d16" },
  warm: { bg: "rgba(232,178,122,.20)", border: "rgba(211,155,96,.42)", text: "#7a4f21" },
  cool: { bg: "rgba(207,214,221,.50)", border: "rgba(180,190,201,.60)", text: "#3d434c" },
  cold: { bg: "#f4f2ee", border: "#e0dcd3", text: "#5f5a51" },
};

export function heatColors(score: number): HeatColors {
  return HEAT_RAMP[heatBand(score)];
}

/** A keyword below this demand floor is struck through and excluded unless re-enabled. */
export const DEFAULT_DEMAND_FLOOR = 20;

export interface Keyword {
  word: string;
  /** Model confidence, 0-1. */
  confidence: number;
  /** Site demand index, 0-100. Undefined when the term is unscored (shows "--"). */
  demand?: number;
  /** Typed in by the user rather than produced by AI/enrichment: rendered
   * green and never scored. */
  userAdded?: boolean;
}

/** Heat-band colours, plus green for user-added terms -- shared by the chips
 * and the keyword-field legend. */
export const USER_ADDED_COLORS: HeatColors = {
  bg: "rgba(26,127,55,.12)",
  border: "rgba(26,127,55,.45)",
  text: "#1a6b33",
};

export const HEAT_LEGEND: { label: string; range: string; colors: HeatColors }[] = [
  { label: "Hot", range: "79+", colors: { bg: "rgba(217,98,43,.14)", border: "rgba(217,98,43,.42)", text: "#8f3d16" } },
  { label: "Warm", range: "56–78", colors: { bg: "rgba(232,178,122,.20)", border: "rgba(211,155,96,.42)", text: "#7a4f21" } },
  { label: "Cool", range: "33–55", colors: { bg: "rgba(207,214,221,.50)", border: "rgba(180,190,201,.60)", text: "#3d434c" } },
  { label: "Cold", range: "0–32", colors: { bg: "#f4f2ee", border: "#e0dcd3", text: "#5f5a51" } },
];

export function keywordHeat(kw: Keyword): number | null {
  if (kw.userAdded || kw.demand === undefined) return null;
  return heat(kw.confidence, kw.demand);
}

/** Highest heat first; unscored keywords sink to the end. Stable for ties. */
export function sortKeywordsByHeat(kws: Keyword[]): Keyword[] {
  return [...kws].sort((a, b) => (keywordHeat(b) ?? -1) - (keywordHeat(a) ?? -1));
}

/** Inserts each new keyword before the first existing one it outscores,
 * leaving the existing (possibly hand-arranged) order untouched. */
export function insertKeywordsByHeat(existing: Keyword[], added: Keyword[]): Keyword[] {
  const out = [...existing];
  for (const kw of sortKeywordsByHeat(added)) {
    const h = keywordHeat(kw) ?? -1;
    const at = out.findIndex((k) => (keywordHeat(k) ?? -1) < h);
    out.splice(at === -1 ? out.length : at, 0, kw);
  }
  return out;
}

// The vision/metadata API returns keywords as a plain ordered string[] --
// no per-keyword confidence or site-demand numbers exist yet. Confidence is
// derived from list order (a real signal: the model/generator emits its most
// relevant terms first). Demand has no real signal to draw on today, so it's
// a stable hash of the word -- a placeholder until a per-keyword demand API
// exists, kept deterministic so the same word always renders the same chip.
function hashDemand(word: string): number {
  let h = 0;
  for (let i = 0; i < word.length; i++) {
    h = (h * 31 + word.charCodeAt(i)) >>> 0;
  }
  return h % 101;
}

export function estimateKeywordScores(words: string[]): Keyword[] {
  const n = words.length;
  return words.map((word, i) => ({
    word,
    confidence: n <= 1 ? 1 : Math.max(0.25, 1 - (i / (n - 1)) * 0.6),
    demand: hashDemand(word.toLowerCase()),
  }));
}
