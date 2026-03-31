plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.androidKotlinMultiplatformLibrary)
}

group = "com.clockworklabs"
version = "0.1.0"

kotlin {
    explicitApi()

    android {
        compileSdk = libs.versions.android.compileSdk.get().toInt()
        minSdk = libs.versions.android.minSdk.get().toInt()
        namespace = "com.clockworklabs.spacetimedb_kotlin_sdk.shared_client"
    }

    listOf(
        iosX64(),
        iosArm64(),
        iosSimulatorArm64()
    ).forEach { iosTarget ->
        iosTarget.binaries.framework {
            baseName = "SpacetimeDBSdk"
            isStatic = true
        }
    }

    jvm()

    sourceSets {
        commonMain.dependencies {
            implementation(libs.kotlinx.collections.immutable)
            implementation(libs.kotlinx.atomicfu)

            implementation(libs.ktor.client.core)
            implementation(libs.ktor.client.websockets)
        }

        jvmMain.dependencies {
            implementation(libs.brotli.dec)
        }

        androidMain.dependencies {
            implementation(libs.brotli.dec)
        }

        commonTest.dependencies {
            implementation(libs.kotlin.test)
            implementation(libs.kotlinx.coroutines.test)
        }

        jvmTest.dependencies {
            implementation(libs.ktor.client.okhttp)
        }

        all {
            languageSettings {
                optIn("kotlin.uuid.ExperimentalUuidApi")
                optIn("com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.InternalSpacetimeApi")
            }
        }

        compilerOptions.freeCompilerArgs.add("-Xexpect-actual-classes")
    }
}
