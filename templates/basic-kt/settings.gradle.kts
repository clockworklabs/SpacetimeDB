@file:Suppress("UnstableApiUsage")

rootProject.name = "basic-kt"

pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
    // TODO: Replace with published Maven coordinates once the SDK is available on Maven Central.
    // includeBuild("<path-to-spacetimedb-kotlin-sdk>/spacetimedb-gradle-plugin")
}

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
}

plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

// TODO: Replace with published Maven coordinates once the SDK is available on Maven Central.
// includeBuild("<path-to-spacetimedb-kotlin-sdk>")
