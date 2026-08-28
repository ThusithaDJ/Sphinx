// Per-site compliance grading, derived from a draft title/description/keyword
// set and the project's saved LimiterProfile(s). Never stored -- recomputed
// on every render. See new_ui_design/README.md "Compliance verdict".
import type { LimiterProfile } from "./api";

export type Verdict = "pass" | "near" | "fail";

export interface FieldVerdict {
  verdict: Verdict;
  ratio: number; // 0-1+, the worse of the length/count ratios
}

export interface SiteCompliance {
  site: LimiterProfile;
  title: FieldVerdict;
  keywords: FieldVerdict;
  overall: Verdict;
}

function lengthVerdict(length: number, max: number): FieldVerdict {
  const ratio = max > 0 ? length / max : 0;
  if (length > max) return { verdict: "fail", ratio };
  if (ratio >= 0.9) return { verdict: "near", ratio };
  return { verdict: "pass", ratio };
}

function countVerdict(count: number, min: number, max: number): FieldVerdict {
  const ratio = max > 0 ? count / max : 0;
  if (count < min || count > max) return { verdict: "fail", ratio: count < min ? 1 : ratio };
  if (ratio >= 0.9) return { verdict: "near", ratio };
  return { verdict: "pass", ratio };
}

function worse(a: Verdict, b: Verdict): Verdict {
  const rank: Record<Verdict, number> = { pass: 0, near: 1, fail: 2 };
  return rank[a] >= rank[b] ? a : b;
}

export function gradeSite(
  site: LimiterProfile,
  titleLength: number,
  keywordCount: number
): SiteCompliance {
  const title = lengthVerdict(titleLength, site.max_title_chars);
  const keywords = countVerdict(keywordCount, site.min_keywords, site.max_keywords);
  return { site, title, keywords, overall: worse(title.verdict, keywords.verdict) };
}

export function gradeAllSites(
  sites: LimiterProfile[],
  titleLength: number,
  keywordCount: number
): SiteCompliance[] {
  return sites.map((s) => gradeSite(s, titleLength, keywordCount));
}

/** The strictest (lowest) title-length limit across a set of sites, and which site owns it. */
export function strictestTitleLimit(
  sites: LimiterProfile[]
): { limit: number; site: string } | null {
  if (sites.length === 0) return null;
  let best = sites[0];
  for (const s of sites) if (s.max_title_chars < best.max_title_chars) best = s;
  return { limit: best.max_title_chars, site: best.name };
}

export function strictestDescriptionLimit(
  sites: LimiterProfile[]
): { limit: number; site: string } | null {
  if (sites.length === 0) return null;
  let best = sites[0];
  for (const s of sites) if (s.max_description_chars < best.max_description_chars) best = s;
  return { limit: best.max_description_chars, site: best.name };
}

export function verdictColor(v: Verdict): string {
  return v === "pass" ? "var(--ok)" : v === "near" ? "var(--warn)" : "var(--danger)";
}
