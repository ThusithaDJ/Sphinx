# Sphinx Pipeline UI — screen & component specification

Companion to `README.md` (tokens, fidelity, state, global behaviour live there).
Every measurement below is CSS px at the design canvas of **1360 × 850**.

Reference the prototype (`Sphinx UI.dc.html`) side by side: the ids `2a`–`2g` below match the
badges in the file.

---

## Shared components

### NavBar — 46px, on every screen

- `height:46px; background:#fff; border-bottom:1px solid #e4e6ea; display:flex; align-items:center; gap:16px; padding:0 16px; flex:none`
- Left: wordmark `Sphinx` at `14/700`, `letter-spacing:.02em`; then the project name
  `Default project` at `12/400 #8b919c`, baseline-aligned (`align-items:baseline; gap:8px`).
- Divider: `1px × 20px`, `#e4e6ea`.
- Destinations, `display:flex; gap:2px`, each `padding:5px 10px; border-radius:6px; font-size:13px`:
  Import · Library · Review · Queue · Sites · Settings.
  Active = `background:#eef2fb; color:#2f6fed; font-weight:600`. Inactive = `color:#5c636e`.
  Badge counts are a nested `<span style="color:#8b919c">` after the label (Library 42,
  Review 12, Queue 2 in the mock).
- `flex:1` spacer, then screen-specific right-hand content:
  - **2a**: two model/tool status pills — `<dot> OpenAI GPT-4o` (green `#1a7f37`),
    `<dot> ffmpeg missing` (amber `#b7791f`), each `12/400 #5c636e`, dot 7px, `gap:5px`.
  - **2b**: primary button `Import…`.
  - **2c**: `Unsaved changes` (`12 #8b919c`) + secondary `Revert to AI` + primary `Save & embed`.
  - **2d**: `2 of 12 reviewed` (`12 #5c636e`) + 150 × 5px progress bar (`#eef0f3` track, `#2f6fed` fill).
  - **2e**: secondary `Pause queue` + primary `Retry failed (2)`.
  - **2f**: primary `Add site profile`.
  - **2g**: `Saved automatically to sphinx.db` (`12 #8b919c`).
- **2c replaces the destination list** with a back affordance: `← Library` (`13/600 #2f6fed`),
  divider, filename (`13/700`), a `needs review` chip
  (`padding:2px 8px; radius:5px; background:rgba(47,111,237,.14); color:#2f6fed; 11/600`),
  and `asset 12 of 42` (`12 #8b919c`).

### Buttons

- **Primary**: `background:#2f6fed; color:#fff; border:none; border-radius:7px; font-size:13px;
  font-weight:600; padding:6px 14px` (in-panel footers use `padding:8px`–`9px` full width).
- **Secondary**: `background:#fff; color:#171a1f; border:1px solid #dfe2e7; border-radius:7px;
  font-size:13px; padding:6px 12px`.
- **Quiet/toolbar**: same as secondary at `font-size:12px; padding:5px 10px; border-radius:6px`.
- **Destructive-tinted** (Reject): secondary chrome with `color:#b3261e`.
- **Link-ish**: bare text, `11.5–12/600 #2f6fed` (e.g. `Scan all now`, `Open editor →`, `Enrich +`,
  `Refresh now`).

### StageDots

Five 7px circles, `gap:4px`, one per pipeline stage in order
`Analyze · Generate · Enrich · Embed · Upload`. Completed = `#2f6fed`; pending = `#dfe2e7`;
when the asset is blocked, completed dots become `#c08a2a` instead. Each dot carries the stage
name as a tooltip. A `11/400 #8b919c` label to the right of the track names the current stage
(`new` at 0, the stage name at 1–4, `uploaded` at 5).

### KeywordChip

Two variants of the same object:

- **Compact (inspector / triage)**: `display:inline-flex; align-items:center; gap:5px;
  padding:3px 8px; border-radius:999px; border:1px solid <heat border>;
  background:<heat bg>; font-size:11.5px; color:#171a1f`, followed by the score in
  `9.5px ui-monospace #8b919c`.
- **Editable (asset editor)**: `padding:5px 9px 5px 6px; border-radius:8px; cursor:grab`,
  content = drag handle `⠿` (`10px`, `#2f6fed` for the top 8 by heat, `#c9ced6` below) +
  word (`12.5px`) + score (`10px` mono `#8b919c`) + remove `×` (`11px #c9ced6`).
  The field ends with an `+ add keyword` affordance:
  `border:1px dashed #c9ced6; color:#8b919c; padding:5px 11px; border-radius:8px`.
- Rejected keywords render `background:#f4f5f7; border:1px dashed #d5d9df; color:#8b919c;
  text-decoration:line-through; 11px`.
- Heat colours: see the ramp table in README. Score = blended demand; chip colour is the only
  encoding of quality, so never re-use these oranges for anything else.

### ComplianceTable (2b inspector, 2c right panel)

`border:1px solid #e4e6ea; border-radius:8px; overflow:hidden`; each row
`display:flex; align-items:center; gap:10px; padding:9px 11px; border-bottom:1px solid #eef0f3`:

1. 8px verdict dot (`#1a7f37` pass, `#b7791f` near, `#b3261e` fail).
2. Site name, `12/600`, fixed `width:96px` in 2b so bars align.
3. Flexible column: a row of two `10.5px` mono labels (`title 62/200`, `kw 34/50`) in `#8b919c`
   with `justify-content:space-between`, then a 3px bar (`#eef0f3` track, fill in the verdict colour,
   width = the worse of the two ratios).
4. Verdict word (`10.5/600`, verdict colour, `width:52px; text-align:right`).

In 2c the same table is condensed: dot + site + a single mono string `title · kw`, no bars.

### Section label

`font-size:10.5px; font-weight:700; letter-spacing:.06em; text-transform:uppercase; color:#8b919c`.
Used for every panel heading and field label. When a count belongs to the label it is a nested
`<span style="color:#171a1f">` (e.g. `KEYWORDS 34`).

### CharCounter

`11px ui-monospace`, right-aligned on the same baseline as the field's label.
`#5c636e` normal, `#b7791f` at ≥ 90% of the limit, `#b3261e` over. When the strictest limit comes
from a specific site, the counter names it: `62 / 70 Adobe limit`.

### StatusStrip — 26px, on every screen

`height:26px; background:#fff; border-top:1px solid #e4e6ea; display:flex; align-items:center;
gap:14px; padding:0 16px; color:#8b919c; font-size:11px`. Separators are literal `·` spans.
Per-screen copy is listed with each screen below.

---

## 2a — Import

**Purpose.** Get files into the library: drop them, or let a watch folder do it. Show what will
and will not survive ingest **before** anything is queued or billed.

**Layout.** NavBar → body `display:flex` → left column `flex:1; padding:20px;
display:flex; flex-direction:column; gap:16px` and right panel `width:432px; background:#fff;
border-left:1px solid #e4e6ea` → StatusStrip.

**Drop zone** (`flex:none`): `border:2px dashed #c3cbdd; border-radius:12px; background:#fff;
padding:40px 24px`, centred column, `gap:10px`.

- 44 × 44 tile: `border-radius:12px; background:#eef2fb; color:#2f6fed`, glyph `↓` at 20px.
- Headline `Drop images, videos or folders here` — `16/700`.
- Sub `JPEG, PNG, TIFF (flattened), MP4, MOV · files are hashed on ingest so duplicates never
  re-bill the model` — `12.5/400 #8b919c`.
- Buttons row (`gap:8px; margin-top:6px`): primary `Choose files…`, secondary `Add watch folder…`.
- **Behaviour.** Whole-window drag target via Tauri's webview drag-drop event (the repo already
  imports `getCurrentWebview`). On `dragover` the zone goes `border-color:#2f6fed;
  background:#f8faff`. `Choose files…` → `open({multiple:true})`; `Add watch folder…` →
  `open({directory:true})` then `startWatch(dir)`.

**Watch folders panel** (`flex:1`, `background:#fff; border:1px solid #e4e6ea; border-radius:12px`):

- Header `padding:13px 16px; border-bottom:1px solid #eef0f3`: section label `WATCH FOLDERS`,
  right-aligned link `Scan all now`.
- Scrolling rows, `padding:12px 16px; gap:11px; border-bottom:1px solid #f4f5f7`:
  8px dot (green active / `#8b919c` paused) · path in `12.5/600 ui-monospace` ·
  meta line `11/400 #8b919c` (`watching · 2 new files · scanned 11 s ago`) ·
  an On/Off pill (`padding:3px 9px; radius:999px; border:1px solid #dfe2e7; 11px #5c636e`) ·
  `Remove` (`11.5 #8b919c`).
- Mock content: `D:\Photos\2026-08` (On, 2 new), `D:\Photos\2026-07` (On, idle),
  `E:\Client-work\ceramics` (Off, `paused · 41 files not ingested`).
- **Behaviour.** Toggling Off calls `stopWatch()`; the API currently supports a single watch dir —
  either extend it or make this list single-select and say so in the UI.

**Incoming panel** (right, 432px):

- Header `padding:14px 16px 12px; border-bottom:1px solid #eef0f3`:
  `Incoming` `13/700` + `6 files` in `#8b919c` weight 400; right side `pre-flight` `11.5 #8b919c`.
- Rows `padding:11px 16px; gap:10px; border-bottom:1px solid #f4f5f7`:
  40 × 30 thumb (`radius:5px`, `#eef0f3` placeholder) · name `12.5/600` truncated ·
  meta `11 #8b919c` (`JPEG · 8.9 MB · 6000×4000`) · verdict `11/600` with a 7px dot in the
  same colour.
- Verdicts (mock): 3 × `ready` (green), `duplicate hash` (amber), `needs ffmpeg` (amber),
  `unsupported` (red `#b3261e`, for `scan-contact-sheet.tif`).
- Footer `padding:12px 16px; border-top:1px solid #eef0f3; gap:9px`:
  checkbox (checked, `accent-color:#2f6fed`) `Queue analysis immediately after ingest` (`12 #5c636e`),
  then primary `Ingest 4 eligible` (`flex:1`) + secondary `Clear`.
- **Behaviour.** Verdicts come from `ingestFiles`' `IngestOutcome` union — pre-flight locally where
  possible (extension, size) and treat the command result as authoritative. The primary button's
  label counts only `ready` rows; disabled at zero. With the checkbox on, follow ingest with
  `enqueueBatch(ids, 'analyze')`.

**Status strip.** `2 files skipped: 1 duplicate, 1 unsupported · sphinx.db 18.2 MB`.

---

## 2b — Library

**Purpose.** See the whole library at a glance with stage and compliance state, and approve
straightforward assets without leaving the grid.

**Layout.** NavBar → filter toolbar (40px) → body `flex` (grid `flex:1; padding:16px;
overflow-y:auto` + inspector `width:392px`) → StatusStrip.

**Filter toolbar**: `padding:10px 16px; background:#fff; border-bottom:1px solid #e4e6ea; gap:10px`.

- Search box: `width:250px; padding:6px 10px; border:1px solid #dfe2e7; radius:7px`, glyph `⌕`,
  placeholder `Search title, keyword, path` (`13 #8b919c`).
- Filter pills (`gap:6px`): active `All 42` = `background:#171a1f; color:#fff; 12/600;
  padding:5px 11px; radius:999px`; inactive `Needs review 12`, `Ready 18`, `Failed 2` =
  `background:#fff; border:1px solid #dfe2e7; color:#5c636e`.
- Right: `Target` label + a select-styled box `Shutterstock ▾` — this chooses which site's limits
  the grid and inspector grade against.

**Grid header row** (above the cards, `margin-bottom:12px`):
left `3 selected · 42 assets in Default project` (`12 #5c636e`, the count bold `#171a1f`);
right four quiet buttons — `Analyze`, `Generate metadata`, `Embed`, `Upload` — acting on the
selection (`enqueueBatch(selectedIds, type)`). Disabled with a tooltip when the selection is empty
or a required tool is missing.

**AssetCard** — `grid-template-columns:repeat(3,1fr); gap:14px`:

- `background:#fff; border:1px solid #e4e6ea; border-radius:10px; overflow:hidden;
  box-shadow:0 1px 2px rgba(20,24,32,.05)`. Selected card: `border-color:#2f6fed`.
- Thumbnail band `height:112px`, gradient placeholder → real thumbnail.
  Top-left kind badge (`image`/`video`): `padding:2px 6px; radius:5px;
  background:rgba(10,14,20,.55); color:#fff; 10/600; letter-spacing:.03em; text-transform:uppercase`.
  Top-right flag badge, same geometry, colours by state — `review` blue-tint,
  `ready` green-tint, `blocked` amber-tint, `failed` red-tint.
- Body `padding:9px 10px 11px; gap:7px`: filename `12/600` truncated · title preview
  `11.5/400 #5c636e; line-height:1.35; height:31px; overflow:hidden` (exactly two lines) ·
  footer row = StageDots + stage label, spacer, `34 kw` (`11 #8b919c`, `—` when none).
- **Behaviour.** Click selects (multi with cmd/shift) and sets the inspected asset; double-click
  opens 2c. Nine cards visible at this canvas; the grid virtualises past a few hundred assets.

**Inspector** (392px, `background:#fff; border-left:1px solid #e4e6ea`):

- Header `padding:14px 16px 12px; border-bottom:1px solid #eef0f3`:
  96 × 64 thumb (`radius:8px`) + filename `13/700` + `JPEG · 8.4 MB · 6000×4000` (`11.5 #8b919c`) +
  two chips: `commercial-safe` (green tint, `10.5/600`) and `gpt-4o` (`background:#f0f2f5; color:#5c636e`).
- Scrolling body `padding:14px 16px 0`:
  - `TITLE` label + counter `62 / 200` → read-only-looking box
    `padding:8px 10px; border:1px solid #dfe2e7; radius:7px; 13px; line-height:1.4`.
    Copy: `Lone Scots pine on a frosted ridge at sunrise with valley mist`.
  - `KEYWORDS 34` label + `Open editor →` link → wrapped compact chips (18 shown, `gap:5px`).
  - `SITE COMPLIANCE` label → ComplianceTable with all four sites.
- Footer `padding:12px 16px; border-top:1px solid #eef0f3; gap:8px`:
  primary `Approve & embed` (`flex:1`) + secondary `Regenerate`.
- **Behaviour.** `Approve & embed` = write metadata + `embedAssetMetadata`, then advance the
  inspected asset to the next needs-review card. `Regenerate` = `generateMetadata` with the current
  analysis, replacing the draft. Both disabled while a compliance row reads `fail`.

**Status strip.** `Watching D:\Photos\2026-08 — 2 new files · Queue: 2 running · 5 pending`.

---

## 2c — Asset editor

**Purpose.** The one screen where a human rewrites what the model produced: title, description,
and the keyword set, with the consequences for every target site visible without switching context.

**Layout.** NavBar (back variant) → body `flex`: left meta rail `width:352px`, centre
`flex:1; padding:18px 20px; gap:16px`, right panel `width:344px` → StatusStrip.
All three columns scroll independently.

**Left rail** (`background:#fff; border-right:1px solid #e4e6ea; padding:16px; gap:14px`):

- 220px-tall preview, `border-radius:10px`, gradient → real image (`object-fit:cover`).
- Metadata grid, `12px`, label `#5c636e` / value `#171a1f`, `justify-content:space-between` rows:
  Format `JPEG · 8.4 MB` · Dimensions `6000 × 4000 · 24 MP` · Captured `12 Aug 2026, 06:41` ·
  Camera `A7R V · 70 mm · f/8` · Hash `b3:9f4c1a…d072` (mono).
- `PIPELINE` block above a `border-top:1px solid #eef0f3; padding-top:12px`:
  five rows, `12px`, each = 8px dot + stage name + a right-aligned `#8b919c` detail
  (`gpt-4o · 3.1 s`, `34 kw`, `+9 demand terms`); pending stages have `#dfe2e7` dots and
  `#8b919c` text (`Embed pending`, `Upload pending`).

**Centre column:**

1. **Title** — label + counter `62 / 70 Adobe limit` (amber). Field is the focused treatment:
   `padding:11px 13px; background:#fff; border:1px solid #2f6fed; border-radius:8px;
   font-size:15px; line-height:1.4; box-shadow:0 0 0 3px rgba(47,111,237,.1)`.
2. **Description** — label + counter `99 / 200`. Field `padding:11px 13px;
   border:1px solid #dfe2e7; radius:8px; 13.5/400 #3d434c; line-height:1.5; min-height:52px`
   (auto-growing textarea in the app).
3. **Keyword field** (`flex:1`, `background:#fff; border:1px solid #e4e6ea; border-radius:10px`):
   - Header `padding:12px 14px; border-bottom:1px solid #eef0f3; gap:10px`:
     `KEYWORD FIELD 34` · hint `drag to reorder priority · warm = high blended demand`
     (`11.5 #8b919c`) · spacer · `Enrich +` link · `Sort by heat` (`11.5 #5c636e`).
   - Body `padding:12px 14px; display:flex; flex-wrap:wrap; gap:6px; align-content:flex-start;
     overflow-y:auto` — 26 editable chips + the dashed add affordance.
   - Footer `padding:10px 14px; border-top:1px solid #eef0f3; 11.5 #8b919c`:
     `avg heat 58 · 3 terms below demand floor 20 · order is stored but not sent to sites`.
   - **Behaviour.** HTML5 drag or pointer-based reorder; the dragged chip follows the cursor at
     `opacity .8`, and the insertion point is a 2px `#2f6fed` bar between chips. `Enrich +` calls
     `enrichKeywords` and appends new chips with a brief `background-color` flash.
     `Sort by heat` reorders descending by score. `×` moves the word to the Rejected list.
     Typing in `+ add keyword` accepts comma/Enter as separators, dedupes case-insensitively,
     and scores the new term from the demand index (unscored terms show `—` and a `#f4f2ee` chip).

**Right panel** (`background:#fff; border-left:1px solid #e4e6ea`):

- `PREVIEW AS` header + four pills (`padding:4px 10px; radius:999px; 11.5/600;
  border:1px solid #dfe2e7`); active = `background:#171a1f; color:#fff`, others
  `background:#fff; color:#5c636e`. Switching re-grades all counters.
- **Submission preview** card: `border:1px solid #e4e6ea; radius:9px; padding:12px;
  background:#fbfcfd` — caption `Shutterstock submission` (`11 #8b919c`), the title at `13/600`,
  then `34 keywords · commercial · no release required` (`11.5 #5c636e`).
- `LIMITS` — condensed ComplianceTable (dot + site + mono `62 / 200 · 34 / 50`).
- `EMBED TARGETS` — three checkboxes (`12 #5c636e`, `accent-color:#2f6fed`):
  `IPTC Keywords + Headline` (on), `XMP dc:subject / dc:title` (on), `EXIF ImageDescription` (off).
  These map to the existing `EmbedConfig`.
- `REJECTED BY YOU` — struck-through dashed chips; clicking one restores it to the field.
- Footer: primary `Approve & next →` (`flex:1`) + secondary `Regenerate`.

**Status strip.** `⌘S save · ⌘⏎ approve and next · ⌥1–4 switch preview site`
(render as Ctrl on Windows — this is a Windows-first Tauri app; use the platform modifier).

---

## 2d — Review

**Purpose.** Burn down the needs-review queue at speed. One asset fills the screen; the decision
is a single keystroke; the checks tell you when not to trust the default.

**Layout.** NavBar (with the reviewed-progress bar) → body `flex`: thumbnail rail `width:132px`,
centre `flex:1; padding:18px 20px; gap:14px`, checks panel `width:330px` → StatusStrip.

**Rail** (`background:#fff; border-right:1px solid #e4e6ea; padding:12px 10px; gap:9px`):
one 74px-tall gradient thumb per queued asset, `border-radius:8px`, wrapped in a
`border:2px solid transparent` box that becomes `#2f6fed` for the current asset.
A 16px status disc sits top-right: green `✓` approved, blue `•` current, absent otherwise.

**Centre:**

- Preview `flex:1; border-radius:12px`, gradient → real image, with a bottom-left caption
  `padding:4px 9px; radius:6px; background:rgba(10,14,20,.6); color:#fff; 11.5px`:
  `studio-ceramics-114.jpg · 6000×4000 · gpt-4o`.
- Proposal card (`flex:none; background:#fff; border:1px solid #e4e6ea; border-radius:11px;
  padding:14px 16px; gap:11px`):
  `PROPOSED TITLE` + counter `58 / 70`, the title at `15/600; line-height:1.4`, then wrapped
  compact keyword chips with a trailing `+16 more` (`11.5 #8b919c`).

**Checks panel:**

- `PRE-APPROVAL CHECKS` → rows `12.5/400; gap:9px`: an 8px dot (`margin-top:5px`), the label,
  and a right-aligned `10.5/600` state word in the dot's colour. Mock rows:
  `Title within all 4 site limits` (pass) · `34 keywords · above minimum of 7` (pass) ·
  `3 keywords below demand floor` (warn) · `No trademarked terms detected` (pass) ·
  `Description reads as commercial-safe` (pass).
- `FLAG FOR REWORK` → five outline pills (`padding:4px 10px; radius:999px;
  border:1px solid #dfe2e7; 11.5 #5c636e`): `wrong subject`, `too generic`, `trademark risk`,
  `needs release`, `editorial only`. Multi-select; selected = `background:#171a1f; color:#fff`.
- Consequence note: `padding:11px 12px; radius:9px; background:#f6f7f9; 12/400 #5c636e` —
  `Approving embeds IPTC/XMP into the file and queues the upload for Shutterstock and Adobe Stock.`
  Site names bold `#171a1f`, generated from the enabled profiles.
- Footer: full-width primary `Approve & next  ⏎`, then a three-up row of secondaries —
  `Edit  E`, `Reject  R` (red text), `Skip  S`.
- **Behaviour.** Bindings: `J`/`K` or `↓`/`↑` move, `Enter` approve+advance, `E` open 2c for this
  asset, `R` reject (requires ≥ 1 rework flag; if none is selected, focus the flag row instead of
  rejecting), `S` skip. Decisions are optimistic and undoable with `⌘Z`/`Ctrl+Z` until the queue
  is left. When the queue empties, the centre column shows an end state
  (`12 reviewed · nothing left in the queue`) rather than an empty frame.

**Status strip.** `J / K move · ⏎ approve · R reject · 10 assets left in queue`.

---

## 2e — Queue

**Purpose.** Know what the machine is doing, what it spent, and exactly why something failed.

**Layout.** NavBar → stat row (`padding:16px 20px; grid-template-columns:repeat(5,1fr); gap:12px`)
→ body `flex:1; padding:0 20px 20px; gap:16px` (jobs `flex:1` + detail `width:376px`) → StatusStrip.

**StatCard**: `background:#fff; border:1px solid #e4e6ea; border-radius:10px; padding:13px 14px`;
value `23/700; letter-spacing:-.02em`; label `11.5 #8b919c`.
Cards: `2 running` · `5 pending` · `31 done today` · `2 failed` (value in `#b3261e`) ·
`$1.86 model spend today`.

**Jobs panel** (`background:#fff; border:1px solid #e4e6ea; border-radius:11px; overflow:hidden`):

- Header `padding:12px 16px; border-bottom:1px solid #eef0f3`: `JOBS` label, spacer,
  filter pills `All` (dark, active) / `Running` / `Failed`, then `Concurrency 2 ▾` (`11.5 #5c636e`).
- Rows `padding:12px 16px; gap:12px; border-bottom:1px solid #f4f5f7`:
  40 × 30 thumb · asset name `12.5/600` + step name `11 #8b919c` ·
  a 150px progress column (5px bar + `10.5px` mono `72% · ~8 s`) ·
  a 62px state chip (`padding:3px 9px; radius:999px; 11/600; text-align:center`) ·
  `Cancel` (`11.5 #5c636e`, `width:44px; text-align:right`).
- State colours — running: bar `#2f6fed`, chip `rgba(47,111,237,.14)` / `#2f6fed`;
  pending: bar `#dfe2e7`, chip `#f0f2f5` / `#5c636e`; failed: bar `#e0b3ae`,
  chip `rgba(179,38,30,.1)` / `#b3261e`; done: bar `#9ecfab`, chip `rgba(26,127,55,.13)` / `#177d34`.
- Step names must match the API's `QueueJobType` vocabulary:
  `analyze` → "Analyze (vision)", `generate_metadata` → "Generate metadata",
  `enrich_keywords` → "Enrich keywords", `embed` → "Embed (exiftool)", `upload` → "Upload (SFTP)".
  Video pre-steps show as "Extract keyframes".
- **Behaviour.** Rows are driven by `listQueueJobs` + `onJobUpdated`; patch by job id, never
  re-sort while the user is looking at a row (append new jobs at the tail). `Cancel` on a pending
  job removes it; on a running job it requests abort and shows `cancelling…` until confirmed.

**Failure detail panel** (376px):

- Header: red dot + `cliff-path-timelapse.mp4` (`12.5/700`) + `failed 2×` (`11 #8b919c`).
- Error callout: `padding:11px 12px; radius:9px; background:rgba(179,38,30,.07);
  border:1px solid rgba(179,38,30,.22); 12.5/400 #8f2019` —
  bold `Extract keyframes failed`, then `ffmpeg not found: program not found in PATH`.
- `LOG` block: `border:1px solid #e4e6ea; radius:8px; background:#fbfcfd; padding:10px 11px;
  font:11px/1.6 ui-monospace; color:#5c636e`, timestamped lines, the error line in `#b3261e`.
  Scrolls independently; auto-scrolls to the tail only when already at the bottom.
- `FIX` block: prose (`12.5 #5c636e`) naming the exact remedy —
  `Point Sphinx at an ffmpeg binary in Settings → Video pipeline, or drop still frames in manually.`
  — plus a secondary button `Open video settings` in `#2f6fed` that navigates to 2g with the
  Video pipeline section focused.
- Footer: primary `Retry job` (`flex:1`, `retryJob(id)`) + secondary `Remove`.
- Empty state when no job is selected: `Select a job to see its log` centred in `#8b919c`.

**Status strip.** `Worker pool 2/4 · avg analyze 3.4 s · avg generate 2.1 s`.

---

## 2f — Sites

**Purpose.** Define what "valid" means per stock site, and how files get there. This is the source
of truth behind every counter and compliance dot elsewhere in the app.

**Layout.** NavBar → body `flex`: profile list `width:400px; background:#fff;
border-right:1px solid #e4e6ea; overflow-y:auto` + detail `flex:1; padding:20px; gap:16px;
overflow-y:auto` → StatusStrip.

**Profile list rows** (`padding:14px 16px; gap:11px; border-bottom:1px solid #f4f5f7`):
8px status dot (`margin-top:6px`) · site name `13/700` + status word (`11 #8b919c`) ·
limits line `11.5 #5c636e` · transport line `11 #8b919c` mono. Mock rows:

| Site | Limits | Transport | Status |
|---|---|---|---|
| Shutterstock | title ≤ 200 · desc ≤ 200 · 7–50 keywords | SFTP · ftp.shutterstock.com | connected (green) |
| Adobe Stock | title ≤ 70 · desc ≤ 200 · 1–49 keywords | SFTP · sftp.contributor.adobe.com | connected (green) |
| Getty / iStock | title ≤ 120 · desc ≤ 500 · 5–50 keywords | CSV manifest export | no transport (amber) |
| Alamy | title ≤ 150 · desc ≤ 500 · 5–50 keywords | not configured | off (grey) |

The list ends with a `12 #8b919c` note: `Metadata is validated against every enabled profile
before a file can be approved.`

**Detail — three stacked cards** (each `flex:none`, `background:#fff; border:1px solid #e4e6ea;
border-radius:11px; padding:16px 18px`; `flex:none` matters or the last card gets squashed):

1. **Site card.** Title `15/700` `Shutterstock` + `default target` chip (green tint) + spacer +
   `Enabled` label and a toggle (34 × 19px pill, `background:#2f6fed`, 15px white knob inset 2px).
   Then a three-column field grid (`gap:12px`): `Title max` 200, `Description max` 200,
   `Keyword range` `7 – 50`. Field = `padding:7px 9px; border:1px solid #dfe2e7; radius:7px; 13px`;
   label above at `11.5 #8b919c`. Then three checkboxes (`12.5 #5c636e`):
   `Reject keywords containing brand or trademark terms` (on),
   `Truncate title on export rather than blocking approval` (on),
   `Include location keywords from EXIF GPS` (off).
2. **Upload transport.** `UPLOAD TRANSPORT` label; a `2fr 1fr` grid: Host
   (`ftp.shutterstock.com`, mono), Port (`22`), Username (`contributor-9182`), Key
   (`~/.ssh/id_ed25519`). Below: secondary `Test connection` (`#2f6fed` text) and a result line —
   green dot + `Last handshake 4 min ago · 18 files uploaded today` (`11.5 #177d34`).
   While testing show `testing…` in `#8b919c`; on failure the line turns `#b3261e` and quotes
   the transport error verbatim.
3. **Demand data.** `DEMAND DATA USED FOR KEYWORD HEAT` label; a dark active pill
   `Shutterstock search volume`, the note `refreshed weekly · 1.2 M term index` (`12.5 #5c636e`),
   spacer, `Refresh now` link. This chooses the demand half of the heat score.

**Behaviour.** Fields write through to `setLimiterProfile` / `updateSftpProfile` on blur (debounced),
with no Save button — matching the "Saved automatically" model used in Settings. Disabling the last
enabled profile is blocked with an inline explanation, since approval needs at least one target.

**Status strip.** `2 of 4 profiles enabled · strictest limit wins when several are enabled`.

---

## 2g — Settings

**Purpose.** Everything machine-shaped: which model, what to tell it, where the local binaries are,
where the data lives.

**Layout.** NavBar → body `flex`: sidebar `width:208px; background:#fff;
border-right:1px solid #e4e6ea; padding:14px 10px; gap:2px` + content
`flex:1; padding:20px 24px; gap:16px; max-width:900px; overflow-y:auto` → StatusStrip.

**Sidebar items** — `padding:7px 11px; radius:7px; font-size:13px`; active
`background:#eef2fb; color:#2f6fed; font-weight:600`, rest `#5c636e`:
`AI provider` (active) · `Prompt & guidance` · `Keyword enrichment` · `File embedding` ·
`Video pipeline` · `Storage & data` · `Shortcuts`. Clicking scrolls the matching card into the
content column and marks it active on scroll (scroll-spy), so all cards stay reachable.

**Card 1 — AI provider** (`flex:none`, `padding:17px 18px`): title `15/700` + `connected`
green-tint chip. Two-column grid (`gap:12px`): Provider select (`OpenAI (GPT-4o)`,
`Anthropic (Claude)`, `Local (Ollama)` — mirror the API's `ProviderKind`), Vision model select
(`gpt-4o`, `gpt-4o-mini`), API key (`type=password`, mono, `sk-live-…`), API base URL
(placeholder `proxy / gateway override`, hint `blank = provider default` in `#b0b6be`).
Footer row: secondary `Test request` + note `Key is stored locally in sphinx.db and never leaves
this machine except to the provider.` (`11.5 #8b919c`).

**Card 2 — Prompt & guidance** (`flex:none`): title `15/700`; a 3-row textarea labelled
`Project guidance` with the hint `appended to every request`, containing
`Fine-art nature photography. Prefer species and place names over mood words. Never invent
locations. Avoid brand names.` (`13px; line-height:1.5; resize:none` in the mock — allow vertical
resize in the app). Then a three-column grid: `Keywords requested` 40, `Demand floor` 20,
`Tone` select (`Factual` / `Commercial` / `Editorial`). These map to `AnalysisConfig` +
`KeywordConfig`.

**Card 3 — Local tools** (`flex:none`, `padding:15px 18px 11px` header then rows):
each row `padding:12px 18px; border-top:1px solid #f4f5f7; gap:11px` = status dot, a name
(`13/600`) with a mono/prose detail line (`11.5 #8b919c`), and a right-hand action.

- `exiftool` — green — `C:\tools\exiftool.exe · v12.76` — `Change` (`12 #5c636e`).
- `ffmpeg` — amber — name carries an inline `required for video` in `11.5 #8a5b12`; detail
  `not found in PATH — video assets stay blocked`; action = secondary `Locate…` button in `#2f6fed`.
- `sphinx.db` — green — `%APPDATA%\Sphinx\sphinx.db · 18.2 MB · 42 assets` — `Reveal`.
- **Behaviour.** `Change`/`Locate…` open a file dialog, then re-run `checkExiftool` /
  `checkFfmpeg` and update the dot without a page reload. `Reveal` opens the OS file manager.

**Status strip.** `Sphinx 0.4.2 · Tauri 2.0 · settings written on change`.

---

## Cross-screen consistency checklist

Before calling the rebuild done:

- Six nav destinations, same order, on every screen; the active one is the only tinted item.
- Stage dots always read left-to-right in pipeline order and never show more than five.
- Keyword chip colour is derived from the heat score only — never from selection or state.
- Every character counter names the site it grades against when the strictest limit is site-specific.
- Every panel that can be empty has a written empty state; no bare frames.
- Every destructive or impossible action is disabled with a reason, not hidden.
- Nothing in the app scrolls as a whole page; the nav bar and status strip are always visible.
- All stacked cards in a scrolling column carry `flex:none` so the last one never compresses.
