# Stack Bench documentation

Use the root [README](../README.md) for the product summary.

## Run Stack Bench

- [Development](development.md): local dependencies and source checks
- [Appliance operation](../appliance/README.md): configure and run campaigns
- [Dashboard](../dashboard/README.md): optional web interface
- [Recovery](../appliance/RECOVERY.md): interrupted runs and retained resources
- [Release](../appliance/RELEASE.md): assemble and verify a release

## Understand the system

- [System design](system-design.md): ownership, data flow, and operator loop
- [Appliance design](../appliance/DESIGN.md): security and container boundaries
- [Grader](../grader/README.md): scoring, evidence, and grader validation
- [Reference apps](../reference-apps/README.md): known-good grading fixtures

## Define benchmark work

- [Ecommerce composition](../tracks/ecommerce/composition/README.md): packs,
  recipes, calibration, and specification treatment
- [Ecommerce levels](../tracks/ecommerce/LEVELS.md): cumulative and dependency
  progression
- [Chat levels](../tracks/chat/LEVELS.md): current chat scope

## Visuals

- [Dependency graph](dependency-graph.html): generated ecommerce feature graph
- [Technical guide](technical-guide.html): current run path
- [Presentation](stack-bench.html): product presentation

`dependency-graph.html` is generated from the versioned graph with
`npm run graph`. Do not edit it by hand.

Markdown files under `backends/`, `conditions/`, `tracks/*/prompts`, and
`tracks/*/contracts` are executable benchmark inputs. They stay with their
owners and are not general documentation.
