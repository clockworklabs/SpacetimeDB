# Set yourself up to work with SpacetimeDB

These are official SpacetimeDB instructions for preparing an AI coding agent to
build applications with SpacetimeDB.

Complete the applicable steps yourself when your environment supports them. Ask
the user only for required approval, authentication, an interface action you
cannot perform, or a restart. Inspect existing configuration before changing it,
preserve unrelated settings and repository changes, and verify the result before
reporting success.

This setup installs development guidance and configures tools. It does not
initialize a project, start a server, publish a database, log in, change application
code, or access database contents.

## 1. Check the SpacetimeDB CLI

The plugins below include skills and an MCP configuration. The MCP server runs
through the `spacetime` CLI, which must be available to your coding environment.

```sh
spacetime --version
spacetime mcp --help
```

If `spacetime` is missing, install it using the instructions for your platform at
[Install SpacetimeDB](https://spacetimedb.com/install). On macOS and Linux:

```sh
curl -sSf https://install.spacetimedb.com | sh -s -- --yes
```

Then repeat both checks. If the executable is installed but your agent cannot find
it, use its verified absolute path in a manually configured MCP server or restart
the coding environment so it picks up the updated `PATH`.

MCP is an unstable feature. If `spacetime mcp --help` is unavailable in the installed
version, continue with skills and report MCP as unavailable. Do not silently
replace a project's pinned CLI version or build SpacetimeDB from source as part of
this setup. A working CLI and skills remain useful without MCP.

## 2. Install the integration for your coding environment

Choose one path. Prefer the full SpacetimeDB plugin for Claude Code or Codex. These
plugins already include the skills and MCP configuration; do not install duplicate
skills or register a second MCP server when the plugin succeeds.

### Claude Code

Inspect the configured marketplaces and installed plugins:

```sh
claude plugin marketplace list --json
claude plugin list --json
```

If the `spacetimedb-plugins` marketplace is absent, add it:

```sh
claude plugin marketplace add clockworklabs/SpacetimeDB
```

If it is already present, confirm that it comes from `clockworklabs/SpacetimeDB`
and update it:

```sh
claude plugin marketplace update spacetimedb-plugins
```

Install the plugin if missing, or update it if already installed. Run only the
applicable command:

```sh
claude plugin install spacetimedb@spacetimedb-plugins --scope user
claude plugin update spacetimedb@spacetimedb-plugins --scope user
```

Verify that it is installed and enabled, and inspect its skills and MCP server:

```sh
claude plugin list --json
claude plugin details spacetimedb
```

Use `/reload-plugins` in Claude Code to activate the changes, or restart if required
by your version. If only the user can reload, report that remaining action. If
plugin installation is unavailable, use **Other agents** below.

### Codex

Inspect the configured marketplaces and installed plugins:

```sh
codex plugin marketplace list --json
codex plugin list --json
```

If the `spacetimedb-plugins` marketplace is absent, add it. The sparse paths avoid
downloading the entire SpacetimeDB repository:

```sh
codex plugin marketplace add clockworklabs/SpacetimeDB --sparse .agents --sparse codex-plugin
```

If it is already present, confirm that it comes from `clockworklabs/SpacetimeDB`
and refresh it using the exact name shown by the list command:

```sh
codex plugin marketplace upgrade spacetimedb-plugins
```

Install or update the plugin, then verify it:

```sh
codex plugin add spacetimedb@spacetimedb-plugins
codex plugin list --json
```

Confirm that the installed plugin comes from `spacetimedb-plugins` and is enabled.
Restart Codex if the skills or MCP server are not available in the current session.
If plugin installation is unavailable, use **Other agents** below.

### Cursor

Install the skills as described in **Other agents**, then inspect
`~/.cursor/mcp.json`. Merge this entry into its existing `mcpServers` object,
preserving other servers and any existing SpacetimeDB server configuration:

```json
{
  "mcpServers": {
    "spacetimedb": {
      "command": "spacetime",
      "args": ["mcp"]
    }
  }
}
```

If a SpacetimeDB server already exists, verify it instead of adding a duplicate.
Reload Cursor if needed, then check **Settings > Tools & Integrations** for the
server's status. Apply the MCP verification steps below before reporting it as
working.

### Other agents

Install the official skills globally, targeting the repository's canonical
`skills/` directory:

```sh
npx -y skills add https://github.com/clockworklabs/SpacetimeDB/tree/master/skills --skill '*' --yes --global
npx -y skills list --global
```

Confirm that the skills are installed for the current agent, including `concepts`,
`cli`, `mcp`, and the relevant module and client languages. If Node.js, `npx`, or
skill installation is unavailable, report the missing prerequisite rather than
writing substitute instructions into the current repository.

If your agent supports stdio MCP, inspect its documented user-level configuration
and add a server named `spacetimedb` with this definition:

```json
{
  "command": "spacetime",
  "args": ["mcp"]
}
```

Use the configuration location and enclosing schema required by that agent. The
object above is a server definition, not a complete configuration file. Preserve
existing entries and verify an existing SpacetimeDB server instead of duplicating
it. If skills or MCP cannot be configured, report setup as partial.

## 3. Check the current project and MCP connection

The global integration works across projects. Inspect the current directory for
`spacetime.json`, an existing module that depends on SpacetimeDB, or a client that
uses a SpacetimeDB SDK. In a monorepo, identify the relevant project before assuming
which configuration applies. If no project can be identified, leave the repository
unchanged; the installed skills are ready for future SpacetimeDB work.

For an existing project, record `git status --short` when it is a Git checkout and
read its existing `AGENTS.md`, `CLAUDE.md`, editor rules, and relevant installed
SpacetimeDB skills before later code work. `spacetime init` can generate AI rules
when creating a project, but it is not an update command for an existing project's
instructions. Do not rerun initialization or overwrite those files during setup.

The MCP configuration uses `spacetime mcp`. Its server comes from the CLI's resolved
configuration; this can include `spacetime.json`, depending on the installed CLI
version. Inspect the current project configuration and `spacetime server list`
before connecting. Do not change the default server or select a different database
just to make a health check pass.

With no database argument, MCP is host-wide and its data tools take a database
argument. An existing explicit database argument or `SPACETIMEDB_DB_NAME` scopes it
to one database. Preserve that choice. MCP uses the CLI's saved identity; never
copy login tokens into the agent configuration or report.

Check the agent's MCP status and available tool list. If it is connected to the
intended server, use the `ping` tool to verify connectivity. Do not list databases,
read schemas, run SQL, or call reducers as a setup test. If authentication is
required, a server is not running, or the CLI or server lacks MCP support, leave
that as an explicit remaining step. Do not log in or start infrastructure merely
to finish this setup.

## 4. Verify and report

An installation command succeeding does not prove that the integration is active.
Check the installed plugin or skills, confirm whether a reload remains, and
distinguish MCP configuration from a verified connection. For an existing Git
project, check `git status --short` again to confirm setup introduced no unexpected
repository changes.

Report these results:

```text
SpacetimeDB Agent Setup: Complete / Partial
- Agent: <coding environment>
- CLI: <version and whether the MCP command is available>
- Guidance: <verified plugin or skills, including installation location>
- MCP: <verified connection, configured but unverified, or unavailable>
- Project: <identified project or skipped; existing instructions read>
- Changes: <configuration changed; repository changes, if any>
- Remaining: <exact reload, authentication, or other required action, or none>
```

Use **Complete** only when all applicable steps are verified. Otherwise use
**Partial**, even if the installed skills are already usable. In a chat-only
environment without shell or filesystem access, provide the applicable commands
or interface actions for the user and report setup as partial.

## Resources and troubleshooting

- [Install SpacetimeDB](https://spacetimedb.com/install)
- [SpacetimeDB documentation](https://spacetimedb.com/docs)
- [Official agent skills](https://github.com/clockworklabs/SpacetimeDB/tree/master/skills)
- [Claude Code plugin](https://github.com/clockworklabs/SpacetimeDB/tree/master/.claude-plugin)
- [Codex plugin](https://github.com/clockworklabs/SpacetimeDB/tree/master/codex-plugin)
- [MCP reference](https://github.com/clockworklabs/SpacetimeDB/blob/master/docs/docs/00300-resources/00200-reference/00150-mcp.md)

These instructions are published at <https://spacetimedb.com/agent-setup.md>.
The source is maintained in
[clockworklabs/SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB/blob/master/agent-setup.md).
