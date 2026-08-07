#!/usr/bin/env node
// What a generated build is not allowed to read.
//
// This lives apart from agent.mjs so that probe-sandbox.mjs can assert against
// the very rules a build is given. A probe holding its own copy of the list
// would keep passing after the real list drifted, which is worse than no probe:
// it reports assurance it no longer has.
//
// `--add-dir` looks like a boundary and is not one: under
// --dangerously-skip-permissions it is advisory, and a session given only an
// empty temp directory still read this project's notes when asked. Audits of
// past runs found builds reading scenarios/01-invariants.json (the assertions
// themselves), grade.mjs (the marking scheme), a sibling run's source, and the
// benchmark's own notes — up to 44 times in a single run.
//
// WHICH LAYER DOES WHAT, measured rather than assumed. Running the probe with
// an EMPTY deny list still refuses every outside path, so it is
// `--permission-mode acceptEdits` — not these rules — that closes the harness
// off: it withholds approval from any file-tool read outside the working
// directory, and --print has nobody to ask. What the rules add is everything
// INSIDE the app directory, which the mode allows freely. That is not
// hypothetical: sequential builds read stack-bench grading output that had been
// written into their own app folder. Verified both ways — an in-app
// `stack-bench/grading-features.json` is refused while a sibling
// `app-source.txt` reads normally.
//
// These rules govern the FILE TOOLS ONLY. `Read(...)` does not apply to Bash:
// a session refused by the Read rule can still `cat` the same path, verified.
// Closing that would mean banning shell commands a build legitimately needs,
// so the sandbox is defence in depth, not the control. The control is
// leak-audit.mjs, which reads the session transcript afterwards and marks a run
// contaminated if it escaped — prevention we cannot guarantee, detection we can.
//
// Pattern notes: glob form (`**/x/**`) matches where an absolute form
// (`//C:/...`) silently does not on Windows, `**` does NOT traverse a
// leading-dot directory, and deny beats allow.

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

// NOTE the app itself lives under stack-bench/results/<run>/app, so a blanket
// deny on stack-bench would block the build from reading its own source. Name
// the harness directories instead, and never deny BUG_REPORT.md — fix mode is
// told to read it.
export const DENY = [
  // The test and the marking scheme.
  'Read(**/stack-bench/tracks/**)',
  'Read(**/stack-bench/grader/**)',
  'Read(**/stack-bench/linter/**)',
  'Read(**/stack-bench/levels/**)',
  // Harness source and its own documentation.
  'Read(**/stack-bench/*.mjs)',
  'Read(**/stack-bench/*.md)',
  'Read(**/stack-bench/*.sh)',
  'Read(**/stack-bench/backends/**)',
  // Archived transcripts of earlier builds: each one quotes the harness files
  // that build read, so leaving them open would hand a later run the marking
  // scheme second-hand.
  'Read(**/stack-bench/transcripts/**)',
  // The other benchmarks, and their prompts and rubrics.
  'Read(**/llm-sequential-upgrade/**)',
  'Read(**/llm-oneshot/**)',
  // Any agent's notes, plans or transcripts — including this one's.
  // `**/.claude/**` alone does NOT match: a leading-dot directory is not
  // traversed by `**`, so a run read this project's notes straight through it.
  // The `projects/**` rule is the one that actually bites; the rest are belt
  // and braces, and probe-sandbox.mjs proves the set.
  'Read(**/projects/**)',
  'Read(**/.claude/**)',
  'Read(.claude/**)',
  'Read(**/memory/**)',
  'Read(**/*.local.md)',
];

// Under `--permission-mode acceptEdits` the file tools are auto-approved but
// Bash is not, and in --print mode an unapproved command is simply refused: a
// build that cannot run `npm install` or `spacetime publish` does not build.
// So Bash is allowed wholesale. That is the same hole named above — `cat` still
// reaches a denied path — and it is why leak-audit.mjs, not this file, decides
// whether a run counts.
export const ALLOW = ['Bash'];

export function writeSandbox(appDir) {
  const p = join(appDir, '.sandbox-settings.json');
  writeFileSync(p, JSON.stringify({ permissions: { allow: ALLOW, deny: DENY } }, null, 2));
  return p;
}
