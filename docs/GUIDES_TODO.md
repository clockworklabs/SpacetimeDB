# Guides to write

Short guides, each focused on one precise topic. Check off each one once it's written.

Each guide has a stub page under `docs/00250-how-to/` (one folder per section below, numbered from `01000`),
wired into the sidebar in `sidebars.ts` under the "How To Guides" header. Once a guide is written, also list it
on the section's index page, `docs/00250-how-to/00000-index.md`. The stubs are regular published pages, so don't
merge this branch to `master` until every guide is written, or remove the unfinished stubs first.

Each guide will also get a short-format video (done later, once the written guides exist).

## Games

- [x] How to make a top 100 leaderboard
- [x] Run a fixed-rate game tick
- [x] Build a matchmaking queue
- [x] Skill-based matchmaking with OpenSkill
- [x] Make a turn-based game with turn timeouts
- [x] Only sync what's near the player
- [x] Hide information from players
- [x] Reset a leaderboard each season

## Social

- [x] Keep usernames unique and block offensive ones
- [x] Build a friends or follow list
- [x] Chat rooms and private DMs
- [x] Typing indicators and other short-lived signals
- [x] Unread counts and notifications

## Common app patterns

- [x] Paginate or infinite-scroll a large table
- [x] Admin and moderator roles
- [x] Send an email from your module
- [x] Call an external API from your module
- [x] Call an AI model from your module
- [x] Call an AI model from a sidecar
- [ ] Seed initial data on first publish
