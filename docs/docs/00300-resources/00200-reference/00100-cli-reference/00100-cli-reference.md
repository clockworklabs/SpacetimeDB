---
title: CLI Reference
slug: /cli-reference
---

# Command-Line Help for `spacetime`

This document contains the help content for the `spacetime` command-line program.

**Command Overview:**

* [`spacetime`↴](#spacetime)
* [`spacetime publish`↴](#spacetime-publish)
* [`spacetime delete`↴](#spacetime-delete)
* [`spacetime logs`↴](#spacetime-logs)
* [`spacetime call`↴](#spacetime-call)
* [`spacetime describe`↴](#spacetime-describe)
* [`spacetime dev`↴](#spacetime-dev)
* [`spacetime energy`↴](#spacetime-energy)
* [`spacetime energy balance`↴](#spacetime-energy-balance)
* [`spacetime sql`↴](#spacetime-sql)
* [`spacetime rename`↴](#spacetime-rename)
* [`spacetime generate`↴](#spacetime-generate)
* [`spacetime list`↴](#spacetime-list)
* [`spacetime login`↴](#spacetime-login)
* [`spacetime login show`↴](#spacetime-login-show)
* [`spacetime logout`↴](#spacetime-logout)
* [`spacetime init`↴](#spacetime-init)
* [`spacetime build`↴](#spacetime-build)
* [`spacetime server`↴](#spacetime-server)
* [`spacetime server list`↴](#spacetime-server-list)
* [`spacetime server set-default`↴](#spacetime-server-set-default)
* [`spacetime server add`↴](#spacetime-server-add)
* [`spacetime server remove`↴](#spacetime-server-remove)
* [`spacetime server fingerprint`↴](#spacetime-server-fingerprint)
* [`spacetime server ping`↴](#spacetime-server-ping)
* [`spacetime server edit`↴](#spacetime-server-edit)
* [`spacetime server clear`↴](#spacetime-server-clear)
* [`spacetime subscribe`↴](#spacetime-subscribe)
* [`spacetime start`↴](#spacetime-start)
* [`spacetime version`↴](#spacetime-version)

## `spacetime`

**Usage:** `spacetime [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `publish` — Create and update a SpacetimeDB database
* `delete` — Deletes a SpacetimeDB database
* `logs` — Prints logs from a SpacetimeDB database
* `call` — Invokes a function (reducer or procedure) in a database. WARNING: This command is UNSTABLE and subject to breaking changes.
* `describe` — Describe the structure of a database or entities within it. WARNING: This command is UNSTABLE and subject to breaking changes.
* `dev` — Start development mode with auto-regenerate client module bindings, auto-rebuild, and auto-publish on file changes.
* `energy` — Invokes commands related to database budgets. WARNING: This command is UNSTABLE and subject to breaking changes.
* `sql` — Runs a SQL query on the database. WARNING: This command is UNSTABLE and subject to breaking changes.
* `rename` — Rename a database
* `generate` — Generate client files for a spacetime module.
* `list` — Lists the databases attached to an identity. WARNING: This command is UNSTABLE and subject to breaking changes.
* `login` — Manage your login to the SpacetimeDB CLI
* `logout` — 
* `init` — Initializes a new spacetime project. WARNING: This command is UNSTABLE and subject to breaking changes.
* `build` — Builds a spacetime module.
* `server` — Manage the connection to the SpacetimeDB server. WARNING: This command is UNSTABLE and subject to breaking changes.
* `subscribe` — Subscribe to SQL queries on the database. WARNING: This command is UNSTABLE and subject to breaking changes.
* `start` — Start a local SpacetimeDB instance
* `version` — Manage installed spacetime versions

###### **Options:**

* `--root-dir <ROOT_DIR>` — The root directory to store all spacetime files in.
* `--config-path <CONFIG_PATH>` — The path to the cli.toml config file



## `spacetime publish`

Create and update a SpacetimeDB database

**Usage:** `spacetime publish [OPTIONS] [name|identity]`

Run `spacetime help publish` for more detailed information.

###### **Arguments:**

* `<NAME|IDENTITY>` — A valid domain or identity for this database.

   Database names must match the regex `/^[a-z0-9]+(-[a-z0-9]+)*$/`,
   i.e. only lowercase ASCII letters and numbers, separated by dashes.

###### **Options:**

* `-c`, `--delete-data <CLEAR-DATABASE>` — When publishing to an existing database identity, first DESTROY all data associated with the module. With 'on-conflict': only when breaking schema changes occur.

  Possible values: `always`, `on-conflict`, `never`

* `--build-options <BUILD_OPTIONS>` — Options to pass to the build command, for example --build-options='--lint-dir='

  Default value: ``
* `-p`, `--project-path <PROJECT_PATH>` — The system path (absolute or relative) to the module project

  Default value: `.`
* `-b`, `--bin-path <WASM_FILE>` — The system path (absolute or relative) to the compiled wasm binary we should publish, instead of building the project.
* `-j`, `--js-path <JS_FILE>` — UNSTABLE: The system path (absolute or relative) to the javascript file we should publish, instead of building the project.
* `--break-clients` — Allow breaking changes when publishing to an existing database identity. This will force publish even if it will break existing clients, but will NOT force publish if it would cause deletion of any data in the database. See --yes and --delete-data for details.
* `--anonymous` — Perform this action with an anonymous identity
* `--parent <PARENT>` — A valid domain or identity of an existing database that should be the parent of this database.

   If a parent is given, the new database inherits the team permissions from the parent.
   A parent can only be set when a database is created, not when it is updated.
* `-s`, `--server <SERVER>` — The nickname, domain name or URL of the server to host the database.
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime delete`

Deletes a SpacetimeDB database

**Usage:** `spacetime delete [OPTIONS] <database>`

Run `spacetime help delete` for more detailed information.


###### **Arguments:**

* `<DATABASE>` — The name or identity of the database to delete

###### **Options:**

* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server hosting the database
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime logs`

Prints logs from a SpacetimeDB database

**Usage:** `spacetime logs [OPTIONS] <database>`

Run `spacetime help logs` for more detailed information.


###### **Arguments:**

* `<DATABASE>` — The name or identity of the database to print logs from

###### **Options:**

* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server hosting the database
* `-n`, `--num-lines <NUM_LINES>` — The number of lines to print from the start of the log of this database. If no num lines is provided, all lines will be returned.
* `-f`, `--follow` — A flag that causes logs to not stop when end of the log file is reached, but rather to wait for additional data to be appended to the input.
* `--format <FORMAT>` — Output format for the logs

  Default value: `text`

  Possible values: `text`, `json`

* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime call`

Invokes a function (reducer or procedure) in a database. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime call [OPTIONS] <database> <function_name> [arguments]...`

Run `spacetime help call` for more detailed information.


###### **Arguments:**

* `<DATABASE>` — The database name or identity to use to invoke the call
* `<FUNCTION_NAME>` — The name of the function to call
* `<ARGUMENTS>` — arguments formatted as JSON

###### **Options:**

* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server hosting the database
* `--anonymous` — Perform this action with an anonymous identity
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime describe`

Describe the structure of a database or entities within it. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime describe [OPTIONS] --json <database> [entity_type] [entity_name]`

Run `spacetime help describe` for more detailed information.


###### **Arguments:**

* `<DATABASE>` — The name or identity of the database to describe
* `<ENTITY_TYPE>` — Whether to describe a reducer or table

  Possible values: `reducer`, `table`

* `<ENTITY_NAME>` — The name of the entity to describe

###### **Options:**

* `--json` — Output the schema in JSON format. Currently required; in the future, omitting this will give human-readable output.
* `--anonymous` — Perform this action with an anonymous identity
* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server hosting the database
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime dev`

Start development mode with auto-regenerate client module bindings, auto-rebuild, and auto-publish on file changes.

**Usage:** `spacetime dev [OPTIONS] [database]`

###### **Arguments:**

* `<DATABASE>` — The database name/identity to publish to (optional, will prompt if not provided)

###### **Options:**

* `--project-path <PROJECT-PATH>` — The path to the project directory

  Default value: `.`
* `--module-bindings-path <MODULE-BINDINGS-PATH>` — The path to the module bindings directory relative to the project directory, defaults to `<project-path>/src/module_bindings`

  Default value: `src/module_bindings`
* `--module-project-path <MODULE-PROJECT-PATH>` — The path to the SpacetimeDB server module project relative to the project directory, defaults to `<project-path>/spacetimedb`

  Default value: `spacetimedb`
* `--client-lang <CLIENT-LANG>` — The programming language for the generated client module bindings (e.g., typescript, csharp, python). If not specified, it will be detected from the project.

  Possible values: `csharp`, `typescript`, `rust`, `unrealcpp`

* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server to publish to
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).
* `-c`, `--delete-data <CLEAR-DATABASE>` — When publishing to an existing database identity, first DESTROY all data associated with the module. With 'on-conflict': only when breaking schema changes occur.

  Possible values: `always`, `on-conflict`, `never`

* `-t`, `--template <TEMPLATE>` — Template ID or GitHub repository (owner/repo or URL) for project initialization



## `spacetime energy`

Invokes commands related to database budgets. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime energy
       energy <COMMAND>`

###### **Subcommands:**

* `balance` — Show current energy balance for an identity



## `spacetime energy balance`

Show current energy balance for an identity

**Usage:** `spacetime energy balance [OPTIONS]`

###### **Options:**

* `-i`, `--identity <IDENTITY>` — The identity to check the balance for. If no identity is provided, the default one will be used.
* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server from which to request balance information
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime sql`

Runs a SQL query on the database. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime sql [OPTIONS] <database> <query>`

###### **Arguments:**

* `<DATABASE>` — The name or identity of the database you would like to query
* `<QUERY>` — The SQL query to execute

###### **Options:**

* `--interactive` — Instead of using a query, run an interactive command prompt for `SQL` expressions
* `--confirmed` — Instruct the server to deliver only updates of confirmed transactions
* `--anonymous` — Perform this action with an anonymous identity
* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server hosting the database
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime rename`

Rename a database

**Usage:** `spacetime rename [OPTIONS] --to <new-name> <database-identity>`

Run `spacetime rename --help` for more detailed information.


###### **Arguments:**

* `<DATABASE-IDENTITY>` — The database identity to rename

###### **Options:**

* `--to <NEW-NAME>` — The new name you would like to assign
* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server on which to set the name
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime generate`

Generate client files for a spacetime module.

**Usage:** `spacetime spacetime generate --lang <LANG> --out-dir <DIR> [--project-path <DIR> | --bin-path <PATH> | --module-name <MODULE_NAME> | --uproject-dir <DIR>]`

Run `spacetime help publish` for more detailed information.

###### **Options:**

* `-b`, `--bin-path <WASM_FILE>` — The system path (absolute or relative) to the compiled wasm binary we should inspect
* `-j`, `--js-path <JS_FILE>` — The system path (absolute or relative) to the bundled javascript file we should inspect
* `-p`, `--project-path <PROJECT_PATH>` — The system path (absolute or relative) to the project you would like to inspect

  Default value: `.`
* `-o`, `--out-dir <OUT_DIR>` — The system path (absolute or relative) to the generate output directory
* `--uproject-dir <UPROJECT_DIR>` — Path to the Unreal project directory, replaces --out-dir for Unreal generation (only used with --lang unrealcpp)
* `--namespace <NAMESPACE>` — The namespace that should be used

  Default value: `SpacetimeDB.Types`
* `--module-name <MODULE_NAME>` — The module name that should be used for DLL export macros (required for lang unrealcpp)
* `-l`, `--lang <LANG>` — The language to generate

  Possible values: `csharp`, `typescript`, `rust`, `unrealcpp`

* `--build-options <BUILD_OPTIONS>` — Options to pass to the build command, for example --build-options='--lint-dir='

  Default value: ``
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime list`

Lists the databases attached to an identity. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime list [OPTIONS]`

###### **Options:**

* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server from which to list databases
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime login`

Manage your login to the SpacetimeDB CLI

**Usage:** `spacetime login [OPTIONS]
       login <COMMAND>`

###### **Subcommands:**

* `show` — Show the current login info

###### **Options:**

* `--auth-host <AUTH-HOST>` — Fetch login token from a different host

  Default value: `https://spacetimedb.com`
* `--server-issued-login <SERVER>` — Log in to a SpacetimeDB server directly, without going through a global auth server
* `--token <SPACETIMEDB-TOKEN>` — Bypass the login flow and use a login token directly



## `spacetime login show`

Show the current login info

**Usage:** `spacetime login show [OPTIONS]`

###### **Options:**

* `--token` — Also show the auth token



## `spacetime logout`

**Usage:** `spacetime logout [OPTIONS]`

###### **Options:**

* `--auth-host <AUTH-HOST>` — Log out from a custom auth server

  Default value: `https://spacetimedb.com`



## `spacetime init`

Initializes a new spacetime project. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime init [OPTIONS] [PROJECT_NAME]`

###### **Arguments:**

* `<PROJECT_NAME>` — Project name

###### **Options:**

* `--project-path <PATH>` — Directory where the project will be created (defaults to `./<PROJECT_NAME>`)
* `--server-only` — Initialize server only from the template (no client)
* `--lang <LANG>` — Server language: rust, csharp, typescript (it can only be used when --template is not specified)
* `-t`, `--template <TEMPLATE>` — Template ID or GitHub repository (owner/repo or URL)
* `--local` — Use local deployment instead of Maincloud
* `--non-interactive` — Run in non-interactive mode



## `spacetime build`

Builds a spacetime module.

**Usage:** `spacetime build [OPTIONS]`

###### **Options:**

* `-p`, `--project-path <PROJECT_PATH>` — The system path (absolute or relative) to the project you would like to build

  Default value: `.`
* `--lint-dir <LINT_DIR>` — The directory to lint for nonfunctional print statements. If set to the empty string, skips linting.

  Default value: `src`
* `-d`, `--debug` — Builds the module using debug instead of release (intended to speed up local iteration, not recommended for CI)



## `spacetime server`

Manage the connection to the SpacetimeDB server. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime server
       server <COMMAND>`

###### **Subcommands:**

* `list` — List stored server configurations
* `set-default` — Set the default server for future operations
* `add` — Add a new server configuration
* `remove` — Remove a saved server configuration
* `fingerprint` — Show or update a saved server's fingerprint
* `ping` — Checks to see if a SpacetimeDB host is online
* `edit` — Update a saved server's nickname, host name or protocol
* `clear` — Deletes all data from all local databases



## `spacetime server list`

List stored server configurations

**Usage:** `spacetime server list`



## `spacetime server set-default`

Set the default server for future operations

**Usage:** `spacetime server set-default <server>`

###### **Arguments:**

* `<SERVER>` — The nickname, host name or URL of the new default server



## `spacetime server add`

Add a new server configuration

**Usage:** `spacetime server add [OPTIONS] --url <url> <name>`

###### **Arguments:**

* `<NAME>` — Nickname for this server

###### **Options:**

* `--url <URL>` — The URL of the server to add
* `-d`, `--default` — Make the new server the default server for future operations
* `--no-fingerprint` — Skip fingerprinting the server



## `spacetime server remove`

Remove a saved server configuration

**Usage:** `spacetime server remove [OPTIONS] <server>`

###### **Arguments:**

* `<SERVER>` — The nickname, host name or URL of the server to remove

###### **Options:**

* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime server fingerprint`

Show or update a saved server's fingerprint

**Usage:** `spacetime server fingerprint [OPTIONS] <server>`

###### **Arguments:**

* `<SERVER>` — The nickname, host name or URL of the server

###### **Options:**

* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime server ping`

Checks to see if a SpacetimeDB host is online

**Usage:** `spacetime server ping <server>`

###### **Arguments:**

* `<SERVER>` — The nickname, host name or URL of the server to ping



## `spacetime server edit`

Update a saved server's nickname, host name or protocol

**Usage:** `spacetime server edit [OPTIONS] <server>`

###### **Arguments:**

* `<SERVER>` — The nickname, host name or URL of the server

###### **Options:**

* `--new-name <NICKNAME>` — A new nickname to assign the server configuration
* `--url <URL>` — A new URL to assign the server configuration
* `--no-fingerprint` — Skip fingerprinting the server
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime server clear`

Deletes all data from all local databases

**Usage:** `spacetime server clear [OPTIONS]`

###### **Options:**

* `--data-dir <DATA_DIR>` — The path to the server data directory to clear [default: that of the selected spacetime instance]
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).



## `spacetime subscribe`

Subscribe to SQL queries on the database. WARNING: This command is UNSTABLE and subject to breaking changes.

**Usage:** `spacetime subscribe [OPTIONS] <database> <query>...`

###### **Arguments:**

* `<DATABASE>` — The name or identity of the database you would like to query
* `<QUERY>` — The SQL query to execute

###### **Options:**

* `-n`, `--num-updates <NUM-UPDATES>` — The number of subscription updates to receive before exiting
* `-t`, `--timeout <TIMEOUT>` — The timeout, in seconds, after which to disconnect and stop receiving subscription messages. If `-n` is specified, it will stop after whichever
                     one comes first.
* `--print-initial-update` — Print the initial update for the queries.
* `--confirmed` — Instruct the server to deliver only updates of confirmed transactions
* `--anonymous` — Perform this action with an anonymous identity
* `-y`, `--yes` — Run non-interactively wherever possible. This will answer "yes" to almost all prompts, but will sometimes answer "no" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).
* `-s`, `--server <SERVER>` — The nickname, host name or URL of the server hosting the database



## `spacetime start`

Start a local SpacetimeDB instance

Run `spacetime start --help` to see all options.

**Usage:** `spacetime start [OPTIONS] [args]...`

###### **Arguments:**

* `<ARGS>` — The args to pass to `spacetimedb-{edition} start`

###### **Options:**

* `--edition <EDITION>` — The edition of SpacetimeDB to start up

  Default value: `standalone`

  Possible values: `standalone`, `cloud`




## `spacetime version`

Manage installed spacetime versions

Run `spacetime version --help` to see all options.

**Usage:** `spacetime version [ARGS]...`

###### **Arguments:**

* `<ARGS>` — The args to pass to spacetimedb-update



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

