# SpacetimeDB Gradle Plugin

Gradle plugin for SpacetimeDB Kotlin projects. Automatically generates Kotlin client bindings from your SpacetimeDB module.

## Setup

```kotlin
// settings.gradle.kts
pluginManagement {
    includeBuild("/path/to/SpacetimeDB/sdks/kotlin")
}

// build.gradle.kts
plugins {
    id("com.clockworklabs.spacetimedb")
}
```

## Configuration

```kotlin
spacetimedb {
    // Path to the SpacetimeDB module directory (default: "spacetimedb/" in project root)
    modulePath.set(file("spacetimedb"))

    // Path to spacetimedb-cli binary (default: resolved from PATH)
    cli.set(file("/path/to/spacetimedb-cli"))
}
```

## Tasks

| Task | Description |
|------|-------------|
| `generateSpacetimeBindings` | Runs `spacetimedb-cli generate` to produce Kotlin bindings. Automatically wired into `compileKotlin`. |
| `cleanSpacetimeModule` | Deletes `spacetimedb/target/` (Rust build cache). Runs as part of `gradle clean`. |

## Notes

- **`gradle clean` triggers a full Rust recompilation** on the next build, since `cleanSpacetimeModule` deletes the Cargo `target/` directory. To clean only Kotlin artifacts, use:
  ```
  gradle clean -x cleanSpacetimeModule
  ```
- The plugin detects module source changes and re-generates bindings automatically.
- Both `org.jetbrains.kotlin.jvm` and `org.jetbrains.kotlin.multiplatform` are supported.
