# Benchmark tasks

The goal is simple: valid code using the current API passes, and outdated or wrong code fails. A failure must measure the model, not our setup.

Every prompt change alters the test for every model. Change as little as the fix needs, and never rewrite prompts for style.

## Files

A task lives in `src/benchmarks/<category>/t_NNN_name/`:

- `spec.rs`: the graders
- `tasks/rust.txt`, `tasks/typescript.txt`, `tasks/csharp.txt`: the prompts
- `answers/rust.rs`, `answers/typescript.ts`, `answers/csharp.cs`: the reference answers

The build regenerates `src/generated/registry.rs`, which is tracked, so commit it with a new task. Update the task count and the task table in `README.md`.

## Choosing a task

- Test one concept. Compare `advanced/t_087_webhook_ordering` for a hard task and `basics/t_004_insert` for a simple one.
- Test a SpacetimeDB contract, not another product's API.
- Use local fixtures for outside calls, never a paid service.
- Do not name other products in task files, the README or commit messages.

## Prompts

Before writing a prompt, read the prompts of the nearest tasks in the same category and copy their format. Most tasks use `TABLE` and `REDUCERS` sections. The advanced tasks use short paragraphs followed by a table list.

**State requirements, not how to meet them.**

- Say "the caller's identity", "the current time" and "fail with", not `ctx.sender`, a time method or `panic!`. Models are being tested on that syntax.
- Say when something runs, such as "when the module is first published", not which lifecycle API to use.
- Do not list the collections or iterator calls to use.
- Do not reveal values that the checks use, such as a test URL.

**State everything the checks enforce.** A model cannot guess these:

- Table and view names, field names, field types, and which tables are public
- Primary keys, auto-increment and indexes
- Reducer names, argument types and any required return type
- Exact text, whenever a check compares text
- That a module has no tables or no reducers, when an extra one would fail the schema check

**Keep the three languages the same.** Only names and types differ.

- Use each language's declared names in the prose too: `event_id` in Rust, `eventId` in TypeScript, `EventId` in C#.
- Use each language's types. Mark 64-bit TypeScript integers as bigint, as in `u64 (bigint)`.
- Check every name you write against the SDK. Earlier prompts named a C# method that does not exist.

**Add nothing that no model needs.**

- Never add a sentence just to get past a check or a content filter. If a valid answer fails, fix the grader.
- The runner already tells models to output one source file and to copy identifiers exactly. New prompts need not repeat this.
- Start with `Write a SpacetimeDB backend module in <Rust|TypeScript|C#> that ...`. Migration tasks and a few others vary, so follow the nearest task.

## Reference answers

- Write each answer the way the current guidelines in the root `skills/` directory teach it.
- Use the current API in all three languages.
- Include only what the prompt asks for. Extra reducers or tables make the schema check compare against the wrong thing.

## Graders

Start from `default_schema_parity_scorers`, then add a behaviour check. `scenario` in `src/eval/scenario.rs` is the usual tool. A schema check alone passes a module whose reducers do nothing.

**A check must fail a module that does nothing.**

- Assert the rows or replies you expect, including intermediate state.
- Confirm that setup rows exist before a delete, and that rows outside the delete survive.
- Assert explicit expected rows. Agreement between two copies of the reference proves little.
- Cover concurrency with real parallel calls where the task promises it, as `t_087` does.

**A check must pass every design the prompt allows.**

- Trace each assertion to a sentence in the prompt. If none exists, delete the assertion. State the requirement instead only when the task cannot test its concept without it.
- Accept any rejection unless the prompt quotes the error text. A plain panic or throw that rolls back is a valid rejection.
- Test error delivery to the caller in its own task, not as part of atomicity or lease tasks.
- Accept extra indexes and extra HTTP routes. Ignore case and underscores in names. The shared helpers already do this, so use them instead of hand-written SQL.
- Do not treat a transport error as an expected rejection.

## Validate

Check only the tasks you touched. A prompt-only change does not need a full reference run.

1. Run the crate's tests, `cargo fmt` and clippy.
2. Run the reference answers for each language, after the local builds in `README.md`. This makes no model calls and uploads nothing.

   ```text
   cargo run -p xtask-llm-benchmark --bin llm_benchmark --locked -- run --lang rust --goldens-only --dry-run --skip-task-catalog-upload --tasks t_NNN
   ```

3. Prove the behaviour check can fail. Temporarily replace the answer with a version that does nothing, then with one that breaks a single requirement. Run the same command and confirm both fail. Restore the answer. The schema check compares an answer with itself here, so only the behaviour check can catch these.
4. Run real models without uploading. Use the `Periodic LLM benchmarks` workflow with `model_set=explicit`, one or two cheap models, the `tasks` input and `dry_run=true`. Enabling, dispatching and disabling that workflow needs the owner's approval.
5. Read the failures and sort them with the table below. Do not assume a failure is the model's fault.

## Whose fault is a failure?

| What happened | What to do |
| --- | --- |
| Valid current-API code fails | Our bug. Fix the grader. |
| Outdated or wrong code passes | Our bug. Fix the grader. |
| Many models fail the same way on a requirement | Suspect the prompt. Check that it states the requirement and that the requirement is real. |
| The prompt names something wrong | Our bug. Fix the prompt. |
| Models use outdated syntax | A model failure. Keep it. |
| Guidelines mode misses a pattern that models need | Fix the root `skills/` files, only where a run shows a benefit. `llms.md` is generated from them. |
| The SDK itself is at fault | Keep the failure and report it to the core team. Do not work around it. |
| The provider rejects the content | Keep the failure. Do not reword the prompt to avoid it. |
| The provider or the setup fails | The batch is refused. Fix the cause and rerun the batch. |

## Changing a published task

- Scores from before the change no longer compare with scores after it. The README should say so when the suite changes.
- Website uploads only add rows. Old rows stay on Trends until someone deletes them from the admin page.
- A refused batch uploads nothing, so a rerun has to cover the whole batch. Group changes so that one rerun covers them all.
- Weakening or deleting a check must be stated in the pull request, as the root `AGENTS.md` requires.
