plugins {
    alias(libs.plugins.kotlinJvm)
    `java-gradle-plugin`
}

group = "com.clockworklabs"
version = "0.1.0"

gradlePlugin {
    plugins {
        create("spacetimedb") {
            id = "com.clockworklabs.spacetimedb"
            implementationClass = "com.clockworklabs.spacetimedb.SpacetimeDbPlugin"
        }
    }
}
