# Release video prompt

The prompt for generating one "what's new" video. Replace `X.Y` with the minor version (e.g. `2.11`).
It runs unattended, so it never asks questions: it decides and documents.

---

Make the promotional "what's new" video for SpacetimeDB X.Y, in the style of the showreel in `tools/showreel`
(read `tools/showreel/README.md` first, especially "Release videos").

**Range.** Cover everything a user can notice between the first published GitHub release of the previous minor and the
first published GitHub release of X.Y (`v<prev>.0 → vX.Y.0`). Patch releases never get their own video; their changes
roll into the next one. Use the published releases (`gh release list`), not just git tag names: a tag can be an
unreleased branch tip or a bare version-bump commit, and patch tags can sit on release branches. State the exact range
you used.

**Research.** Read every release note in the range and every PR in `git log <from>..<to>` (release notes miss things).
Check every claim against the PR diff and the code at the tags. Release notes can overstate, describe reverted work, or
present older features as new. For each candidate, check whether it already existed before the range, and whether a
fix repairs a bug introduced inside the same range (then it isn't news). Write the findings to
`release-vX.Y/RESEARCH.md`.

**Choose content.**
- It's a promo, not a changelog. Keep only what users can notice. Leave out internal refactors, CI, metrics, logging,
  tests and docs-only changes.
- Don't advertise embarrassing fixes: nothing about fixes of data loss or unreachable data, durability or security
  holes, crashes, silently wrong results, or regressions from a recent release. Lifted limits (longer timeouts, new
  platforms, faster recovery) are fine.
- Pick the strongest 1–4 features as full scenes (~8–9 s each) and at most one or two card screens (1–5 cards each)
  for smaller features.
- There is no target length. A thin release makes a short video; never pad.

**Script before visuals.** For every scene and card, first write one plain sentence: what shipped, who it's for, and
what they can now do. Then derive:
- the kicker, which names the feature;
- the headline, which states the new capability in plain words. Test: someone who reads only the kicker and headline
  can tell a colleague what's new. No clever taglines ("Your agent, meet your database" fails; "Connect your coding
  agent to your database" passes).
- the caption and visuals, which show that capability.

Claims must be exact: soften wording rather than overclaim, attribute performance numbers to benchmarks (or leave
them out), and don't call something new if it existed before.

**Document.** Write `release-vX.Y/DECISIONS.md`:
- the range;
- the Script table (scene, one-sentence script, kicker / headline);
- sources;
- wording choices;
- illustrative data (chat lines, names, timings);
- what was left out and why;
- open doubts.

**Build.** First read `tools/showreel/kit/CATALOG.md` (every component and every scene built so far) and render the
gallery (`node --expose-gc kit/gallery.js`) to see them. Write `release-vX.Y/scenes.js` and `audio.js` using the shared parts (`release.js`: `header`, `makeIntro`,
`makeCards`, `makeOutro`, `makeReel`; `release-audio.js`). Reuse existing scene types from other reels where they fit,
but the feature comes first: when no existing scene shows it well, write a custom scene for it (the 2.9 MCP scene, an agent
chat beside a live database panel, is the kind of bespoke scene that sells a feature). Note in `DECISIONS.md` which scenes
are new, so good ones can be added to the library.
Follow the brand rules in the README:
- flat `#0B1114` background, no glows;
- Inter and Source Code Pro;
- the brand gradient on one element per screen;
- green as the only accent, red only for errors;
- headlines stay on screen 2.5–4.5 s after they finish appearing.

Run `node --expose-gc check.js release-vX.Y` and fix every error and warning it reports (missing glyphs, text off the
canvas or crossing a box edge, overlapping text, shrunk headlines, short scenes, a NaN soundtrack); explain in
`DECISIONS.md` any warning you keep on purpose. Render stills (`npm run release:stills -- release-vX.Y <times> --samples 1`),
check them with contact sheets (`./sheet.sh`), and fix crowding and timing the checker can't see. Then build with `./build-release.sh X.Y`: one full-quality
render, no compressed copies. Keep memory bounded.

**Shared code.** Existing reels must never change. Add new components to `kit/` rather than editing existing ones. If you
do need to edit anything in `kit/`, `lib.js`, `ui.js` or `synth.js`: run `node --expose-gc snapshot.js --save` on the
clean checkout before editing, then `node --expose-gc snapshot.js --check` after; every reel must stay identical. A
genuinely useful new scene goes into `kit/CATALOG.md` (and the gallery).

**Deliver.** `release-vX.Y/spacetimedb-vX.Y.mp4` plus `DECISIONS.md`.
