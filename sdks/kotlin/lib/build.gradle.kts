plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.androidKotlinMultiplatformLibrary)
}

group = "com.clockworklabs"
version = "0.1.0"

kotlin {
    androidLibrary {
        compileSdk = libs.versions.android.compileSdk.get().toInt()
        minSdk = libs.versions.android.minSdk.get().toInt()
        namespace = "com.clockworklabs.spacetimedb_kotlin_sdk.shared_client"
    }

    if (org.gradle.internal.os.OperatingSystem.current().isMacOsX) {
        listOf(
            iosX64(),
            iosArm64(),
            iosSimulatorArm64()
        ).forEach { iosTarget ->
            iosTarget.binaries.framework {
                baseName = "lib"
                isStatic = true
            }
        }
    }

    jvm()

    sourceSets {
        androidMain.dependencies {
            implementation(libs.ktor.client.okhttp)
            implementation(libs.brotli.dec)
        }

        commonMain.dependencies {
            implementation(libs.kotlinx.collections.immutable)
            implementation(libs.kotlinx.atomicfu)

            implementation(libs.ktor.client.core)
            implementation(libs.ktor.client.websockets)
        }

        jvmMain.dependencies {
            implementation(libs.kotlinx.coroutines.swing)
            implementation(libs.ktor.client.okhttp)
            implementation(libs.brotli.dec)
        }

        if (org.gradle.internal.os.OperatingSystem.current().isMacOsX) {
            nativeMain.dependencies {
                implementation(libs.ktor.client.darwin)
            }
        }

        all {
            languageSettings {
                optIn("kotlin.uuid.ExperimentalUuidApi")
            }
        }

        compilerOptions.freeCompilerArgs.add("-Xexpect-actual-classes")
    }
}
