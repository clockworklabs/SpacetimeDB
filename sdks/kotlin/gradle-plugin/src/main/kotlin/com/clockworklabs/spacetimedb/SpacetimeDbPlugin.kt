package com.clockworklabs.spacetimedb

import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.tasks.Delete
import org.gradle.api.tasks.SourceSetContainer
import org.jetbrains.kotlin.gradle.dsl.KotlinMultiplatformExtension

class SpacetimeDbPlugin : Plugin<Project> {

    override fun apply(project: Project) {
        val ext = project.extensions.create("spacetimedb", SpacetimeDbExtension::class.java)

        ext.modulePath.convention(project.layout.projectDirectory.dir("spacetimedb"))

        val generatedDir = project.layout.buildDirectory.dir("generated/spacetimedb")

        // Clean the Rust target directory when running `gradle clean`
        project.tasks.register("cleanSpacetimeModule", Delete::class.java) {
            it.group = "spacetimedb"
            it.description = "Clean SpacetimeDB module build artifacts"
            it.delete(ext.modulePath.map { dir -> dir.dir("target") })
        }
        project.tasks.named("clean") { it.dependsOn("cleanSpacetimeModule") }

        val generateTask = project.tasks.register("generateSpacetimeBindings", GenerateBindingsTask::class.java) {
            it.cli.set(ext.cli)
            it.modulePath.set(ext.modulePath)
            it.outputDir.set(generatedDir)
        }

        // Wire generated sources into Kotlin compilation
        project.pluginManager.withPlugin("org.jetbrains.kotlin.jvm") {
            project.extensions.getByType(SourceSetContainer::class.java)
                .getByName("main")
                .java
                .srcDir(generatedDir)

            project.tasks.named("compileKotlin") {
                it.dependsOn(generateTask)
            }
        }

        project.pluginManager.withPlugin("org.jetbrains.kotlin.multiplatform") {
            project.extensions.getByType(KotlinMultiplatformExtension::class.java)
                .sourceSets
                .getByName("commonMain")
                .kotlin
                .srcDir(generatedDir)

            project.tasks.matching { it.name.startsWith("compile") }.configureEach {
                it.dependsOn(generateTask)
            }
        }
    }
}
