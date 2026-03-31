import java.util.Properties

plugins {
    alias(libs.plugins.kotlinJvm)
    alias(libs.plugins.spacetimedb)
}

spacetimedb {
    modulePath.set(layout.projectDirectory.dir("spacetimedb"))
    val localProps = rootProject.file("local.properties").let { f ->
        if (f.exists()) Properties().also { it.load(f.inputStream()) } else null
    }
    (providers.environmentVariable("SPACETIMEDB_CLI").orNull
        ?: localProps?.getProperty("spacetimedb.cli"))
        ?.let { cli.set(file(it)) }
}

kotlin {
    sourceSets.all {
        languageSettings {
            optIn("com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.InternalSpacetimeApi")
        }
    }
}

dependencies {
    implementation(project(":spacetimedb-sdk"))
    testImplementation(libs.kotlin.test)
    testImplementation(libs.ktor.client.okhttp)
    testImplementation(libs.ktor.client.websockets)
    testImplementation(libs.kotlinx.coroutines.core)
}

val integrationEnabled = providers.gradleProperty("integrationTests").isPresent
    || providers.environmentVariable("SPACETIMEDB_HOST").isPresent

tasks.test {
    useJUnitPlatform()
    testLogging.exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
    enabled = integrationEnabled
}
