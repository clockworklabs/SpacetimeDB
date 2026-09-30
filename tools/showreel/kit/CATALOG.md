# Release-video library catalog

Read this before designing a release video. It lists every reusable component (`kit/`) and every scene built so far
(to copy or adapt). Render the stills with:

```sh
node --expose-gc kit/gallery.js          # → kit/gallery/<name>.png (960×540), one per entry below
```

Reels import the kit with `const R = require('../release');` (it re-exports `kit/`). Scenes are plain functions
`sName(ctx, t)` that compute local time `u = t - T.name`, return early outside their slot, and draw with `slide()`
for entry/exit. See any `release-v2.*/scenes.js` for the pattern, and `RELEASE_PROMPT.md` for the rules.

**The feature comes first.** Use a component or copy a scene when it shows the feature well. When nothing fits, write a
new scene, note it in `DECISIONS.md`, and add it here if it could serve again.

## Frame and structure

| Component | Gallery | What it does |
|---|---|---|
| `makeReel({ DUR, T, SCENES, label, draw, seed })` → `{ frame, setSlow }` | any `ex-*` | Background grid + starfield, camera shakes, flashes, shockwaves, the HUD, the end fade. `draw` = scene functions in order. |
| `makeIntro({ pre, from, to, sub, T })` | `intro` | "RELEASE NOTES", wordmark, version odometer (`pre` stays, `from` rolls out, `to` rolls in), subtitle. |
| `makeOutro({ version, T })` | `outro` | Logo, wordmark glint, "Version X is out.", `spacetime version upgrade` + releases URL. |
| `makeCards({ start, end, kick, parts, list, toOutro })` | `cards`, `ex-feature-cards-5` | A card screen: 1–5 cards `{ k, title: [2 lines], d: [2 lines] }`; text shrinks to fit. `toOutro` collapses into the outro instead of sliding out. |

## Layout (`kit/layout.js`)

| Component | Gallery | What it does |
|---|---|---|
| `header(ctx, u, kicker, parts, { align })` | `header` | Kicker + headline; one part `fill: 'brand'`. Shrinks to fit 1640 px. |
| `slide(ctx, u, len)` | — | Scene enters from the right, exits to the left. |
| `enter(ctx, u, u0, dy, d)` | — | Fade + lift in at `u0`. |
| `panel(ctx, x, y, w, h, title)` | `layout-panel-code-pill-caption` | Window with traffic lights and a title. |
| `code(ctx, parts, x, y, size, alpha)` + `SYN` | same | A line of syntax-highlighted code: `[[text, SYN.kw], …]`. |
| `pill(ctx, x, y, label, o)` → width | same | Rounded label; `o.stroke` highlights it. |
| `caption(ctx, s, u, u0, y)` | same | One plain sentence under a scene. |
| `status(ctx, x, y, lt, t, run)` | — | Spinner that turns into a check after `run` s. |
| `fitText(ctx, s, x, y, maxW, o)` | — | Text shrunk to a width. |
| `bubble(ctx, s, x, y, align, a)` | `chat` | A chat bubble. |

## Components

| Component | Gallery | Used by | Sound cue |
|---|---|---|---|
| `agentChat(ctx, u, t, { x, y, w, h, title, chat, ys, toolRun, enterAt })`: user bubbles, tool-call pills (spinner → check), typed agent replies | `chat` | 2.7, 2.9, 2.10 MCP | `A.chatCues(start, chat, toolRun)` |
| `linkPulses(ctx, u, { x0, x1, y, calls })`: a line with data dots travelling at each call time | `chat` | 2.7, 2.9, 2.10 | (in `chatCues`) |
| `typedCommand(ctx, cmd, x, y, u, c0, c1, { size })`: "$ cmd" typed between c0 and c1 (c0 = null → whole) | `terminal` | 2.0, 2.2, 2.3, 2.4, 2.5, 2.7, 2.9 | `A.typing(a, b)` |
| `resultLine(ctx, s, x, y, a, { ok, size, gap, fill, weight })`: check or red cross + text | `terminal` | 2.0, 2.3, 2.5, 2.7 | `A.done(t)` / `A.fail(t)` |
| `planMarker(ctx, x, y, col, a)`: the migration plan's "▸" (the mono font lacks it) | `terminal` | 2.2, 2.7 | — |
| `laneFrame(ctx, x, y, w, h, { now, version, title, titleX, titleSize })`: one before/after lane | `compare` | 2.1, 2.2, 2.3, 2.5, 2.7, 2.8 | — |
| `beforeAfter(ctx, u, drawLane, { old, now, enterAt })`: places the old (dimmed) and new lanes | `compare` | same | — |
| `commandChip(ctx, cmd, cx, top, u, c0, c1, { stroke, size })`: centered "$ cmd" bar | `widgets` | 2.0, 2.4 | `A.typing` |
| `statusPill(ctx, right, top, state, t, label)`: connected / connection lost / reconnecting… | `widgets` | 2.6 | `A.fail` / `A.done` |

## Scene examples (copy and adapt)

| Gallery | Reel · function | Shows |
|---|---|---|
| `ex-mcp-local` | 2.9 `sMcp` | Agent chat + a server panel whose tools light up and databases/tables appear |
| `ex-mcp-maincloud` | 2.10 `sMcp` | Agent chat + a live hosted database: rows highlight on SQL, a row appears on a reducer call |
| `ex-mcp-endpoint` | 2.7 `sMcp` | Agent chat + endpoint URL, tool pills and a value that updates |
| `ex-plugins` | 2.9 `sPlugins` | Terminal with install commands per tool, then chips of what gets installed |
| `ex-unity-playmode` | 2.9 `sUnity` | Settings toggle + timeline lanes of Play sessions |
| `ex-lock` | 2.7 `sLock` | Terminal rows with ok/blocked results + a padlock that locks, shakes and opens |
| `ex-unique-migration` | 2.7 `sUniq` | One-line code change + before/after lanes with table rows and migration output |
| `ex-feature-cards-5` | 2.7 `dx` | A five-card screen |
| `ex-submodules` | 2.8 `sSub` | Code + a module box sliding into a slot of another module + namespaced names |
| `ex-tab-reconnect` | 2.8 `sTab` | Before/after lanes with mini browser windows and event lines |
| `ex-react-reconnect` | 2.6 `sRecon` | Chat app that drops and reconnects + a retry timeline with backoff |
| `ex-cpp-query` | 2.6 `sCpp` | Code typed line by line, build check, live view rows, language pills |
| `ex-procedures-flag` | 2.5 `sProc` | Config line struck out and removed, code panel, build check, per-language rows |
| `ex-solid-sync` | 2.5 `sSolid` | Terminal + code + two browser windows kept in sync + framework logo row |
| `ex-view-pk` | 2.5 `sViews` | Code chip with a highlighted part + before/after boards with client events |
| `ex-http-routes` | 2.4 `sHttp` | Code + curl + a route map with callers pointing at the module |
| `ex-templates-3` | 2.4 `sTmpl` | Command chip + three app mock cards (chat, game, transfers) |
| `ex-godot` | 2.3 `sGodot` | Editor mock + a small multiplayer game (two windows) |
| `ex-pipelining` | 2.3 `sPipe` | Request conveyor lanes: one at a time vs streamed, batched replies |
| `ex-remove-tables` | 2.2 `sDrop` | Numbered step chips + before/after lanes |
| `ex-safer-cli` | 2.2 `sCli` | Full-width terminal: list output, confirmation prompt, CI line |
| `ex-rust-browser` | 2.1 `sRust` | Cargo.toml + code + native and browser windows receiving the same rows |
| `ex-http-timeouts` | 2.1 `sHttp` | Timeline lanes with limit markers and a request bar |
| `ex-unreal-events` | 2.1 `sUnreal` | C++ code, protocol switch, game view with an event popping |
| `ex-event-tables` | 2.0 `sEvents` | Code + counters + three clients reacting to each event |
| `ex-await-reducers` | 2.0 `sAwait` | Code with a resolved and a rejected call + another user's view |
| `ex-spacetime-json` | 2.0 `sConfig` | Long commands collapsing into short ones + config file + dev terminal + app |
| `ex-logo-grid` | 2.0 `sTmpl` | Grid of framework logos + "already supported" row + command chip |

Logos available in `A.logos`: rust, react, unity, unreal, svelte, vue, nextjs, nodejs, bun, deno, angular, tanstack,
remix, nuxt, typescript, cpp, javascript, html5, csharp (files in `assets/logos/`). The SpacetimeDB mark is `drawMark()`
(in `lib.js`), the wordmark `A.wordmark`.

## Sound (`kit/audio.js`)

`createReleaseAudio({ DUR, T, seed })` returns the synth instruments plus: `intro(firstScene)`, `scene(start, next, o)`
(crash + groove + transition), `cards(start, next, n)`, `outro()`, `groove(a, b, o)`, `typing(a, b)`, `pop(t)`,
`done(t, i)`, `fail(t)`, `chatCues(start, chat, toolRun)`, `transition(next)`, `finish(path)`. Key sounds to the same
scene-local times the visuals use (export them from `scenes.js`).

## Checks

- `node --expose-gc check.js release-vX.Y`: layout and audio checks for one reel (run by `build-release.sh` before rendering).
- `node --expose-gc snapshot.js --save` / `--check`: any change to the kit must keep every reel byte-identical.
