# Sphinx — product brief & user flows (input for a UI redesign)

## What this document is

This is a **content and behavior brief**, not a visual spec. It exists so that whoever
designs the next version of Sphinx's UI (currently: Claude Design) understands the product,
the person using it, everything the app can actually do, and — most importantly — *why the
current UI isn't working*, without needing to reverse-engineer any of that from code or from
the existing screen-by-screen spec (`new_ui_design/README.md` + `SCREENS.md`, describing the
UI that shipped and is now being reconsidered).

Deliberately **not** included here: colors, type scale, spacing, component pixel geometry.
That's a fresh decision for this redesign, not a constraint carried over from the last one.

---

## 1. What Sphinx does

Sphinx is a desktop app for **stock-media contributors** — people who shoot photos and video
and sell them through stock marketplaces (Shutterstock, Adobe Stock, iStock, Dreamstime,
123RF, Pond5, and any other site the user chooses to add). Selling on these sites requires,
for every single file: an accurate title, a description, and a set of keywords chosen so
buyers can actually find the file — all fitted to each site's own length and count rules.
Doing this by hand for a shoot of a few hundred files is the tedious, repetitive part of the
job. Sphinx automates it:

1. **Ingest** the files into a local catalog (hashing them so duplicates are never
   re-processed or re-billed).
2. **Analyze** each one with a vision AI model (cloud provider or a local model) to get a
   factual description of what's actually in the frame.
3. **Generate** a title, description, and keyword set from that analysis, sized to fit
   whichever stock sites the user targets.
4. Let the user **review and correct** what the model produced — this is the one step a human
   is required for, because the model's factual read of a photo is not the same as a
   marketable listing, and mistakes here cost real money (wrong keywords = the file never
   gets found; a wrong claim = a licensing problem).
5. **Embed** the final metadata into the file itself (IPTC/XMP, via `exiftool`) so it's
   portable and self-describing.
6. **Upload** the finished file to each target site over SFTP.

Everything runs **locally**. Files are read from wherever they already live on disk — Sphinx
never copies or moves them, it just remembers their path — and the whole catalog is one
SQLite database on the user's machine. The only network calls are: the vision-AI model, the
marketplaces' keyword-demand APIs (used only to weight which keywords are actually worth
including), and SFTP to deliver the finished files.

## 2. Who's using it, and what a session looks like

One user, running this on their own machine, after a shoot. There's no login, no team, no
cloud account — it's a personal production tool, closer to a photo editor's Lightroom/Capture
One workflow than to a SaaS dashboard.

A typical session:

- Come back from a shoot with 30–400 new photos and/or video clips.
- Drop the folder into Sphinx (or let a watched folder pick them up automatically).
- Kick off analysis for everything that's new.
- While that runs in the background, go do something else — or watch it happen.
- Come back and **triage**: most of what the model produced is good enough to approve as-is;
  a meaningful minority needs a keyword swapped, a wrong claim removed from the description,
  or (rarely) full manual rewrite.
- Approve the good ones in bulk; open the ones that need attention individually.
- Let embedding and upload run.
- Occasionally something fails — a keyword-API hiccup, a missing local tool, a network
  blip — and the user needs to understand *why*, fix the actual cause (which is often outside
  the file itself — a missing tool, a bad credential, ffmpeg not installed), and retry just
  that step without re-doing the whole file.

Less frequently, the user does **setup and maintenance**: pointing the app at a new AI
provider or a local model, adding a stock site they've started selling on, fixing keyword
choices from three weeks ago because a buyer complained, or cleaning out files they imported
by mistake.

**This is a power-user tool used many times a day.** Optimize for fewer clicks and clear
state over onboarding hand-holding; density and information-on-screen are welcome. The person
using it already understands stock photography — the UI doesn't need to explain what a
keyword or a title limit is, just make working with them fast.

## 3. Why we're redesigning: what's wrong with the current UI

The current shipped UI (seven screens: Import, Library, Asset Editor, Review, Queue, Sites,
Settings — see the old handoff docs in this folder for exactly what was built) works, but the
user's own words after living with it: **"features are kind of all over the place."**
Specific friction that's been observed:

- **Library and Review overlap without being the same thing.** Both let you inspect an
  asset's proposed metadata and approve it into the pipeline, with different UI, different
  keyboard behavior, and no clear rule for which one a user should reach for.
- **Queue feels disconnected from the assets it's about.** Every job in the queue belongs to
  one specific asset that's *also* sitting in the Library grid with its own stage indicator —
  but they're two separate screens with no link between "this card is stuck" and "here's the
  job that's stuck, and why."
- **Sites bundles three unrelated concerns** under one screen: per-site metadata rules (title
  length, keyword count), SFTP delivery credentials, and stock-site keyword-demand API keys.
  They only share the fact that they're both "about a stock site" — a user configuring where
  files get delivered and a user tuning keyword limits are doing conceptually different tasks.
- **Credentials are split across two screens with no consistent pattern**: the AI provider's
  API key lives in Settings, SFTP passwords and keyword-API keys live in Sites, and there's no
  single place that answers "what have I given this app access to."
- **New features get bolted onto the existing structure rather than reshaping it.** Recent
  additions (asset delete, custom site profiles, import progress, notifications — all
  described in §11) were each wedged into whichever existing screen was the closest fit,
  which is exactly the pattern that produces "all over the place" over time.

**The ask for this redesign is explicitly not "reskin the same seven screens."** Section 10
below invites a fresh take on the information architecture itself — grouping, navigation, and
which actions live next to which other actions — using everything in this document as the
input, not the current screen list as a constraint.

## 4. Core concepts (glossary)

Read this section before the flows — the flows assume these terms.

- **Asset** — one ingested file (a photo or a video clip). Identified by a content hash, so
  the same file dropped in twice is recognized as a duplicate rather than processed twice.
- **Project** — a named workspace with its own AI-provider config, site profiles, and
  settings. The data model supports many projects, but today the whole app only ever operates
  on one built-in "Default" project — there is no project switcher. (A redesign is free to
  either keep assuming a single project or design for a switcher — see §10.)
- **Media type** — `image` or `video`. Images go straight to analysis; video first goes
  through keyframe extraction (a handful of representative stills, pulled at scene changes) so
  the vision model can "watch" it as a sequence of frames, optionally alongside a transcript of
  its audio.
- **Analysis** — the vision model's factual read of an asset: a description, the concrete
  subjects visible, the scene/setting, mood, dominant colors, candidate keywords, whether it
  contains anything that would restrict commercial licensing (recognizable people, brands,
  private property), and any legible text. Stored once per asset (re-running replaces it).
- **Metadata** (a.k.a. "the generated record") — the actual title, description, and keyword
  set derived from the analysis, fitted to whichever site profile is active. This is what gets
  embedded and uploaded, and it's the thing a human edits during review.
- **Keyword demand / heat** — each keyword carries two numbers: the model's confidence that
  it's relevant, and (optionally) real search-demand data pulled from a stock site's own API.
  Blending the two gives a "heat" score used to help the user prioritize which keywords matter
  most and which weak ones might be worth dropping. A keyword below the user's demand floor is
  flagged as weak, not silently removed.
- **Limiter / site profile** — the length and count rules one stock site enforces (max title
  characters, max description characters, min/max keyword count). Six real marketplaces ship
  as built-in presets; the user can add their own for a site that isn't in that list. **When
  more than one site profile is "enabled" as a target, the strictest limit of the enabled set
  wins** — this is the rule that drives every character counter and pass/fail check in the app.
- **Provider / analysis config** — which vision AI does the analyzing: a cloud API (OpenAI,
  Google Gemini, Anthropic Claude) or a **local model via Ollama** running on the user's own
  machine (no API key needed, but it does need a locally-running Ollama server and benefits
  from a GPU — see below). Each provider keeps its own saved config (model, key, base URL), so
  switching providers doesn't lose the others' settings.
- **Job / queue** — every pipeline step (analyze, generate metadata, enrich keywords, embed,
  upload) that runs against an asset does so as a queued job: pending → running → done/failed,
  with automatic retry with backoff, and a manual retry available for anything that ends up
  failed. Jobs can be enqueued one at a time or in bulk across a selection of assets.
- **Embed** — writing the final title/description/keywords into the file itself as IPTC and
  XMP tags, via the external `exiftool` binary. This is what makes the metadata travel with the
  file rather than living only in Sphinx's database.
- **Upload / SFTP profile** — a saved connection (host, port, username, password, remote
  directory) for delivering finished files to one stock site over SFTP. Passwords are stored in
  the OS credential store, never in the app's database. First connection pins the host key
  (trust-on-first-use) so a later host-key change is visible rather than silently accepted.
- **Watch folder** — a single folder Sphinx can monitor continuously; anything new that
  appears there is ingested automatically (today: one watch folder at a time).
- **Local tools** — `exiftool` (required for embedding) and `ffmpeg` (required for video
  keyframe extraction) are external binaries the app shells out to. They are not bundled; the
  app looks for them on the system PATH or at a path the user points it to, and reports
  reachable/not-reachable so the user knows *before* a job fails why it will fail.
- **GPU check** — a best-effort, advisory-only detection of whether the machine has a usable
  GPU, surfaced only to warn that a local AI model will be slow on CPU alone — never a hard
  block.

## 5. The pipeline — the backbone of the whole app

Every asset's life follows the same six-stage path, and this progression (more than any one
screen) is the thing a redesign should make legible at a glance, everywhere an asset appears:

| # | Stage | What happens | What can go wrong |
|---|-------|--------------|---------------------|
| 0 | **New** | File is hashed and cataloged. Nothing else has happened yet. | Unsupported file type; duplicate of an already-ingested file; a video without `ffmpeg` available is blocked here. |
| 1 | **Analyze** | Vision model examines the image (or video keyframes + optional transcript) and returns a factual description. | Provider not configured / bad API key; network/timeout; local model unreachable; model refuses or returns unusable output. |
| 2 | **Generate** | Analysis is turned into a title/description/keyword set sized to the active site profile(s). Pure/local computation, no network call, effectively never fails once analysis exists. | — |
| — | **Enrich** *(optional, not a hard gate)* | Keywords are cross-checked against a stock site's real search-demand data and the weakest ones get supplemented. Skippable — nothing downstream requires it. | Keyword-provider API key missing/invalid; API error. |
| — | **Review / edit** *(human step, not a queued job)* | The user reads the proposal, corrects anything wrong, reorders or swaps keywords, and either approves it or sends it back. This is the only stage that isn't automatic and isn't optional to skip *thoughtfully* — it's the actual point of the app. | User rejects and needs a way to say *why* (so the same mistake doesn't reproduce next time), or defers a decision. |
| 3 | **Embed** | Approved title/description/keywords are written into the file via `exiftool`. | `exiftool` not found/misconfigured; file locked or read-only; unsupported file format for a given tag. |
| 4 | **Upload** | File is delivered to each enabled site's SFTP target. | Not configured (no SFTP profile for that site); connection/auth failure; site-specific rejection (e.g. an account not yet approved to submit to that marketplace). |

A single asset can be sitting at any of these stages at any time, and a batch import will have
assets spread across all of them simultaneously — the UI needs to make "where is everything
right now, and what's stuck" answerable at a glance without cross-referencing multiple screens.

## 6. Primary user flows

These are told as stories, not screen walkthroughs — deliberately not tied to the current
navigation, since that's what's being reconsidered.

### Flow A — First-time setup
A new user opens Sphinx for the first time. Nothing is configured. They need to: point the app
at an AI provider (paste a cloud API key, or point at a local Ollama server), understand
that a couple of local tools (`exiftool`, `ffmpeg`) may need to be installed or located before
the file-writing and video steps will work, and see clearly which of those things are and
aren't ready before they try to use them — not discover a missing tool only after a job fails.
They don't need to configure a stock site or SFTP yet to start experimenting.

### Flow B — Bulk import and triage a new shoot (the everyday flow)
User drops a folder of ~150 mixed photos and video onto the app (or picks it via a folder
browser). Before anything is committed, they see what will and won't be ingested (duplicates,
unsupported formats, videos that need `ffmpeg`) so nothing surprising happens. They commit the
import, optionally with "start analyzing immediately" turned on. Import of a large folder
takes real time (hashing every file) — the user needs to see that it's working and roughly how
far along it is, not wonder if the app has frozen.

Once analysis has run across the batch, the user needs to see, at a glance, across the whole
batch: which assets are ready to approve as-is, which need a look, and which failed outright.
They approve the obviously-good ones with minimal friction — ideally without opening each one
individually — and drill into the ones flagged as needing attention. For those, they can
either fix things quickly in-place or jump to a focused single-asset editing view. Approving
queues embedding and upload automatically for the sites they've enabled.

### Flow C — Deep-edit one asset's metadata
User opens a single asset that needs real correction: the model got a keyword wrong, called a
private residence "editorial" when it should be flagged for release requirements, or the
description needs a full rewrite. They need: the image/video large enough to actually judge
against; the full keyword set as something they can add to, remove from, and reprioritize
(order matters for which keywords are emphasized, even though it isn't sent to any site); live
feedback on whether the current title/description/keyword count still fits every site they're
targeting; and a way to see what changes when they preview against a *specific* site (a title
that fits Shutterstock might not fit Adobe Stock). They can move to the next asset without
losing their place.

### Flow D — A job fails, and the user needs to fix it
Something in the pipeline failed for one or more assets — most commonly a missing/misconfigured
tool, a bad credential, or a transient network error. The user needs three things, ideally
without hunting: **which asset**, **which step failed and why** (the actual error, not just
"failed"), and **the concrete fix** (often: "go configure X" rather than anything they can do
to the file itself). After fixing the underlying cause (e.g. pointing Settings at the right
`exiftool` binary), they retry just the failed step — not the whole pipeline for that asset.
Failures that share the same root cause (e.g. "AI provider misconfigured" failing ten assets at
once) should be obviously the same problem, not ten separate mysteries.

### Flow E — Add a new stock site
User starts selling on a marketplace Sphinx doesn't have a built-in preset for. They need to:
define its title/description/keyword-count rules (so the app grades against it correctly),
optionally set up SFTP delivery for it, and have it show up everywhere sites are already
represented (compliance checks, the enabled-sites list, upload targets) without extra wiring.
They can also remove a site profile they added by mistake or no longer use. (Built-in presets
for the six well-known sites always remain available as a starting point — they aren't
something a user deletes, only something they can choose not to target.)

### Flow F — Switch some or all work to a local AI model
User wants to try analyzing with a local model (privacy, cost, or just experimenting) instead
of a cloud provider. They point the app at a local Ollama server, and the app tells them
plainly whether it's actually reachable and what models it has available — before they try to
use it and get a confusing failure. If their machine has no GPU, they're warned it'll be slow,
but nothing stops them from proceeding. Switching back to a cloud provider later doesn't lose
the local config, or vice versa — each provider's settings persist independently.

### Flow G — Clean up the library
User imported something by mistake (wrong folder, a test file, a duplicate that slipped
through under a different path) and wants it gone from the catalog. They need a clearly
reversible-feeling, low-ceremony way to remove it from Sphinx's library — with a plain
statement that this doesn't touch the actual file on disk, since Sphinx never owned it in the
first place. This should be doable without leaving whatever bulk-browsing view they're already
in.

### Flow H — Hands-off ingestion via a watch folder
User points Sphinx at a folder they regularly drop new exports into (e.g. straight from
Lightroom) and leaves the app running. New files appear in the catalog on their own. The user
wants a passive but visible sense that this is working (folder being watched, how many new
files just came in) without needing to babysit it, and an easy way to pause or remove the
watch later.

### Flow I — Quick daily check-in
User opens the app just to see: is anything stuck or failed that needs attention, is anything
still mid-flight, and is there a backlog of unreviewed assets waiting. This should be
answerable in the first few seconds of opening the app, not by visiting several screens in
turn.

## 7. Functional building blocks — what each thing *is* (not how it should look)

Grouped by concept rather than by current screen, since the grouping itself is up for
reconsideration.

**Representing one asset.** A thumbnail (real preview, not a placeholder — images and video
both need to visually preview, video needs actual playback somewhere, not just a static
frame), its media type, its current pipeline stage (§5's six stages, ideally readable as a
single compact indicator wherever an asset appears in any list), and a plain-language flag for
"needs your attention" vs. "looks ready" vs. "blocked" vs. "failed."

**The metadata record.** Title (single line, judged against a length limit), description
(a couple of sentences), and the keyword set — which is not just a list but an *ordered,
editable, per-word-scored* collection: each keyword carries a heat score (confidence + demand
blended), can be removed (and recovered — rejecting isn't necessarily final), can be added to
freely, and its order is meaningful to the user (priority) even though it's not sent anywhere.

**Compliance / fit-checking.** For any given title/description/keyword-set draft, and any set
of "enabled" site targets, there's a pass/near-limit/fail read against each site's rules, and
an overall "does this pass everything I'm targeting" answer that gates whether embedding is
allowed to proceed. When multiple sites are targeted, the UI needs to make clear *which* site
is the binding constraint on a given field, not just "over some limit somewhere."

**The job/queue system.** A job belongs to exactly one asset and one pipeline step. Its
lifecycle (pending → running → done/failed, with attempt count) and, on failure, the actual
error text and attempt history need to be visible without hunting — and tied back to *why*
before asking the user to fix and retry. Bulk actions ("do this step for these N selected
assets") are a first-class way jobs get created, not just individual triggers.

**Confirmation & progress conventions.** Any action that removes something from the catalog
should say plainly that it doesn't touch the file on disk. Any action that could take real
wall-clock time (a big folder import, in particular) needs a visible in-progress state — this
was a genuine, reported problem: a large import with no progress indicator reads as a frozen
app, not a working one.

**Notifications.** Every completed or failed action should surface a lightweight, transient,
color-differentiated acknowledgment (this exists today as bottom-center toasts: green for
success, red for failure, amber for a destructive-but-intended action like a removal, blue for
a neutral heads-up) — the point is the user never has to wonder "did that actually do
anything," without demanding a click to dismiss.

**Tool/provider health.** exiftool, ffmpeg, the active AI provider, and (when using a local
model) the Ollama server and GPU availability all have a simple reachable/not-reachable state
that should be visible *before* the user hits a wall using a feature that depends on them, with
a direct path to fix it (locate a binary, enter a key, point at a server) from wherever that
state is shown.

**Credentials, generally.** Three different kinds of secret exist — the AI provider's API key,
SFTP passwords, and stock-site keyword-API keys — and a user should be able to reason about
"what has access to what" as one coherent idea, even if the redesign keeps them in different
places contextually.

## 8. What the backend can already do (design against this; don't invent gaps, don't be shy about using all of it)

This redesign is a **frontend rework** — the underlying app (Rust/Tauri commands + local
SQLite catalog) already supports everything below. Nothing here requires new backend work;
if a flow genuinely needs something not on this list, flag it as an open question rather than
assuming it exists.

- Ingest explicit files, or a whole folder recursively; maintain one watch folder; content-hash
  every file so duplicates are recognized regardless of path.
- List/browse the whole catalog; remove an asset from the catalog (never touches the file).
- Run vision analysis on one asset (image or video) against whichever provider is configured;
  four provider kinds today (OpenAI, Gemini, Anthropic, local Ollama), each with independently
  saved settings; check whether the configured provider/local server is actually reachable.
- Generate title/description/keywords from an analysis, fitted to a chosen site profile; save a
  user-edited version of that record.
- List, add, and remove site profiles (six built-in presets plus any the user creates); read
  and update the length/count rules on any of them.
- Embed a saved metadata record into a file via `exiftool`; check whether `exiftool` is
  reachable; export a CSV of metadata for a set of assets.
- Look up and save per-site keyword-demand credentials (Shutterstock, Adobe Stock); enrich a
  saved metadata record's keywords against that demand data.
- Enqueue any pipeline step (analyze / generate / enrich / embed / upload) for one asset or a
  batch of assets; list jobs and live counts by status; retry a failed job; get pushed
  real-time job-status-changed events rather than needing to poll.
- Create, edit, and delete SFTP delivery profiles per site (credentials live in the OS
  credential store, never in the app database); upload one asset over a saved profile.
- Extract keyframes and (optionally) a transcript from a video, feeding the same analysis path
  images use; check whether `ffmpeg` is reachable.
- Best-effort local GPU detection, purely advisory.

## 9. Constraints for the redesign

- **Desktop app, offline-first.** No login, no account, no cloud sync. Network calls exist
  only for the AI provider, keyword-demand APIs, and SFTP upload — everything else is local and
  should feel instant.
- **Files are referenced, never owned.** Sphinx stores the original path, not a copy. Any
  "remove" action is scoped to Sphinx's own catalog only — this needs to be unambiguous in the
  UI every time, since it's a real point of user anxiety around any delete-like action.
  Multi-Project data model exists but isn't exposed today — treat "one project" as the current
  reality, not an unchangeable constraint (see §10).
- **One watch folder at a time**, today — a hard current limit, not a deliberate design choice.
- **External tool dependencies are real and visible.** `exiftool` and `ffmpeg` are not bundled
  with the app; their absence is common and expected, and should degrade gracefully (block only
  the specific capability that needs them, never the whole app) rather than being treated as an
  error state.
- **Local AI has no credential but has a reachability and a performance dimension** (is the
  server up, is there a GPU) that cloud providers don't — don't force it through the exact same
  "enter an API key" mental model as the cloud providers if it doesn't fit.
- **This is a Windows-first but cross-platform build** (Tauri: Windows, macOS, Linux) — avoid
  OS-specific assumptions in the flows themselves (e.g. keyboard modifier should adapt per
  platform), even though the primary dev/test machine today is Windows.

## 10. Explicitly open for this redesign to decide

- **The seven-screen navigation is not a requirement.** Whether Library and Review should be
  one place with two modes, whether Queue should live embedded in Library instead of standing
  alone, whether Sites should split into "site rules" vs. "delivery" vs. "credentials," and
  whether Settings and Sites should merge into one "configuration" area — all open. Use §3 and
  §6 as the actual constraints (what needs to be reachable from what, what's currently
  disconnected that shouldn't be), not the existing screen boundaries.
- **Whether a project switcher belongs in this version at all,** given the backend already
  supports multiple projects but no UI has ever exposed it.
- **How much of the pipeline should be visible "ambiently"** (e.g. a persistent sense of
  "what's running / what's stuck") versus something the user navigates to on demand.
- **Whether/how bulk actions and single-asset deep-editing share a visual language** rather than
  feeling like two different apps (this is part of today's Library-vs-Review friction).

## 11. Recent additions (post-dates the old `README.md`/`SCREENS.md` in this folder — factor these in)

These shipped after the original seven-screen design and are not reflected in the old spec
files in this folder; treat them as current, real behavior to account for, not proposals:

- Video assets now preview and play (thumbnail + full playback) everywhere an image would have
  shown a thumbnail before — this was previously broken/missing.
- A visible progress indicator during a folder import (previously: no feedback at all during
  what could be a genuinely slow operation, which read as the app hanging).
- A meaningfully larger single-asset preview in the library's inspector panel (the old one was
  a small 96×64px thumbnail — too small to actually judge an image by).
- A hover-revealed delete affordance on library asset cards, with a plain on-disk-file-is-safe
  confirmation before removing.
- User-created custom site profiles (in addition to the six built-in presets), addable and
  removable.
- Color-coded, auto-dismissing toast notifications for every completed or failed action,
  bottom-center.
- Local AI (Ollama) as a fourth provider option alongside the three cloud providers, plus a
  best-effort GPU-availability warning shown when using it.
