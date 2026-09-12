# Stack Bench documentation

Use the root [README](../README.md) for the product summary.

## Run Stack Bench

- [Development](development.md): local dependencies and source checks
- [Appliance operation](../appliance/README.md): configure and run campaigns
- [Execution jobs](execution-jobs.md): submit work, assign hosts, and integrate a task queue
- [Credential profiles](credential-profiles.md): select and attribute account/API-key use
- [Dashboard](../dashboard/README.md): optional web interface
- [Recovery](../appliance/RECOVERY.md): interrupted runs and retained resources
- [Release](../appliance/RELEASE.md): assemble and verify a release

## Understand the system

- [System design](system-design.md): ownership, data flow, and operator loop
- [Prompting method](prompting.md): prompt inputs, specification treatments,
  stack guidance, and repair examples
- [Appliance design](../appliance/DESIGN.md): security and container boundaries
- [Grader](../grader/README.md): scoring, evidence, and grader validation
- [Reference apps](../reference-apps/README.md): grading fixtures and qualification requirements

## Define benchmark work

- [Authoring](authoring.md): add features, checks, prompts, and rules through their existing owners
- [Grading coverage](grading-coverage.md): current qualification gaps and check justifications
- [L4–L6 preparation](l4-l6-readiness.md): later-depth probe gaps and proposed production workloads
- [Research roadmap](research-roadmap.md): staged data collection, parallel runs,
  comparison methods, and the research evidence pack
- [Ecommerce composition](../tracks/ecommerce/composition/README.md): packs,
  recipes, calibration, and specification treatment
- [Ecommerce levels](../tracks/ecommerce/LEVELS.md): cumulative and dependency
  progression
- [Chat levels](../tracks/chat/LEVELS.md): current chat scope

## Visuals

- [Dependency graph](dependency-graph.html): generated ecommerce feature graph
- [Technical guide](technical-guide.html): current run path
- [Presentation](stack-bench.html): product presentation and illustrative checks
- [How it works](how-it-works.html): isometric system map with a guided tour of a campaign and its qualification

Qualification status belongs to the current definition and evidence, not these illustrations.
Use [Grading coverage](grading-coverage.md) for limits. A paused run requires its live
controller; controller-restart recovery of the pause is not supported.

`dependency-graph.html` is generated from the current graph with
`npm run graph`. Do not edit it by hand.

Markdown files under `backends/`, `conditions/`, `tracks/*/prompts`, and
`tracks/*/contracts` are executable benchmark inputs. They stay with their
owners and are not general documentation.
