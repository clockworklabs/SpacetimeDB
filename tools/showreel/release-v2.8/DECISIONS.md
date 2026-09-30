# v2.8 video: decisions

Range: published 2.7.0 (`v2.7.0-hotfix3`) → `v2.8.0`, 35 commits: v2.7.1 plus 2.8.0. The literal `v2.7.0` git tag is only
the version-bump commit. Length 29.5 s: two features, no cards. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| Submodules | TypeScript module authors can mount another module (e.g. an auth library) under a namespace; its tables, reducers and views come along as `myauth.users`, `myauth.verify_token`. | NEW · TYPESCRIPT SUBMODULES / TypeScript modules can mount other modules. |
| Reconnect on return | React, Svelte and Solid web apps reconnect as soon as the user comes back to the tab, wakes the laptop or gets the network back (2.7.1). | REACT, SVELTE AND SOLID APPS · 2.7.1 / Web apps reconnect when you return to the tab. |

## Sources
#5486 (submodules; docs at v2.8.0); #5525 (resume listeners in the connection manager).

## Wording choices
- Submodules: "TypeScript modules · needs a 2.8 server" on screen; no Rust/C#/C++ claim. The separator is `.` (the 2.8.0 docs showed `/`, fixed in 2.8.3). No `spacetime call` with arguments (format not verified).
- Reconnect: auto-reconnect already existed; what's new is reconnecting immediately on return. The 2.7 lane says it "waits for its next scheduled retry" (a limitation), not that it was broken. No identity claim.

## Checker warnings kept
- `check.js` warns that "tab in the background" crosses a box edge: it's an overlay drawn over the dimmed rows while the tab is hidden. Kept on purpose.

## Illustrative
The `auth_lib` library and its tables, the board values, the lane timings.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): TS composite-index range scans returning wrong rows (#5479), the sequence-migration precheck (#5561), partial scheduler drift fix (#5574), V10 schema dropping column defaults (#5508), init metrics-task leak (#5601).
- "Enable HTTP/2" and "Cache CORS preflight" (2.7.1 notes): no matching change in this repo. Scheduled-function delay warning (#5592): operator-only. Quieter logs, the Convex migration guide (docs), refactors, CI, tests, version bumps.
