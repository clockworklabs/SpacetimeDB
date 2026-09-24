# Stack Bench documentation

Use the root [README](../README.md) for the product summary and
[Getting started](../GETTING-STARTED.md) for a first run.

## Run Stack Bench

- [Appliance operation](../appliance/README.md): configure and run campaigns
- [Dashboard](../dashboard/README.md): web interface and run-setup interface
- [Execution jobs](execution-jobs.md): submit work, assign hosts, and integrate a task queue
- [Credential profiles](credential-profiles.md): select and attribute account/API-key use
- [Recovery](../appliance/RECOVERY.md): interrupted runs and retained resources
- [Release](../appliance/RELEASE.md): assemble and verify a release

## Understand the method

- [Prompting method](prompting.md): prompt inputs, guidance profiles,
  specification treatments, and repair requests
- [Grader](../grader/README.md): outcomes, scoring, and check validation
- [Grading coverage](grading-coverage.md): what checks observe, their limits,
  and failure review
- [Check categories](check-categories.md): categories and counting units
- [Study method](study-method.md): collecting and reporting a defensible comparison
- [Reference apps](../reference-apps/README.md): grading fixtures and qualification

## Understand the system

- [System design](system-design.md): ownership, terms, data flow, and operator loop
- [Appliance design](../appliance/DESIGN.md): security and container boundaries

## Develop and author

- [Development](development.md): local dependencies, source checks, and agent adapters
- [Authoring](authoring.md): add features, checks, prompts, and rules through their existing owners
- [Ecommerce composition](../tracks/ecommerce/composition/README.md): packs,
  recipes, calibration, and specification treatment
- [Ecommerce levels](../tracks/ecommerce/LEVELS.md): sequential levels and dependency depths
- [Chat levels](../tracks/chat/LEVELS.md): current chat scope

## Visuals

- [Dependency graph](dependency-graph.html): generated ecommerce feature graph
- [Presentation](stack-bench.html): product presentation and illustrative checks

These illustrations do not carry qualification status; use the current
definition and evidence. `dependency-graph.html` is generated from the current
graph with `npm run graph`. Do not edit it by hand.

Markdown files under `backends/`, `conditions/`, `tracks/*/prompts`, and
`tracks/*/contracts` are executable benchmark inputs. They stay with their
owners and are not general documentation.
