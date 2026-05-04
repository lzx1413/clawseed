plugins {
    id("com.android.library")
    `maven-publish`
}

group = "dev.clawseed"
version = "0.1.0"

android {
    namespace = "dev.clawseed.sdk.embedded"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }
}

dependencies {
    api(project(":sdk:android"))
    implementation("androidx.core:core-ktx:1.16.0")
}

publishing {
    publications {
        register<MavenPublication>("release") {
            afterEvaluate {
                from(components["release"])
            }
            artifactId = "clawseed-embedded"
        }
    }
}
