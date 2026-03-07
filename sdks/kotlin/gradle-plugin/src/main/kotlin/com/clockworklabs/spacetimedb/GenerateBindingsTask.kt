package com.clockworklabs.spacetimedb

import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFile
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

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val modulePath: DirectoryProperty

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    init {
        group = "spacetimedb"
        description = "Generate SpacetimeDB Kotlin client bindings"
    }

    @TaskAction
    fun generate() {
        val outDir = outputDir.get().asFile
        outDir.mkdirs()

        val cliPath = if (cli.isPresent) cli.get().asFile.absolutePath else "spacetimedb-cli"

        execOps.exec { spec ->
            spec.commandLine(
                cliPath, "generate",
                "--lang", "kotlin",
                "--out-dir", outDir.absolutePath,
                "--module-path", modulePath.get().asFile.absolutePath,
            )
        }
    }
}
