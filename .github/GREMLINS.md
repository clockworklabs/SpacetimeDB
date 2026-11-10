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

## Security Gremlin 🛡️

- **What:** Security reviewer (report-only)
- **GitHub account:** `clockwork-labs-bot`
- **Discord channel:** #security-gremlin (CL - SpacetimeDB)
- **Powered by:** [OpenClaw](https://github.com/openclaw/openclaw) + GPT-6.1

### What Security Gremlin does

- **Reviews code for security bugs** — on request, reads commits and PRs for problems with authentication, authorization, module sandboxing, untrusted input, and leaked secrets
- **Checks dependencies and CI** — looks for vulnerable dependencies and unsafe GitHub Actions workflows
- **Reports privately** — posts findings in #security-gremlin for the team to triage

### What Security Gremlin does NOT do

- Discuss vulnerabilities in public issues, PR comments, or commit messages
- Build or run code from PRs by people outside Clockwork Labs
- Scan or probe live systems
- Approve or merge PRs, or take any destructive action

### Contacting Security Gremlin

- Mention `@Security Gremlin App` in #security-gremlin on Discord

---

*To add a new gremlin, document it here.*
