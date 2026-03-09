# GREMLINS.md — Who Lives in the Pipes

This file documents the automated agents and bots that operate on this repository.

## Clawd 🔧

- **What:** Anti-entropy gremlin / CI watchdog
- **GitHub account:** `clockwork-labs-bot`
- **Discord channel:** #gremlins (CL - SpacetimeDB)
- **Powered by:** [OpenClaw](https://github.com/openclaw/openclaw) + Claude

### What Clawd does

- **Monitors CI** — watches for failures, flaky tests, and regressions
- **Reviews PRs** — comments on obvious bugs (never approves)
- **Surfaces stale PRs** — finds approved PRs that just need a rebase
- **Documents testing** — creates and maintains `DEVELOP.md` files explaining CI and test infrastructure
- **Alerts the team** — posts findings in #gremlins, pings DevOps when something needs attention

### What Clawd does NOT do

- Approve or merge PRs
- Take any destructive action (delete branches, close PRs, force push)
- Modify production infrastructure

### Contacting Clawd

- Mention `@Openclaw` in #gremlins on Discord
- Tag `@clockwork-labs-bot` on GitHub PRs/issues

---

*To add a new gremlin, document it here.*
