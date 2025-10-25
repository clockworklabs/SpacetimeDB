# DEVELOP.md

A concise, opinionated guide for contributors. This doc tells you **how to set up the project, run it locally, and contribute effectively**.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Prerequisites](#prerequisites)
3. [Project Layout](#project-layout)
4. [Environment Variables](#environment-variables)
5. [Provider Setup (LLMs)](#provider-setup-llms)
6. [Build & Run](#build--run)
7. [Benchmark Suite](#benchmark-suite)
8. [Rustdoc JSON (Docs Context)](#rustdoc-json-docs-context)
9. [Quality Gates](#quality-gates)
10. [Troubleshooting](#troubleshooting)
11. [Contributing](#contributing)

---

## Quick Start

```bash
# 1) Clone
git clone <REPO_URL>
cd <REPO_ROOT>

# 2) Set minimal env for local dev (PowerShell example)
$env:SPACETIME_SERVER="local"
$env:LLM_DEBUG="true"
$env:LLM_DEBUG_VERBOSE="false"
$env:LLM_BENCH_CONCURRENCY="20"
$env:LLM_BENCH_ROUTE_CONCURRENCY="4"

# 3) Build core Rust workspace (release optional)
cargo build
# or
cargo build --release

# 4) Smoke test the benchmark runner
cargo llm --help
```

---

## Prerequisites

- **Rust**: Stable toolchain (install via [rustup](https://rustup.rs)).
    - On Windows, ensure **MSVC Build Tools** and **CMake** are on `PATH` if any native deps compile.
- **.NET 8+**: If working on the C# modules or runners.
- **Node.js 18+**: If building the website / UI for results (optional).
- **Git LFS**: If this repo stores large fixtures (optional).

> Tip (Windows): `cmake --version` and `cl.exe` should succeed. If not, open **x64 Native Tools Command Prompt for VS** or install Build Tools for VS 2022.

---

## Project Layout

```
<repo-root>/
  crates/                 # Rust workspace crates
  modules/                # SpacetimeDB modules (Rust/C#)
  benchmarks/             # LLM tasks, goldens, and configs
  tools/                  # Scripts, generators, helpers
  docs/                   # Additional docs (designs, specs)
  DEVELOP.md              # This file
```

Conventions:
- **Workspace** defined in the top-level `Cargo.toml`.
- Benchmarks are grouped by **category** (e.g., `basic/`, `schema/`) and **language** (e.g., `rust/`, `csharp/`).

---

## Environment Variables

> These are the **defaults** and/or recommended dev values. Production/CI may override them.

| Name | Purpose | Values / Example | Required |
|---|---|---|---|
| `SPACETIME_SERVER` | Target SpacetimeDB environment | `"local"` | ✅ |
| `LLM_DEBUG` | Print short debug info while generating | `true` / `false` (default `true` in dev) | ✅ |
| `LLM_DEBUG_VERBOSE` | Extra‑verbose logs (payloads, scoring detail) | `false` | ✅ |
| `LLM_BENCH_CONCURRENCY` | Parallel task concurrency across the whole bench run | `20` | ✅ |
| `LLM_BENCH_ROUTE_CONCURRENCY` | Per‑route concurrency (throttle per vendor/model) | `4` | ✅ |
| `OPENAI_API_KEY` | OpenAI credential | `sk-...` | optional* |
| `OPENAI_BASE_URL` | OpenAI-compatible base URL override | `https://api.openai.com/` | optional |
| `ANTHROPIC_API_KEY` | Anthropic credential | `...` | optional* |
| `ANTHROPIC_BASE_URL` | Anthropic base URL override | `https://api.anthropic.com` | optional |
| `GOOGLE_API_KEY` | Gemini credential | `...` | optional* |
| `GOOGLE_BASE_URL` | Gemini base URL override | `https://generativelanguage.googleapis.com` | optional |
| `XAI_API_KEY` | xAI Grok credential | `...` | optional |
| `DEEPSEEK_API_KEY` | DeepSeek credential | `...` | optional |
| `META_API_KEY` | Meta Llama credential (via provider) | `...` | optional |

\*Required only if you plan to run that provider locally.

**Canonical dev block** (copy/paste into your shell profile):

```bash
bash
OPENAI_API_KEY=
OPENAI_BASE_URL=https://api.openai.com/

ANTHROPIC_API_KEY=
ANTHROPIC_BASE_URL=https://api.anthropic.com

GOOGLE_API_KEY=
GOOGLE_BASE_URL=https://generativelanguage.googleapis.com

XAI_API_KEY=
XAI_BASE_URL=https://api.x.ai

DEEPSEEK_API_KEY=
DEEPSEEK_BASE_URL=https://api.deepseek.com

META_API_KEY=
META_BASE_URL=https://openrouter.ai/api/v1

SPACETIME_SERVER="local"
LLM_DEBUG=true
LLM_DEBUG_VERBOSE=false
LLM_BENCH_CONCURRENCY=20
LLM_BENCH_ROUTE_CONCURRENCY=4
```

Windows PowerShell:

```powershell
$env:SPACETIME_SERVER="local"
$env:LLM_DEBUG="true"
$env:LLM_DEBUG_VERBOSE="false"
$env:LLM_BENCH_CONCURRENCY="20"
$env:LLM_BENCH_ROUTE_CONCURRENCY="4"
```


### LLM Providers — Keys & Base URLs

Set only the providers you use. You can also point to OpenAI‑compatible gateways by overriding the base URL.

| Provider | API Key Env | Base URL Env (optional) | Default Base URL |
|---|---|---|---|
| OpenAI | `OPENAI_API_KEY` | `OPENAI_BASE_URL` | `https://api.openai.com` |
| Anthropic | `ANTHROPIC_API_KEY` | `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` |
| Google Gemini | `GOOGLE_API_KEY` | `GOOGLE_BASE_URL` | `https://generativelanguage.googleapis.com` |
| xAI Grok | `XAI_API_KEY` | `XAI_BASE_URL` | `https://api.x.ai` |
| DeepSeek | `DEEPSEEK_API_KEY` | `DEEPSEEK_BASE_URL` | `https://api.deepseek.com` |
| Meta (Llama) | `META_API_KEY` | `META_BASE_URL` | *(provider-specific)* |
| OpenRouter (gateway) | `OPENROUTER_API_KEY` | `OPENROUTER_BASE_URL` | `https://openrouter.ai/api/v1` |

> Notes
> - These match the providers wired in this repo (`OpenAiClient`, `AnthropicClient`, `GoogleGeminiClient`, `XaiGrokClient`, `DeepSeekClient`, `MetaLlamaClient`).
> - If you route through an OpenAI‑compatible proxy (incl. OpenRouter), set the corresponding `*_BASE_URL`.

Set only the providers you use. You can also point to OpenAI‑compatible gateways by overriding the base URL.

| Provider | API Key Env | Base URL Env (optional) | Default Base URL |
|---|---|---|---|
| OpenAI | `OPENAI_API_KEY` | `OPENAI_BASE_URL` | `https://api.openai.com` |
| Anthropic | `ANTHROPIC_API_KEY` | `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` |
| Google Gemini | `GOOGLE_API_KEY` | `GOOGLE_BASE_URL` | `https://generativelanguage.googleapis.com` |
| xAI Grok | `XAI_API_KEY` | `XAI_BASE_URL` | `https://api.x.ai` |
| DeepSeek | `DEEPSEEK_API_KEY` | `DEEPSEEK_BASE_URL` | `https://api.deepseek.com` |
| OpenRouter | `OPENROUTER_API_KEY` | `OPENROUTER_BASE_URL` | `https://openrouter.ai/api/v1` |
| Together | `TOGETHER_API_KEY` | `TOGETHER_BASE_URL` | `https://api.together.xyz/v1` |
| Mistral | `MISTRAL_API_KEY` | `MISTRAL_BASE_URL` | `https://api.mistral.ai/v1` |
| Cohere | `COHERE_API_KEY` | `COHERE_BASE_URL` | `https://api.cohere.ai/v1` |
| Groq | `GROQ_API_KEY` | `GROQ_BASE_URL` | `https://api.groq.com/openai/v1` |
| Fireworks | `FIREWORKS_API_KEY` | `FIREWORKS_BASE_URL` | `https://api.fireworks.ai/inference/v1` |
| Perplexity | `PERPLEXITY_API_KEY` | `PERPLEXITY_BASE_URL` | `https://api.perplexity.ai` |

> Notes
> - If you route Meta Llama / other OSS models through an OpenAI‑compatible proxy, set `OPENAI_BASE_URL` accordingly (or a provider‑specific `*_BASE_URL`).
> - Model identifiers are matched against route `api_model` or `display_name` (case‑insensitive).

---

## Provider Setup (LLMs)

- You can run **any subset** of providers depending on which API keys you set.
- Many routes are OpenAI-compatible; for local/open-source gateways, set `OPENAI_BASE_URL` accordingly.

Sanity check keys:

```bash
cargo llm run -- routes list
```

---

## Build & Run

### Build

```bash
# Full workspace
cargo build

# Release
cargo build --release
```

### Run Local Services (if applicable)

- **SpacetimeDB**: Ensure a local server is running if modules need it. With `SPACETIME_SERVER="local"`, the tooling points to local endpoints by default.

### Run a Single Tool/Binary

```bash
# List binaries
cargo run --bin help

# Example: run the benchmark CLI help
cargo llm --help
```

---

## Benchmark Suite

### Current Benchmarks

**basics**
000. empty-reducers — tests whether it can create basic reducers with various arguments
001. basic-tables — can it create tables with basic columns
002. scheduled-table — can it create a scheduled table and reducer
003. struct-in-table — can it put a struct in a table
004. insert — can it insert a row
005. update — can it update a row
006. delete — can it delete a row
007. crud — can it insert, update, and delete a row in the same reducer
008. index-lookup — can it look up something from an index
009. init — can it write the init reducer
010. connect — can it write the client_connected/client_disconnected reducers
011. helper-function — can it create a non-reducer helper function

**schema**
012. spacetime-product-type — can it define a new spacetime product type
013. spacetime-sum-type — can it define a new sum type
014. elementary-columns — can it create columns with basic types
015. product-type-columns — can it create columns with product types
016. sum-type-columns — can it create columns with sum types
017. scheduled — can it create scheduled columns
018. constraints — can it add primary keys, unique constraints, and indexes
019. many-to-many — can it create a many-to-many relationship
020. ecs — can it create a basic ecs
021. multi-column-index — can it create a multi-column index

Benchmarks live under `benchmarks/` with structure like:

```
benchmarks/
  t_001_foo/
    tasks/
      rust.txt
      csharp.txt
    answers/
      rust.rs
      csharp.cs
    spec.rs          # scoring config, reducer/schema checks, etc.
```

### Creating a new benchmark

1. **Copy a template**
- Duplicate any existing benchmark folder.
- Bump the numeric prefix to a new, unused ID: `t_123_my_task`.

2. **Rename for the new task**
- Rename the folder to your ID + short slug (snake-case): `t_123_my_task`.

3. **Write the task prompt**
- Create/update `tasks/rust.txt` and/or `tasks/csharp.txt`.
- Be explicit (tables, reducers, helpers, constraints). Avoid ambiguity.

4. **Add golden answers**
- Implement the canonical solution in `answers/rust.rs` and/or `answers/csharp.cs`.
- Keep them minimal and correct; compile locally if applicable.

5. **Define scoring**
- Edit `spec.rs` to add scorers (e.g., schema/table/field checks, reducer/func exists).
- Use existing scorer names where possible; keep checks specific.

6. **Quick validation**
- No provider calls (context only):  
  `cargo llm run --hash-only --tasks t_123_my_task`
- Build goldens only:  
  `cargo llm run --goldens-only --tasks t_123_my_task`

7. **Categorize**
- Ensure the folder sits under the right category path and/or set in `spec.rs`.  
  Run a subset:  
  `cargo llm run --categories basic`


### Typical Commands

```bash
# Run everything with current env (providers/models from your .env)
cargo llm run

# Only Rust (or C#)
cargo llm run --lang rust
cargo llm run --lang csharp

# Only certain categories (use your actual category names)
cargo llm run --categories basics,schema

# Only certain tasks by number (globally numbered)
cargo llm run --tasks 0,7,12

# Limit providers/models explicitly
cargo llm run \
  --providers openai,anthropic \
  --models "openai:gpt-5 anthropic:claude-sonnet-4-5"

# Dry runs
cargo llm run --hash-only         # build context only (no provider calls)
cargo llm run --goldens-only      # build/check goldens only

# Be aggressive (skip some safety checks)
cargo llm run --force

# Compare results files
cargo llm diff results/base.json results/head.json

# CI sanity check per language
cargo llm ci-check --lang rust
cargo llm ci-check --lang csharp

```

Outputs:
- Logs to stdout/stderr (respecting `LLM_DEBUG`/`LLM_DEBUG_VERBOSE`).
- JSON results in a per‑run folder (timestamped), merged into aggregate reports.

---

## Troubleshooting

**HTTP 400/404 from providers**
- Check the model ID spelling and whether it’s available for your account/region.
- Verify the correct base URL for non-default gateways.

**Timeouts / Rate-limits**
- Lower `LLM_BENCH_CONCURRENCY` or `LLM_BENCH_ROUTE_CONCURRENCY`.
- Some providers aggressively throttle bursts; use backoff/retry when supported.

---

```bash
# OpenAI / Compat
# export OPENAI_API_KEY=sk-...
# export OPENAI_BASE_URL=https://api.openai.com

# Anthropic
# export ANTHROPIC_API_KEY=...
# export ANTHROPIC_BASE_URL=https://api.anthropic.com

# Google Gemini
# export GOOGLE_API_KEY=...
# export GOOGLE_BASE_URL=https://generativelanguage.googleapis.com

# xAI Grok
# export XAI_API_KEY=...
# export XAI_BASE_URL=https://api.x.ai

# DeepSeek
# export DEEPSEEK_API_KEY=...
# export DEEPSEEK_BASE_URL=https://api.deepseek.com

# Meta (Llama) via provider
# export META_API_KEY=...
# export META_BASE_URL=...
```
