# SpacetimeDB Gradle Plugin

Gradle plugin for SpacetimeDB Kotlin projects. Automatically generates Kotlin client bindings and build-time configuration from your SpacetimeDB module.

## Setup

```kotlin
// settings.gradle.kts
pluginManagement {
    includeBuild("/path/to/SpacetimeDB/sdks/kotlin/spacetimedb-gradle-plugin")
}

// build.gradle.kts
plugins {
    id("com.clockworklabs.spacetimedb")
}
```

## Configuration

```kotlin
spacetimedb {
    // Path to the SpacetimeDB module directory.
    // Default: read from "module-path" in spacetime.json, falls back to "spacetimedb/"
    modulePath.set(file("server"))

    // Path to spacetimedb-cli binary (default: resolved from PATH)
    cli.set(file("/path/to/spacetimedb-cli"))

    // Config file paths (default: spacetime.local.json and spacetime.json in root project)
    localConfig.set(file("spacetime.local.json"))
    mainConfig.set(file("spacetime.json"))
}
```

## Generated Files

### Bindings (`build/generated/spacetimedb/bindings/`)

Kotlin data classes, table handles, reducer stubs, and query builders generated from your module's schema via `spacetimedb-cli generate`.

### SpacetimeConfig (`build/generated/spacetimedb/config/SpacetimeConfig.kt`)

Build-time constants extracted from `spacetime.local.json` / `spacetime.json`:

```kotlin
package module_bindings

object SpacetimeConfig {
    const val DATABASE_NAME: String = "my-app"      // from "database" field
    const val MODULE_PATH: String = "./spacetimedb"  // from "module-path" field
}
```

Fields are only included when present in the config. `spacetime.local.json` takes priority over `spacetime.json`.

## Tasks

| Task | Description |
|------|-------------|
| `generateSpacetimeBindings` | Runs `spacetimedb-cli generate` to produce Kotlin bindings. Wired into `compileKotlin`. |
| `generateSpacetimeConfig` | Generates `SpacetimeConfig.kt` from project config. Wired into `compileKotlin`. |
| `cleanSpacetimeModule` | Deletes `spacetimedb/target/` (Rust build cache). Runs as part of `gradle clean`. |

## Notes

- **`gradle clean` triggers a full Rust recompilation** on the next build, since `cleanSpacetimeModule` deletes the Cargo `target/` directory. To clean only Kotlin artifacts:
  ```
  gradle clean -x cleanSpacetimeModule
  ```
- The plugin detects module source changes and re-generates bindings automatically.
- Both `org.jetbrains.kotlin.jvm` and `org.jetbrains.kotlin.multiplatform` are supported.
