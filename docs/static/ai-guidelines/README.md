# AI Guidelines

Constructive-only "happy path" cheat sheets optimized for **one-shot code generation**.
Each file shows correct SpacetimeDB patterns for a single language — no anti-patterns,
no "don't do this" sections.

## Files

| File | Language |
|------|----------|
| `spacetimedb-rust.md` | Rust server modules |
| `spacetimedb-typescript.md` | TypeScript server modules |
| `spacetimedb-csharp.md` | C# server modules |

## How these are used

The LLM benchmark (`tools/xtask-llm-benchmark`) injects these as context when running
in `guidelines` mode. The benchmark sends the guidelines as a static prefix along with
task instructions, then validates the generated code by publishing it to SpacetimeDB.

## Guidelines vs Cursor Rules

These are **not** the same as the cursor/IDE rules in `docs/static/ai-rules/`.

- **Guidelines** (this directory): Minimal, constructive-only references for one-shot
  generation. Show the correct way to do things and nothing else.
- **Cursor rules** (`ai-rules/`): IDE-oriented `.mdc` rules for Cursor, Windsurf, etc.
  Include anti-hallucination guardrails, common mistake tables, client-side patterns, and
  migration guidance. These work well in an IDE where the model has project context and
  can iterate, but they are not designed for single-shot benchmark generation.

See the comment in `tools/xtask-llm-benchmark/src/context/constants.rs` for more detail.
