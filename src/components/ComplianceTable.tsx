import type { LimiterProfile } from "../lib/api";
import { gradeAllSites, verdictColor, type Verdict } from "../lib/limits";

function VerdictDot({ verdict }: { verdict: Verdict }) {
  return <span className="verdict-dot" style={{ background: verdictColor(verdict) }} />;
}

/** Full table (2b inspector): dot + name + dual bar + verdict word. */
export function ComplianceTable({
  sites,
  titleLength,
  keywordCount,
}: {
  sites: LimiterProfile[];
  titleLength: number;
  keywordCount: number;
}) {
  const rows = gradeAllSites(sites, titleLength, keywordCount);
  if (rows.length === 0) {
    return <p className="empty-note">No site profiles enabled. Enable one under Sites.</p>;
  }
  return (
    <div className="compliance-table">
      {rows.map((row) => {
        const worstRatio = Math.max(row.title.ratio, row.keywords.ratio);
        return (
          <div key={row.site.name} className="compliance-row">
            <VerdictDot verdict={row.overall} />
            <span className="compliance-site">{row.site.name}</span>
            <div className="compliance-mid">
              <div className="compliance-labels">
                <span>
                  title {titleLength}/{row.site.max_title_chars}
                </span>
                <span>
                  kw {keywordCount}/{row.site.max_keywords}
                </span>
              </div>
              <div className="compliance-bar-track">
                <div
                  className="compliance-bar-fill"
                  style={{
                    width: `${Math.min(100, worstRatio * 100)}%`,
                    background: verdictColor(row.overall),
                  }}
                />
              </div>
            </div>
            <span className="compliance-verdict" style={{ color: verdictColor(row.overall) }}>
              {row.overall}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/** Condensed variant (2c right panel): dot + site + single mono string, no bars. */
export function ComplianceTableCondensed({
  sites,
  titleLength,
  keywordCount,
}: {
  sites: LimiterProfile[];
  titleLength: number;
  keywordCount: number;
}) {
  const rows = gradeAllSites(sites, titleLength, keywordCount);
  if (rows.length === 0) {
    return <p className="empty-note">No site profiles enabled.</p>;
  }
  return (
    <div className="compliance-table compliance-table--condensed">
      {rows.map((row) => (
        <div key={row.site.name} className="compliance-row compliance-row--condensed">
          <VerdictDot verdict={row.overall} />
          <span className="compliance-site">{row.site.name}</span>
          <span className="compliance-condensed-mono">
            {titleLength}/{row.site.max_title_chars} · {keywordCount}/{row.site.max_keywords}
          </span>
        </div>
      ))}
    </div>
  );
}
