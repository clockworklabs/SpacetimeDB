plugins {
    alias(libs.plugins.kotlinJvm)
}

dependencies {
    testImplementation(project(":spacetimedb-sdk"))
    testImplementation(libs.kotlin.test)
}

tasks.test {
    useJUnitPlatform()
}
