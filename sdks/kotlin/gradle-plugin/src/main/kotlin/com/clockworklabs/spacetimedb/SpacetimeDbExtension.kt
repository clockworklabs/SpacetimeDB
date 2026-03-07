package com.clockworklabs.spacetimedb

import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty

abstract class SpacetimeDbExtension {
    /** Path to the spacetimedb-cli binary. Defaults to "spacetimedb-cli" on the PATH. */
    abstract val cli: RegularFileProperty

    /** Path to the SpacetimeDB module directory. Defaults to "spacetimedb/" in the project root. */
    abstract val modulePath: DirectoryProperty
}
