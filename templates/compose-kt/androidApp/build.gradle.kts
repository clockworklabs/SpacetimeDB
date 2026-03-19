plugins {
    alias(libs.plugins.androidApplication)
    alias(libs.plugins.composeCompiler)
}

android {
    namespace = "com.clockworklabs.spacetimedb_compose_kt"
    compileSdk {
        version = release(libs.versions.android.compileSdk.get().toInt())
    }

    defaultConfig {
        applicationId = "com.clockworklabs.spacetimedb_compose_kt"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.targetSdk.get().toInt()
        versionCode = 1
        versionName = "1.0"
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

dependencies {
    implementation(projects.sharedClient)
    implementation(libs.androidx.activity.compose)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.ktor.client.websockets)
}
