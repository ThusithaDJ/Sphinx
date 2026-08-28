# Handoff: Sphinx — Pipeline UI (turn 2, options 2a–2g)

## Overview

Sphinx is a Tauri + React + TypeScript desktop app that generates stock-media metadata
(title, description, keywords) with a vision model, validates it against each stock site's
limits, embeds it into the file with exiftool, and uploads it over SFTP.

The current UI (`src/App.tsx`, ~1,590 lines, one component, `src/App.css`) is a single
scrolling page: a wide asset table, collapsible config panels, and inline expanded analysis
rows. This handoff replaces it with **seven purpose-built screens under one top-level nav**,
all sharing a single visual language ("pipeline direction"):

- Every asset carries a **5-dot pipeline stage track** (Analyze → Generate → Enrich → Embed → Upload).
- Every keyword carries a **blended heat score** (`0.5 × model confidence + 0.5 × site demand`,
  0–100) rendered as chip colour + a small monospace number.
- Every editable field shows **live character counters against the strictest enabled site limit**.
- Every screen has the same three-part frame: **top nav bar (46px) → work area → status strip (26px)**,
  and most work areas are **left content + right inspector**.

The work to be done: rebuild the app's UI as these seven screens, wired to the **existing**
`src/lib/api.ts` Tauri commands. No backend/Rust changes are implied by this design; where a
screen shows something the API cannot yet provide, it is called out under "Data" for that screen.

## About the design files

The files in this bundle are **design references written in HTML** — prototypes that show the
intended look, structure, density, and behaviour. They are **not production code to copy**.
They use a small in-house streaming-template runtime (`support.js`, `<x-dc>`, `<sc-for>`) that has
nothing to do with the Sphinx codebase; ignore it entirely.

The task is to **recreate these designs inside the existing Sphinx environment** — React 18 +
TypeScript + Vite + Tauri 2, styled the way the repo already styles things (plain CSS in
`src/App.css`; no CSS framework is installed, so do not introduce one without asking). Read the
HTML for exact colours, sizes, spacing, and copy; write idiomatic React components.

Recommended target structure (adjust to the repo's conventions):

```
src/
  App.tsx                 // routing shell: nav bar + status strip + <Outlet/>
  routes/
    ImportScreen.tsx      // 2a
    LibraryScreen.tsx     // 2b
    AssetEditorScreen.tsx // 2c
    ReviewScreen.tsx      // 2d
    QueueScreen.tsx       // 2e
    SitesScreen.tsx       // 2f
    SettingsScreen.tsx    // 2g
  components/
    NavBar.tsx StatusStrip.tsx StageDots.tsx KeywordChip.tsx
    HeatField.tsx ComplianceTable.tsx AssetCard.tsx Inspector.tsx
    CharCounter.tsx StatCard.tsx Toggle.tsx FieldRow.tsx
  lib/api.ts              // unchanged
  lib/heat.ts             // new: heat score + heat colour ramp (spec below)
```

## Fidelity

**High fidelity.** Colours, type sizes, paddings, radii, and copy in the HTML are final and
should be reproduced closely. Two deliberate exceptions:

1. **Thumbnails are CSS gradient placeholders** (`linear-gradient(135deg, c1, c2)`). In the app
   they become real thumbnails from the asset path (Tauri `convertFileSrc`), same box sizes and radii.
2. **All data is fictional sample content.** Names, counts, log lines, and hostnames are
   plausible fixtures, not fixtures to ship.

Everything is designed for a **1360 × 850 CSS-px window** (the mock canvas), which is a
realistic small desktop window. Layouts must survive 1280 × 800 minimum and grow gracefully:
fixed-width side panels, flexible centre column.

## Screens

Full per-screen and per-component specification, including exact hex values, sizes, and copy:
see **[SCREENS.md](./SCREENS.md)**. Summary:

| Id | Screen | Purpose |
|----|--------|---------|
| 2a | Import | Drop files/folders, manage watch folders, pre-flight each incoming file |
| 2b | Library | Browse all assets as a stage-first grid; inspect and approve one in the right panel |
| 2c | Asset editor | Full metadata record for one asset: title, description, drag-ordered keyword field, per-site preview |
| 2d | Review | Keyboard triage of the needs-review queue, one asset at a time, with pre-approval checks |
| 2e | Queue | Live job list with progress, plus a failure-detail panel with log and fix |
| 2f | Sites | Per-site limit profiles (title/desc/keyword caps) and SFTP transport config |
| 2g | Settings | AI provider, prompt guidance, local tool paths, storage |

## Interactions & behaviour

Global:

- **Nav** — six destinations: Import, Library, Review, Queue, Sites, Settings. Active item:
  `background #eef2fb`, `color #2f6fed`, `font-weight 600`. Inactive: `color #5c636e`, no
  background. Counts render as a `#8b919c` span after the label (Review = needs-review count,
  Queue = running+pending). Use client-side routing; the window never scrolls as a whole —
  each screen's inner regions scroll (`overflow-y:auto` on the content and inspector columns).
- **Status strip** (26px, `#fff`, `border-top:1px solid #e4e6ea`, `#8b919c`, 11px) is
  screen-specific and reflects live state (watch folder activity, queue counts, worker pool,
  keyboard hints). Never decorative — if there is nothing true to say, hide it.
- **Hover** — cards and rows lift to `background #fbfcfd`; buttons darken one step
  (primary `#2f6fed → #2560d8`; secondary border `#dfe2e7 → #c9ced6`). Transition
  `background-color .12s ease, border-color .12s ease`. No transforms, no shadows on hover.
- **Focus** — `outline: none` plus `border-color:#2f6fed; box-shadow:0 0 0 3px rgba(47,111,237,.10)`
  on inputs/textareas (this is the focused-title treatment shown in 2c). Keep a visible focus
  ring on every interactive element for keyboard users.
- **Disabled** — `opacity .5; cursor:not-allowed`. Buttons whose action is impossible
  (Upload with no SFTP profile, Embed with exiftool missing) are disabled with a `title=` reason,
  never hidden.

Screen-specific behaviour (drag-and-drop ingest, keyboard triage bindings, keyword drag-to-reorder,
queue polling, connection test, tool locate) is documented per screen in SCREENS.md.

Motion budget is deliberately small: progress bars animate width (`transition: width .3s linear`),
panels appear without animation, and nothing bounces. This is a work tool used dozens of times a day.

## State management

The existing single-component `useState` pile should be split by screen. Suggested shape —
keep it plain React (no new state library) unless the repo already leans another way:

```ts
// app-level (context or a tiny store), because the nav badges and status strip need them
projectId: number
assets: Asset[]              // listAssets(limit, offset) + onAssetsIngested subscription
assetCount: number
jobCounts: JobCounts         // queueJobCounts(), refreshed on onJobUpdated
tools: { exiftool: string | null; ffmpeg: string | null }  // checkExiftool / checkFfmpeg
limiterProfiles: LimiterProfile[]
activeLimiter: LimiterProfile | null   // getLimiterProfile(projectId)
sftpProfiles: SftpProfile[]

// Import (2a)
incoming: { path, name, size, dims, verdict: 'ready'|'duplicate'|'unsupported'|'needs-ffmpeg' }[]
watchDirs: { path, active, lastScan, newCount }[]   // startWatch/stopWatch
queueAfterIngest: boolean

// Library (2b)
filter: 'all'|'review'|'ready'|'failed'; query: string; selectedIds: Set<number>
inspectedId: number | null   // drives the right panel (getMetadata + getAnalysis)
targetSite: string           // which site the counters/compliance grade against

// Asset editor (2c)
draft: { title: string; description: string; keywords: Keyword[] }  // Keyword = {word, confidence, demand}
dirty: boolean; rejected: string[]; previewSite: string
dragIndex: number | null

// Review (2d)
queueIds: number[]; cursor: number; decisions: Record<id, 'approved'|'rejected'|'skipped'>
flags: string[]              // per-asset rework flags

// Queue (2e)
jobs: Job[]                  // listQueueJobs(100) + onJobUpdated patching in place
jobFilter: 'all'|'running'|'failed'; selectedJobId: number | null; concurrency: number

// Sites (2f) / Settings (2g)
editingProfile: LimiterProfile & SftpProfile draft; testResult: 'idle'|'testing'|'ok'|'error'
analysisConfig, keywordConfig, embedConfig, videoConfig, transcriptionConfig  // existing getters/setters
```

Data flow rules:

- **Reads on mount, events for updates.** `onAssetsIngested` and `onJobUpdated` already exist —
  subscribe once at app level, patch state in place. Do not poll `listQueueJobs` on a timer if the
  event stream covers it; fall back to a 2s poll only while the Queue screen is visible.
- **Metadata edits are local until saved.** The editor holds a `draft`; `Save & embed` writes
  metadata then calls `embedAssetMetadata`. `Revert to AI` restores the last `getMetadata()` payload.
- **Validation is derived, never stored.** Character counts and per-site verdicts are computed
  from `draft` + `limiterProfiles` on every render — see `lib/heat.ts` + a `lib/limits.ts` helper.

## Design tokens

Add these to `src/App.css` as custom properties on `:root` and use them everywhere.

Colour:

| Token | Hex | Use |
|---|---|---|
| `--bg` | `#f6f7f9` | app background |
| `--surface` | `#ffffff` | panels, cards, nav, status strip |
| `--surface-2` | `#fbfcfd` | nested/quiet cards, hover |
| `--ink` | `#171a1f` | primary text |
| `--ink-2` | `#3d434c` | body copy in descriptions |
| `--muted` | `#5c636e` | secondary text, inactive nav |
| `--faint` | `#8b919c` | labels, meta, counters |
| `--faint-2` | `#b0b6be` | placeholder / hint text |
| `--line` | `#e4e6ea` | panel borders, dividers |
| `--line-2` | `#eef0f3` | inside-panel dividers |
| `--line-3` | `#f4f5f7` | list row dividers |
| `--field` | `#dfe2e7` | input/button borders, empty progress |
| `--field-2` | `#c9ced6` | secondary hover border, drag handle idle |
| `--dash` | `#c3cbdd` | drop-zone dashed border |
| `--accent` | `#2f6fed` | primary action, active nav, selection |
| `--accent-tint` | `#eef2fb` | active nav background |
| `--accent-ring` | `rgba(47,111,237,.10)` | focus ring |
| `--ok` | `#1a7f37` | pass / connected dot |
| `--ok-ink` | `#177d34` | pass text |
| `--ok-tint` | `rgba(26,127,55,.13)` | pass chip |
| `--warn` | `#b7791f` | warning dot / near-limit counter |
| `--warn-ink` | `#8a5b12` | warning text |
| `--danger` | `#b3261e` | fail dot, destructive text |
| `--danger-ink` | `#8f2019` | fail message text |
| `--danger-tint` | `rgba(179,38,30,.07)` | fail callout background |
| `--danger-line` | `rgba(179,38,30,.22)` | fail callout border |
| `--overlay` | `rgba(10,14,20,.55)` | badge over thumbnail |
| `--off` | `#8b919c` | disabled/off state dot |

Keyword heat ramp (light surfaces) — `[background, border, text]`, chosen by score:

| Score | Background | Border | Text |
|---|---|---|---|
| > 78 (hot) | `rgba(217,98,43,.14)` | `rgba(217,98,43,.42)` | `#8f3d16` |
| 56–78 (warm) | `rgba(232,178,122,.20)` | `rgba(211,155,96,.42)` | `#7a4f21` |
| 33–55 (cool) | `rgba(207,214,221,.50)` | `rgba(180,190,201,.60)` | `#3d434c` |
| ≤ 32 (cold) | `#f4f2ee` | `#e0dcd3` | `#5f5a51` |

Typography:

- Family: `'Segoe UI', system-ui, sans-serif` (matches Windows-first Tauri target).
- Mono (counters, hashes, paths, logs): `ui-monospace, Consolas, monospace`.
- Scale: `23/700` stat number · `16/700` drop-zone headline · `15/700` section title and
  editor title field · `15/600` triage title · `14/400` base · `13/600–700` panel titles,
  buttons, nav · `13/400` field values · `12.5/400` body meta and check rows · `12/400` small
  meta · `11.5/400` chip and counter text · `11/400` status strip and log ·
  `10.5/700` uppercase section labels (`letter-spacing:.06em`) · `10/600` thumbnail badges.
- Line-height `1.45` base, `1.4` on titles, `1.6` on the monospace log block.

Spacing (px, effectively a 4/6/8 rhythm): `2 4 5 6 7 8 9 10 11 12 14 16 18 20 24`.
Panel padding `16px` (`14px 16px` for headers/footers), screen padding `16–20px`,
grid gap `14px`, chip gap `5–6px`, form grid gap `12px`.

Radii: `999px` pills · `12px` screen-level panels and drop zone · `11px` cards with header+footer ·
`10px` asset cards, stat cards, preview · `8px` inner tables, chips in the editor, thumbnails ·
`7px` inputs, buttons, nav items · `5–6px` badges · `50%` dots.

Elevation: one shadow only — `0 1px 2px rgba(20,24,32,.05)` on asset cards. Focus ring
`0 0 0 3px rgba(47,111,237,.10)`. No other shadows.

Fixed dimensions: nav bar 46px · status strip 26px · toolbar row 40px (`10px 16px`) ·
inspector 392px (2b) / 344px (2c right) / 352px (2c left) / 376px (2e) / 400px (2f) /
432px (2a) / 330px (2d) · settings sidebar 208px · triage rail 132px · stage dot 7px ·
status dot 8px · progress bar 5px (3px inside compliance rows) · toggle 34 × 19px.

## Derived values (implement in `src/lib/heat.ts`)

```ts
// Blended keyword heat, 0–100. Order of keywords is stored for the user's benefit
// but is NOT sent to any site — the set is what matters.
export const heat = (confidence: number, demand: number) =>
  Math.round(50 * confidence + 0.5 * demand);   // confidence 0–1, demand 0–100

export const heatBand = (score: number) =>
  score > 78 ? 'hot' : score > 55 ? 'warm' : score > 32 ? 'cool' : 'cold';

// A keyword below the project's demand floor (default 20) is shown struck through
// and excluded from the exported set unless the user re-enables it.
```

Compliance verdict for a field, per site: `pass` when length ≤ limit and keyword count is inside
`[min, max]`; `near` (amber) at ≥ 90% of a length limit; `fail` (red) when over the limit or under
the keyword minimum. When several site profiles are enabled, **the strictest limit wins** and the
counter in the editor names the site it is grading against (e.g. `62 / 70 Adobe limit`).

## Assets

None to import. The design uses no images, no icon font, and no SVG illustration:

- Thumbnails: gradient placeholders in the mock → real file thumbnails in the app.
- "Icons" are single Unicode glyphs in text runs — `↓` (drop), `⌕` (search), `⠿` (drag handle),
  `×` (remove chip), `✓` (approved), `←`/`→` (back/next), `▾` (select caret), `·` (separators).
  If the repo prefers a real icon set, swap these for equivalents at the same optical size;
  do not add colourful or decorative icons.
- `public/tauri.svg` and `public/vite.svg` remain untouched Vite scaffolding.

## Files in this bundle

| File | What it is |
|---|---|
| `screens/*.png` | 2× reference capture of each screen (`2a-import`, `2b-library`, `2c-asset-editor`, `2d-review`, `2e-queue`, `2f-sites`, `2g-settings`) — the visual target for each section of SCREENS.md |
| `SCREENS.md` | Per-screen, per-component specification — the main implementation document |
| `Sphinx UI.dc.html` | The design prototype. Turn 2 (`#2a`–`#2g`, top of the page) is the direction to build. Turn 1 below it is history: `1a` recreation of today's App.tsx, `1b` pipeline direction (the ancestor of turn 2), `1c` dark "studio" alternative, `1d` interactive heat-bench experiment — all rejected or superseded |
| `support.js` | Runtime the prototype needs in order to open in a browser. **Not** part of the design; ignore it when implementing |
| `screenshots/2a-import.png` … `2g-settings.png` | 2× captures of each screen (2724 × 1704), for quick visual reference without opening the prototype |

To view the prototype: open `Sphinx UI.dc.html` in a browser (both files must sit in the same folder).
Scroll to the top section; each screen is labelled with its id.

## Out of scope / open questions

- **No publish/upload log screen** exists yet. Uploads are visible in the Queue as `upload` jobs;
  a dedicated per-site delivery log was discussed and not designed. Ask before inventing one.
- **Video assets** are designed as blocked-until-ffmpeg (2a pre-flight, 2e failure detail,
  2g tool row). Transcription config exists in the API but has no screen — surface it under
  Settings → Video pipeline when it is needed.
- **Projects** — the API supports multiple projects (`listProjects`, `createProject`); the design
  shows a single "Default project" label in the nav. A project switcher is not designed.
- The design system attached to this project is currently empty, so the palette above is the
  authority. If a real design system lands later, re-map tokens rather than re-designing screens.
