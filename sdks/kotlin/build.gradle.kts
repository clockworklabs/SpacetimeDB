plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.kotlinAtomicfu)
    `maven-publish`
}

group = "com.clockworklabs"
version = "0.1.0"

kotlin {
    jvm()
    iosArm64()
    iosSimulatorArm64()
    macosArm64()

    applyDefaultHierarchyTemplate()

    compilerOptions {
        optIn.addAll(
            "kotlin.concurrent.atomics.ExperimentalAtomicApi",
            "kotlin.uuid.ExperimentalUuidApi"
        )
    }

    sourceSets {
        commonMain {
            dependencies {
                implementation(libs.kotlinx.coroutines.core)
                implementation(project.dependencies.platform(libs.ktor.bom))
                implementation(libs.ktor.client.core)
                implementation(libs.ktor.client.websockets)
            }
        }
        commonTest {
            dependencies {
                implementation(kotlin("test"))
            }
        }
        jvmMain {
            dependencies {
                implementation(libs.ktor.client.okhttp)
                implementation(libs.brotli.dec)
            }
        }
        appleMain {
            dependencies {
                implementation(libs.ktor.client.darwin)
            }
        }
    }
}

publishing {
    publications {
        withType<MavenPublication> {
            pom {
                name.set("SpacetimeDB Kotlin SDK")
                description.set("SpacetimeDB client SDK for Kotlin Multiplatform")
            }
        }
    }
    repositories {
        mavenLocal()
    }
}

tasks.matching { it.name == "jvmTest" }.configureEach {
    if (this is Test) {
        testLogging {
            showStandardStreams = true
        }
        maxHeapSize = "1g"
    }
}
