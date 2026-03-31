buildscript {
    val SPACETIMEDB_CLI by extra("/home/fromml/Projects/SpacetimeDB/target/release/spacetimedb-cli")
}
plugins {
    alias(libs.plugins.kotlinJvm) apply false
    alias(libs.plugins.kotlinMultiplatform) apply false
    alias(libs.plugins.androidKotlinMultiplatformLibrary) apply false
}

subprojects {
    afterEvaluate {
        plugins.withId("org.jetbrains.kotlin.multiplatform") {
            extensions.configure<org.jetbrains.kotlin.gradle.dsl.KotlinMultiplatformExtension> {
                jvmToolchain(21)
            }
        }
        plugins.withId("org.jetbrains.kotlin.jvm") {
            extensions.configure<org.jetbrains.kotlin.gradle.dsl.KotlinJvmExtension> {
                jvmToolchain(21)
            }
        }
    }
}
