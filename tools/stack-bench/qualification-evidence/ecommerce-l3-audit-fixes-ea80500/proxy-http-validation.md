# HTTP capture candidate validation

The review file was re-read in full. Its SHA-256 remains `01b5eb82dabc5fe389334c5b3358e70e6b60248a796bd40abd51b4148957515d` (27,326 bytes; September 27, 12:04:40 local). See `review-current-disposition-27e01bd86.md` for the full finding disposition.

## Change and preserved rules

The existing authenticated browser proxy now supplies bounded plaintext HTTP snapshots. It observes bytes forwarded to each browser context, independently of page reads and missing Chromium completion events. HTTPS remains opaque to the proxy. Existing browser capture remains the fallback.

The first integration draft incorrectly stopped waiting for active HTTP requests. Existing E2E tests caught delayed leaks and canceled-read regressions. That draft was rejected. The proxy now reports actual active downstream requests, including requests without headers. The original navigation deadline and final one-second drain remain. Native EventSource is exempt from completion waits; fetch-SSE remains unsupported and inconclusive. A client cancellation settles activity without claiming a complete body.

Partial bytes remain separate from completed responses and successful write receipts. Cumulative privacy snapshots replace redundant prefixes in the existing bounded buffer. Normal complete-body storage keeps its old exact deduplication. Captured leaks still fail; overflow or missing observation still prevents an absence pass.

## Repeatable validation

Run from `tools/stack-bench` after `npm run build --silent`:

```
node --test dist/tests/network-interruption.integration.js dist/tests/transport-cache.integration.js dist/tests/transport-frames.test.js
```

- `proxy-http-final.log`: **68/68 passed**, including existing reload/drain, cache, fresh/reopened context, native EventSource, fault interruption, split UTF-8, compression, detached observer, overflow, live-request lifecycle, replay-tunnel exclusion, and partial-write checks.
- The final restriction of prefix deduplication to cumulative privacy snapshots was then checked with both transport files: `proxy-http-candidate.log`, **52/52 passed**. The helper was unchanged from the 68-test run.
- `proxy-http-typecheck-final.log`: typecheck passed.
- `proxy-http-candidate-lint.log`: focused lint passed.
- The only later source change removes a redundant assignment already performed by the constructor; the staged review records that change.

Set `STACK_BENCH_PROXY_HTTP_EVIDENCE` and `STACK_BENCH_CAPTURE_EVIDENCE` to output paths to save structured helper and browser artifacts. `proxy-http-final-helper.json` and `proxy-http-candidate.json*` hold this run's evidence. The tests close their contexts, browsers, listeners and proxy children.

Failed drafts and red proofs are retained: `proxy-http-red*`, `proxy-http-green*`, `proxy-http-existing-behavior*`, `proxy-prefix-red*`, `proxy-lifecycle-red*`, and `proxy-http-preserved*`. The overflow fixture was corrected to consume the response before asserting overflow. Its earlier discarded response did not prove that all bytes reached the browser. The existing stalled-response assertion now checks the proxy-owned request diagnostic; its inconclusive verdict and private-data redaction checks remain.

## Release limit

This is local integration evidence, not release qualification. No paid study was started. Historical grades remain unchanged. The candidate still needs affected reference/defect qualification and validated evidence reuse, then the normal immutable controller-image gate. The traced privacy scope remains four execution groups, nine checks and 46 registered defects; widen only if the final executable diff proves another affected behavior.
