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

dependencies {
    implementation(project(":spacetimedb-sdk"))
    testImplementation(libs.kotlin.test)
}

tasks.test {
    useJUnitPlatform()
}
