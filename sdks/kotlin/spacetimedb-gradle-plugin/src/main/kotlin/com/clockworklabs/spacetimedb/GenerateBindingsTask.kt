package com.clockworklabs.spacetimedb

import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.Optional
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import javax.inject.Inject

abstract class GenerateBindingsTask @Inject constructor(
    private val execOps: ExecOperations
) : DefaultTask() {

    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.ABSOLUTE)
    abstract val cli: RegularFileProperty

    @get:Internal
    abstract val modulePath: DirectoryProperty

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val moduleSourceFiles: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    init {
        group = "spacetimedb"
        description = "Generate SpacetimeDB Kotlin client bindings"
    }

    @TaskAction
    fun generate() {
        val moduleDir = modulePath.get().asFile
        require(moduleDir.isDirectory) {
            "SpacetimeDB module directory not found at '${moduleDir.absolutePath}'. " +
            "Set the correct path via: spacetimedb { modulePath.set(file(\"/path/to/module\")) }"
        }

        val outDir = outputDir.get().asFile
        if (outDir.isDirectory) {
            outDir.listFiles()?.forEach { it.deleteRecursively() }
        }
        outDir.mkdirs()

        val cliPath = if (cli.isPresent) {
            cli.get().asFile.absolutePath
        } else {
            "spacetimedb-cli"
        }

        try {
            execOps.exec { spec ->
                spec.commandLine(
                    cliPath, "generate",
                    "--lang", "kotlin",
                    "--out-dir", outDir.absolutePath,
                    "--module-path", modulePath.get().asFile.absolutePath,
                )
            }
        } catch (e: Exception) {
            if (!cli.isPresent) {
                logger.warn(
                    "spacetimedb-cli not found — Kotlin bindings will not be auto-generated. " +
                    "Install from https://spacetimedb.com or set: spacetimedb { cli.set(file(\"/path/to/spacetimedb-cli\")) }"
                )
                return
            }
            throw e
        }
    }
}
