import java.time.LocalDate
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val buildDate: String = LocalDate.now().toString()
val localProperties = Properties().apply {
    val file = rootProject.file("local.properties")
    if (file.exists()) {
        file.inputStream().use { input -> load(input) }
    }
}

fun signingProperty(localName: String, envName: String): String? {
    return localProperties.getProperty(localName)
        ?: providers.environmentVariable(envName).orNull
}

val releaseStoreFile = signingProperty("android.injected.signing.store.file", "ANDROID_KEYSTORE_PATH")
val releaseStorePassword = signingProperty("android.injected.signing.store.password", "ANDROID_KEYSTORE_PASSWORD")
val releaseKeyAlias = signingProperty("android.injected.signing.key.alias", "ANDROID_KEY_ALIAS")
val releaseKeyPassword = signingProperty("android.injected.signing.key.password", "ANDROID_KEY_PASSWORD")
val hasReleaseSigning = listOf(
    releaseStoreFile,
    releaseStorePassword,
    releaseKeyAlias,
    releaseKeyPassword,
).all { !it.isNullOrBlank() }

android {
    namespace = "dev.clawseed.demo"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.clawseed.demo"
        minSdk = 26
        targetSdk = 36
        versionCode = 8
        versionName = "2.0"
        buildConfigField("String", "BUILD_DATE", "\"$buildDate\"")
        buildConfigField("String", "SDK_VERSION", "\"0.4.0\"")
    }

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = rootProject.file(releaseStoreFile!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (hasReleaseSigning) {
                signingConfig = signingConfigs.getByName("release")
            } else {
                logger.warn("Release signing is not configured; release APK will be unsigned.")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        // Extract libclawseed.so to nativeLibraryDir so we can exec() it as a process.
        // Without this, AGP leaves .so files uncompressed inside the APK (Android 6+ default),
        // and ProcessBuilder cannot launch a binary from inside a zip file.
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

dependencies {
    implementation(project(":sdk:embedded"))

    implementation("androidx.core:core-ktx:1.16.0")
    implementation("androidx.activity:activity-compose:1.10.1")

    val composeBom = platform("androidx.compose:compose-bom:2026.04.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    debugImplementation("androidx.compose.ui:ui-tooling")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.9.0")

    // ViewModel
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.0")

    // Material Icons (core only to control APK size)
    implementation("androidx.compose.material:material-icons-core")


    // Coroutines for ClawseedService background work
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    // OkHttp for REST API
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // DataStore (本地持久化)
    implementation("androidx.datastore:datastore-preferences:1.1.4")

    // kotlinx-serialization (for tool JSON building)
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

}
