plugins {
    id("com.android.application")
    kotlin("android")
}

android {
    namespace = "dev.zcode.gpui.mobile"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.zcode.gpui.mobile"
        minSdk = 26
        // 35+ forces edge-to-edge (no setDecorFitsSystemWindows opt-out), which
        // would need the UI to pad for system bar and keyboard insets itself.
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

dependencies {
    implementation("androidx.games:games-activity:4.4.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.core:core-ktx:1.13.1")
}
