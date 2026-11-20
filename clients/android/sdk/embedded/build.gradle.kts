plugins {
    id("com.android.library")
    `maven-publish`
}

group = "dev.clawseed"
version = "0.2.0"

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
    // Pure Java Ed25519 verifier; Android API 26 does not guarantee a native Ed25519 provider.
    implementation("org.bouncycastle:bcprov-jdk15to18:1.72")
    testImplementation("junit:junit:4.13.2")
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
