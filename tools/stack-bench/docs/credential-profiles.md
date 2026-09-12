# Named execution credentials

A job selects credentials by profile ID. Selection order is attempt ID, adapter ID,
then default. Selection is explicit; the runner does not rotate accounts.

Set `STACK_BENCH_CREDENTIAL_PROFILES_FILE` to an absolute path in trusted controller
storage. The file maps profile IDs to provider, mode, secret file, and version:

```json
{
  "claude-work": {
    "provider": "anthropic",
    "mode": "subscription-token",
    "secretFile": "/state/secrets/claude-work",
    "version": "v1"
  },
  "openai-api": {
    "provider": "openai",
    "mode": "api-key",
    "secretFile": "/state/secrets/openai-api",
    "version": "v1"
  }
}
```

The example paths must be replaced with paths available inside the controller.
Use protected secret files. Do not put credential values in a job or campaign.

Execution credential references have this form:

```json
{
  "default": "claude-work",
  "adapters": { "codex": "openai-api" },
  "attempts": { "an-exact-attempt-id": "claude-work" }
}
```

Profiles must match the selected adapter's provider. `anthropic` accepts API keys
or subscription tokens. `openai` accepts API keys or `subscription-token` mode;
that mode reads the existing Codex ChatGPT account login JSON file. `openrouter`
accepts API keys only. Normal provider preflight still validates authentication.

Execution evidence stores only the profile ID, version, provider, and mode.
Secret contents and file paths are not attribution fields. Before each provider
invocation, the worker checks that its selected profile and secret have not changed
since admission. Update the profile version when deliberately replacing a secret.
Do not overwrite a secret used by an active attempt. Use a new profile for new work.

Existing environment-based credentials continue to work when no named assignment
is selected. Profile selection clears conflicting credentials for that provider
and generic API-key overrides. It preserves other providers' credentials for
mixed-adapter jobs.
