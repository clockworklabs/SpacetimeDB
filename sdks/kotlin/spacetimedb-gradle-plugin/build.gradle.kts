plugins {
    alias(libs.plugins.kotlinJvm)
    `java-gradle-plugin`
}

group = "com.clockworklabs"
version = "0.1.0"

kotlin {
    jvmToolchain(21)
}

dependencies {
    compileOnly("org.jetbrains.kotlin:kotlin-gradle-plugin:${libs.versions.kotlin.get()}")
}

gradlePlugin {
    plugins {
        create("spacetimedb") {
            id = "com.clockworklabs.spacetimedb"
            implementationClass = "com.clockworklabs.spacetimedb.SpacetimeDbPlugin"
        }
    }
}
