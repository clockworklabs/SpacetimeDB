plugins {
    alias(libs.plugins.kotlinJvm)
    application
}

kotlin {
    jvmToolchain(21)
}

application {
    mainClass.set("MainKt")
}

dependencies {
    implementation(libs.spacetimedb.sdk)
    implementation(libs.kotlinx.coroutines.core)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.ktor.client.websockets)
}
