# v2.4 video: decisions

Range: `v2.3.0 → v2.4.0`. v2.3.0 sits on a release branch, so the range is the master commits after its fork point
(27 commits). v2.3.0-hotfix1 (no GitHub release) only reverts #4884, also in this range. v2.4.1 belongs to the 2.5 video.
Length 36.5 s. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| HTTP handlers | Module authors can define their own HTTP routes (in beta), so webhooks, bots or `curl` can call the module directly. | NEW · HTTP HANDLERS · BETA / Modules can serve their own HTTP routes. |
| Templates | `spacetime init`/`dev` offer three new ready-to-run apps: an AI chat, a multiplayer Hangman game and a money-transfer demo. | NEW · STARTER TEMPLATES / Three new starter templates. |
| Cards | Reducers in Rust, C# and C++ modules run on their own thread without async overhead; the Blackholio sample game is complete in Godot and has a TypeScript version. | MODULES AND SAMPLES / Also in 2.4. |

## Sources
#4636, #5150, #5119, #5134, #5095, #5030, #5140.

## Wording choices
- HTTP handlers are labeled BETA (opt-in needed in Rust, C# and C++). The code is the TypeScript example from the docs. No built-in auth, custom domains or path parameters implied; `localhost:3000` because Maincloud availability at 2.4 can't be verified.
- LLM chat: the reply appears all at once (no streaming), "Bring your own API key" is on screen, and nothing suggests secure key storage.
- Reducer card: no speed number (a refactor with no published benchmark); TypeScript modules not included.

## Illustrative
The chat prompt and reply, the Hangman word, balances and transfer, the module name.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): Safari failing to decode compressed messages since 2.2.0 (#5144), half-written commitlog data after a crash (#5116), views after module updates (#5149).
- Apache 2.0 for the Rust module crates and the Unreal SDK (#5151): the messaging should be confirmed with the team first (the server stays BSL).
- JS energy accounting revert (#4927, billing), metrics, logging, docs, CI, version bumps.
