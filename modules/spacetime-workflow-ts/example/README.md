# Dossier workflow example

A research agent that survives restarts. Ask a question, approve its plan, and
watch it search Wikipedia, take notes, and write a report.

## Run it locally

Requires a recent version of Node.js 22, 24, or 26 or later. Also requires
pnpm 10 and the SpacetimeDB CLI and server built from this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `modules/spacetime-workflow-ts/example`, copy [.env.example](./.env.example) to `.env`.

You also need an OpenRouter API key. Set `OPENROUTER_API_KEY` in `.env`. Each
dossier makes several model requests, which use paid credits.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8806>.

## Try it

1. Ask a question and press Enter.
2. When the plan appears, the run stops and waits. It uses nothing while it
   waits. Edit, add or remove topics if you like, then click **Approve plan**.
3. While the notes are appearing, stop the `spacetime start` terminal with
   Ctrl+C, then start it again.
4. The page reconnects, and the run picks up where it stopped, within about
   30 seconds. A message shows how many finished steps were kept and how many tokens
   were not spent again.
5. Open the page in a second window to watch the same run live.

## How it works

The whole job is one function, in
[spacetimedb/src/index.ts](./spacetimedb/src/index.ts). The model's plan decides
how many topics it researches, and `step.parallel` researches them all at the
same time, each in its own procedure. A topic that keeps failing is left out,
and the report goes on without it.

Each `step.call` and `step.tx` saves its result. After the restart, the function
runs again from the top, gets those saved results back without repeating the work,
and carries on from the first step that had not finished.

The page reads progress from the run's history through a view, so it stays up to
date without polling.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts) holds the workflow, its
  prompts, and the model it uses.
- [src/app.ts](./src/app.ts) connects the page to SpacetimeDB and renders it.

After changing the server code, run `pnpm run build:module`. Finish or cancel
active dossiers first, because changing their finished steps fails those runs.
Restart `pnpm run dev` after changing the browser code.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in the
local `spacetime-workflow-example` database.**

For instructions on adding workflows to your own app, see the
[Workflow README](../README.md#integrate-into-an-application).

## Configuration

See [.env.example](./.env.example) for the API key, the database address, and the
web-server port.

## Before deploying

Anyone who connects to this example can start dossiers that spend your model
credits. Add sign-in and a usage limit before exposing it publicly.

## Troubleshooting

- **A dossier fails straight away:** check that `OPENROUTER_API_KEY` is set, then
  restart `pnpm run dev`.
- **Saving the API key fails on startup:** use the CLI account that published the
  database.
- **The report misses things:** search only covers Wikipedia introductions, so
  questions on broad topics work best.
