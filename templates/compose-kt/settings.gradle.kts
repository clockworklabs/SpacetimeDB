@file:Suppress("UnstableApiUsage")

rootProject.name = "compose-kt"
enableFeaturePreview("TYPESAFE_PROJECT_ACCESSORS")

pluginManagement {
    repositories {
        google {
            mavenContent {
                includeGroupAndSubgroups("androidx")
                includeGroupAndSubgroups("com.android")
                includeGroupAndSubgroups("com.google")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
    // TODO: Replace with published Maven coordinates once the SDK is available on Maven Central.
    // includeBuild("<path-to-spacetimedb-kotlin-sdk>/spacetimedb-gradle-plugin")
}

dependencyResolutionManagement {
    repositories {
        google {
            mavenContent {
                includeGroupAndSubgroups("androidx")
                includeGroupAndSubgroups("com.android")
                includeGroupAndSubgroups("com.google")
            }
        }
        mavenCentral()
    }
}

plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

// TODO: Replace with published Maven coordinates once the SDK is available on Maven Central.
// includeBuild("<path-to-spacetimedb-kotlin-sdk>")

include(":desktopApp")
include(":androidApp")
include(":sharedClient")