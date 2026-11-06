---
name: run-campaign
description: Start, watch, stop, and export a StackBench campaign on the Docker appliance
metadata:
  author: clockworklabs
  role: operator
---

# Run a campaign

A campaign is one plan file run end to end: the models, stacks, repetitions,
checks, and cost caps it names. Every command below runs from `tools/stack-bench`.
`controller` means this prefix:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller
```

## Before the first campaign

1. Run the demo once so the images and the `stack-bench-state` volume exist
   ([GETTING-STARTED.md](../../GETTING-STARTED.md)).
2. Save the demo's setup as your environment file:

   ```sh
   docker run --rm --mount type=volume,source=stack-bench-state,target=/state,readonly --entrypoint cat stack-bench-controller:local /state/controller-home/demo.env > operator.env
   ```

   In Windows PowerShell 5, replace `> operator.env` with
   `| Out-File -Encoding utf8 operator.env`.
3. Store the model provider's credential. It is read from the terminal, never
   from a command argument:

   ```sh
   docker run --rm -it --mount type=volume,source=stack-bench-state,target=/state stack-bench-controller:local set-secret claude_subscription_token
   ```

   Secret names: `claude_subscription_token` or `anthropic_api_key` (then set
   `STACK_BENCH_AGENT_AUTH=api-key` in `operator.env`), `openai_api_key`,
   `xai_api_key`, `gemini_api_key`, `openrouter_api_key`.

## Start

Plans live in the state volume under `plans/`. Setup installs the prepared ones.

```sh
controller campaign run plans/paid-l1.json --out campaigns/campaign-001
```

The command exits when every attempt has finished. If the host is short of
memory it stops launching, lets active attempts finish, and exits 2 with the
rest pending; run `campaign resume` later.

## Watch and control

```sh
controller campaign status campaigns/campaign-001
controller campaign inspect campaigns/campaign-001
controller campaign stop campaigns/campaign-001
controller campaign resume plans/paid-l1.json --out campaigns/campaign-001
```

`stop` ends active work and keeps its evidence. `resume` continues pending
attempts. The dashboard at `localhost:7331` shows the same campaign.

## Export

Rebuild the report, then copy the whole campaign out of the volume:

```sh
controller campaign report campaigns/campaign-001
docker create --name stack-bench-result-transfer --mount type=volume,source=stack-bench-state,target=/state,readonly stack-bench-controller:local --help
docker cp stack-bench-result-transfer:/state/results/campaigns/campaign-001 results/campaign-001
docker rm stack-bench-result-transfer
```

Open `results/campaign-001/report/report.html`. Keep the full copy; it is the
evidence behind every published number. Archive it with
[archive-evidence](../archive-evidence/SKILL.md).

## If something is interrupted

Do not remove `sb-*` containers by hand. Follow
[recover-run](../recover-run/SKILL.md) first.
