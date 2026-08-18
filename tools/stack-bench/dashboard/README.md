# StackBench dashboard

The dashboard is an optional local view and control surface over the existing
StackBench CLI. It does not schedule attempts, grade applications, operate
Docker, or repair source itself. Campaign plans, durable campaign state, run
artifacts, and controller commands remain the source of truth.

The supported deployment runs from the same pinned controller image as the
CLI:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
```

Open `http://127.0.0.1:7331`. Docker publishes that port only on the host's
loopback interface. Stop it with:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml --profile dashboard stop dashboard
```

The service binds only to loopback. It reads campaign manifests from
`/var/lib/stack-bench/results/plans`, reads campaigns from
`/var/lib/stack-bench/results/campaigns`, and appends dashboard-submitted
operations to `/var/lib/stack-bench/results/dashboard/operations.jsonl`.
Starting a campaign invokes the same `campaign run` command used by the CLI.
The CLI can inspect or resume the result normally, and CLI-started campaigns
appear in the dashboard.

For UI development only, `npm run dashboard` starts a read-only host view over
`tools/stack-bench/results`. Run controls are deliberately disabled outside the
appliance.
