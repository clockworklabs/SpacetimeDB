# Container status and lifecycle requests

`spacetime container status DATABASE` reads the current control state without
opening database storage. Viewer access is sufficient. `--json` returns the
typed status, including desired and observed state, deployment revision,
generation, fixed diagnostics, and public endpoint availability. An absent
current instance is distinct from an old instance's terminal report. Endpoint
allocation may be pending while the rest of the status is available.

`spacetime container start DATABASE`, `stop DATABASE`, and `restart DATABASE`
require Admin access. Start requests execution, stop requests a stop, and restart
requests a new instance with a fresh environment snapshot. Acceptance records
the desired action; physical stop and readiness complete asynchronously. Use
status to inspect progress. None of these commands changes the published image
or its declaration.

Each lifecycle request uses a UUIDv7. Before sending it, the CLI prints structured
retry parameters to stderr: the action, resolved database Identity, request ID,
and selected server URL. Keep these parameters if the response is lost. Retry
with that Identity, the same action, `--request-id UUID`, and `--server URL`.
`--request-id` requires an Identity so a changed database name cannot redirect a
retry. Never create a fresh request ID merely to resolve an unknown outcome.

An accepted exact retry returns the original result generation, even if another
request has since advanced the database generation. Reusing an ID for another
action fails. Current Admin access is checked again on every retry. The retry
window is seven days from the UUID's timestamp. `--json` writes only a verified
receipt to stdout; its generation is a decimal string to preserve all 64 bits.

Server selection and login use the normal CLI configuration, with an explicit
`--server` override supported on every command. Authenticated requests disable
redirects and inherited proxies. Public `container url` remains anonymous, and
local `container build` continues to run without loading server credentials.

The HTTP counterparts are authenticated `GET
/v1/database/DATABASE/container/status` and `POST` to the `start`, `stop`, or
`restart` suffix with `{"request_id":"UUID"}`. Successful mutations return HTTP
202. Responses are not cacheable. The operational API rejects hosted container
credentials and uses current ordinary database roles.
