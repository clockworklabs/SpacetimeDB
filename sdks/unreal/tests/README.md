# Unreal Headless Test Harness for SpacetimeDB

This directory contains the Unreal Engine headless test harness for the **SpacetimeDB Unreal SDK**.

---

## Overview
The tests here validate the Unreal SDK integration against a running SpacetimeDB instance.  
They use the Rust-based test harness in [`sdk.rs`](../../testing/src/sdk.rs) and execute an Unreal test project headlessly.

- **Test Harness**:  
  `crates\testing\src\sdk.rs`

- **Unreal Test Entry Point**:  
  `crates\sdk-unreal\tests\test.rs`

- **Unreal Test Project**:  
  `crates\sdk-unreal\tests\TestClient`

## Prerequisites

The tests require an environment variable pointing to your Unreal Engine installation:
Example for Unreal Engine 5.6:
```sh
set UE_ROOT_PATH="C:/Program Files/Epic Games/UE_5.6"
```

## Running the Test

From inside the Unreal SDK crate:

```sh
cd \sdk-unreal
cargo test -p sdk-unreal-test-harness --test test -- --nocapture
```

## Using a Custom SpacetimeDB Build

To run tests against a custom CLI build of SpacetimeDB, set the following environment variables:
```sh
set CUSTOM_SPACETIMEDB_ROOT=true
set CUSTOM_SPACETIMEDB_PATH=<Path to custom spacetime CLI>
```



## Unreal Engine Command Line Flags

When running Unreal headlessly in CI, the following flags are commonly used:

| Flag | Purpose |
| --- | --- |
| `-ExecCmds="..."` | Executes in-editor console commands in sequence (e.g., `Automation RunTests ...; Automation ExportReport; Quit`). |
| `-Unattended` | Disables all interactive dialogs; required for fully automated/headless runs. |
| `-NoPause` | Prevents “Press any key to continue” prompts on exit. |
| `-NullRHI` | Disables rendering entirely; speeds up headless runs and avoids GPU usage. |
| `-NoSplash` | Skips the Unreal splash screen for faster startup. |
| `-NoSound` | Disables audio systems; can improve performance in automation. |
| `-nop4` | Disables Perforce integration; avoids P4 login prompts in CI. |
| `-log` | Prints log output to the console window (instead of only writing to file). |
| `-Log="Path\To\File.log"` | Writes engine log output to a specific file. |
| `-ReportOutputPath="..."` | Directory where HTML reports (and related assets) will be saved when running `Automation ExportReport`. |
| `-ReportExportPath="..."` | File path for exporting the raw test results as JSON, which the HTML report uses. |
| `-LogCmds="automation veryverbose"` | Increases log verbosity for the automation system; useful for debugging test discovery and execution. |
| `-test="Filter"` | (When using `-run=Automation`) Runs only tests that match the given filter. |
| `-run=Automation` | Runs the Automation commandlet (source builds or certain editor configs only). Not always available in installed builds. |