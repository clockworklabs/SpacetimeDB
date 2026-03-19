plugins {
    alias(libs.plugins.kotlinJvm)
}

dependencies {
    testImplementation(project(":spacetimedb-sdk"))
    testImplementation(libs.kotlin.test)
    testImplementation(libs.ktor.client.okhttp)
    testImplementation(libs.ktor.client.websockets)
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:${libs.versions.kotlinx.coroutines.get()}")
}

// Generated bindings live in src/jvmTest/kotlin/module_bindings/.
// Regenerate with:
//   spacetimedb-cli generate --lang kotlin \
//       --out-dir integration-tests/src/jvmTest/kotlin/module_bindings/ \
//       --module-path integration-tests/spacetimedb

val integrationEnabled = providers.gradleProperty("integrationTests").isPresent
    || providers.environmentVariable("SPACETIMEDB_HOST").isPresent

tasks.test {
    useJUnitPlatform()
    testLogging.exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
    // Requires a running SpacetimeDB server — skip unless explicitly requested.
    // Run with: ./gradlew :integration-tests:test -PintegrationTests
    // CI sets SPACETIMEDB_HOST to enable automatically.
    enabled = integrationEnabled
}
