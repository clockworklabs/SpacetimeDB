@file:Suppress("UnstableApiUsage")

rootProject.name = "spacetimedb-kotlin-tps-bench"

pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
}

plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

// Resolve SDK + gradle plugin from the local checkout
includeBuild("../../../sdks/kotlin")
