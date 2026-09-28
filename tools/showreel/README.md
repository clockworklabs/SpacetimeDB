# SpacetimeDB promo video (showreel)

A ~90 s, 1920×1080 @ 60 fps motion-graphics promo for SpacetimeDB, generated entirely from code:
a Canvas 2D renderer (`@napi-rs/canvas`) plus a procedurally synthesized soundtrack. There is no
template or stock footage. Everything is drawn from the scene code in this directory.

## Build

Requirements: Node 22+, `ffmpeg` with `libx264`. Run from this directory:

```sh
npm install
npm run build          # audio → video (parallel, motion-blurred) → mux  ≈ 2–3 min on 24 cores
# output: ./spacetimedb-showreel.mp4   (gitignored; ~45 MB)
```

Individual steps:

| Command | What it does |
|---|---|
| `npm run audio` | Synthesizes `out/reel.wav` (120 BPM, A minor) from `audio.js` |
| `npm run video` | Renders all frames in parallel workers into `out/seg_*.mp4` (`--jobs N`, `--samples S` = motion-blur subframes) |
| `npm run mux` | Concatenates segments, adds audio + light film grain → `spacetimedb-showreel.mp4` |
| `npm run stills -- 12.5 42 80 --samples 1` | Renders PNG stills at given times to `out/still_<t>.png` (fast way to review a change) |
| `./sheet.sh out.png a.png b.png c.png d.png` | 2×2 contact sheet of four stills |

Review loop that works well: edit `scenes.js`, render a handful of stills with `--samples 1`,
build a contact sheet, inspect, and only then do the full build.

## Files

| File | Role |
|---|---|
| `lib.js` | Canvas/fonts setup, palette `C`, easing `E`, helpers (`riseText`, `text`, `glass`, `drawMark` = the logo mark from `images/dark/logo.svg`), asset loading (framework logos from `docs/static/images/logos`, wordmark from `images/dark/logo-text.svg`). **`DUR` (total length) lives here.** |
| `scenes.js` | The whole timeline. `T` = scene start times; one function per scene using local time `u = t - T.<scene>`; global FX lists (`FLASHES`, `SHAKES`, `SHOCKS`, `BURSTS`); HUD; `frame(ctx, t)` composes a frame. |
| `audio.js` | Synth instruments (kick, clap, hats, bass, pads, plucks, risers, whooshes, impacts, reverb) + an arrangement keyed off `T`/`EVENTS` imported from `scenes.js`, so sound stays in sync when scenes move. |
| `render.js` | Stills mode, worker mode and orchestrator; motion blur = averaging `--samples` subframes over a 180° shutter. |
| `mux.sh`, `sheet.sh` | ffmpeg helpers. |

## Timeline (current: 90 s)

| Start | Scene (`T` key) | Content |
|---|---|---|
| 0:00 | `hook` | "What if / Your database / was the server?" |
| 0:05 | `stack` | "Your backend, today." 9-service diagram with error badges → collapses into the logo: "One system." |
| 0:15 | `code` | Rust module typed (table + reducer) → `spacetime publish` → `module.wasm` flies into the core → `spacetime generate` → `client.ts` subscription |
| 0:27 | `rt` | 36 orbiting clients, reducer calls in, updates fan out; live `SELECT * FROM player` table; BitCraft line; zoom through the logo + flash |
| 0:36 | `cloud` | "Ship it to Maincloud." `spacetime login` / `publish --server maincloud`, managed-feature badges, scale-to-zero traffic chart, "Start free" button |
| 0:43 | `feat` | "Everything your backend needs. Built in." 6 cards (reducers, subscriptions, views, scheduled reducers, auth, hot-swap) |
| 0:49.5 | `speed` | 303,920 TPS counter + benchmark bars, "~110× the throughput of Bun + Postgres." |
| 0:57 | `langs` | Rust / C# / TypeScript / C++, then a 14-logo client SDK grid |
| 1:03 | `ai` | "Built for AI agents." agent-setup.md terminal, 9 skill cards, "…glue-free." then "And a place for agents to work…" |
| 1:12.5 | `scale` | "Does it scale?" contention chart (100k TPS ceiling vs single-threaded line) → BitCraft root + region databases, zero-overhead quote; implodes into the outro |
| 1:24.5 | `outro` | Logo build, wordmark with glint, "Development at the speed of light.", `spacetimedb.com` + install command |

To retime: change `T` (and `DUR` in `lib.js`), then adjust in-scene `u` timings and any exit ranges.
`K` holds derived key moments (impacts, flashes). `audio.js` blocks are relative to `T.*`.

## Decisions & feedback log (keep these when iterating)

Stakeholder feedback so far:

- **Readability first**: it's a promotional video, so every headline must stay on screen long enough to read (≈2.5–4.5 s after it finishes revealing). The first 30 s cut was too fast.
- **Length target ≈ 1:30.**
- **Order**: performance comes right after the built-in features; "Does it scale?" is the last section before the logo.
- **Name**: keep "SpacetimeDB" (not the newer "Spacetime" umbrella brand).
- **No Convex price comparison** (considered cheeky). Keep the "Convex" label in the benchmark chart as is. No pipelining caveat on screen.
- **Syntax highlighting keeps its own palette** (`SYN` in `scenes.js`: purple attributes, pink keywords, yellow types, green function calls, cyan strings). It is exempt from the accent rules below.
- "Start free" on the Maincloud scene is a prominent solid-green button.

Designer's brand rules (applied in `lib.js`/`scenes.js`):

- Background is flat `#0B1114`. **No background gradients**: no color fields, vignette, halos, or light rays.
- **No outer glows.** `lib.js` makes `shadowBlur` a no-op on every canvas context, so glows can't creep back in.
- **Inter** for sans (and never all-caps with Inter); **Source Code Pro** for mono. Uppercase is only used for small mono labels. Geist is registered only as a glyph fallback (→ etc.).
- **Brand gradient (pink→purple→green) on one element per screen**, normally the key word of the headline (`fill: 'brand'` in `riseParts`).
- **Accent color used sparingly**: green is the single accent (plus red only for errors and the 100k ceiling). Legacy `C.pink/purple/yellow/cyan` are remapped to neutrals; use `BRAND.*` only via `brandGrad`.

## Sources behind on-screen claims

- Benchmark: https://spacetimedb.com/blog/benchmarking. Contended transfer workload (α = 1.5): SpacetimeDB 303,920 TPS; Node.js+SQLite 3,188; Bun+Postgres 2,773; Node.js+Supabase 2,534; Node.js+Postgres 961; Convex 127. "~110× Bun + Postgres" is the published ratio (essay: https://spacetimedb.com/essays/spacetime/03-performance-that-changes-the-design.md). Footnote says "Vendor-run".
- Scaling: https://spacetimedb.com/blog/how-does-spacetime-scale. "(1 / 1 ms) / 1% = 100,000 TPS. No matter how many cores you add"; 1% contention makes a cluster slower than a single core; BitCraft root + region databases; "hundreds or thousands of databases"; zero-overhead principle quote. The cluster curve's *shape* is illustrative; only the ceiling and the ~300k line are from the post.
- BitCraft "entire backend is one module": repo `README.md` and `docs/docs/00100-intro/...what-is-spacetimedb.md`.
- Maincloud (fully managed, serverless, scales to zero, handles scaling/replication/backups, `spacetime publish --server maincloud`, free tier): `docs/docs/00300-resources/00100-how-to/00100-deploy/00100-maincloud.md`, https://spacetimedb.com/pricing. Note: the pricing page lists replication and backups under Pro and above.
- Code: Rust module syntax from `skills/rust-server/SKILL.md`; TS client from `skills/typescript-client/SKILL.md` (2.0 APIs).
- AI: https://spacetimedb.com/agent-setup.md and the skills index at https://spacetimedb.com/.well-known/agent-skills/index.json (these agent-only files are listed in the sitemap); agents-at-runtime line from essay 07.

## Known limitations

- The soundtrack was checked with loudness/waveform analysis only; nobody has listened to it critically yet.
- Rendered outputs (`out/`, `*.mp4`) are not committed; rebuild them with `npm run build`.
