# llm-apps

`cargo llm-apps` runs the two benchmarks that measure how well AI models build apps on SpacetimeDB.

- `cargo llm-apps oneshot ...` drives the one-shot benchmark in [`tools/llm-oneshot`](../llm-oneshot). A model builds an app from one prompt in Cursor; these commands publish graded apps and summarize the grades into `docs/llms/`.
- `cargo llm-apps sequential ...` drives the sequential-upgrade benchmark in [`tools/llm-sequential-upgrade`](../llm-sequential-upgrade). Headless Claude Code builds a chat app and upgrades it one level at a time, with cost captured through OpenTelemetry.

Run `cargo llm-apps <benchmark> --help` for the commands. The benchmark directories hold the prompts, specs and process docs; see their READMEs.

Generated apps and run output never live in this repository. They are written to gitignored directories and published with the `publish` commands to [`spacetimedb-llm-oneshot-results`](https://github.com/clockworklabs/spacetimedb-llm-oneshot-results) and [`spacetimedb-llm-sequential-upgrade-results`](https://github.com/clockworklabs/spacetimedb-llm-sequential-upgrade-results). Publishing leaves out dependencies, build output, secrets and telemetry that contains personal data.
