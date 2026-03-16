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

tasks.test {
    useJUnitPlatform()
    // Integration tests need a running SpacetimeDB server.
    // Skip by default; run explicitly with: ./gradlew :integration-tests:test -PintegrationTests
    enabled = System.getenv("SPACETIMEDB_HOST") != null || project.hasProperty("integrationTests")
}
