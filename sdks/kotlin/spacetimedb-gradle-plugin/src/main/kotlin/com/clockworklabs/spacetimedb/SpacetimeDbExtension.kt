package com.clockworklabs.spacetimedb

import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty

abstract class SpacetimeDbExtension {
    /** Path to the spacetimedb-cli binary. Defaults to "spacetimedb-cli" on the PATH. */
    abstract val cli: RegularFileProperty

    /** Path to the SpacetimeDB module directory. Defaults to "spacetimedb/" in the root project. */
    abstract val modulePath: DirectoryProperty

    /** Path to spacetime.local.json. Defaults to "spacetime.local.json" in the root project. */
    abstract val localConfig: RegularFileProperty

    /** Path to spacetime.json. Defaults to "spacetime.json" in the root project. */
    abstract val mainConfig: RegularFileProperty
}
