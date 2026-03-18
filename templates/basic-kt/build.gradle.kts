plugins {
    alias(libs.plugins.kotlinJvm)
    alias(libs.plugins.spacetimedb)
    application
}

kotlin {
    jvmToolchain(21)
}

application {
    mainClass.set("MainKt")
}

spacetimedb {
    modulePath.set(layout.projectDirectory.dir("spacetimedb"))
}

dependencies {
    implementation(libs.spacetimedb.sdk)
    implementation(libs.kotlinx.coroutines.core)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.ktor.client.websockets)
}
