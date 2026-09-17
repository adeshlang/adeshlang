plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "com.editor.adesh"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = "com.editor.adesh"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

tasks.register("checkAdeshBridgeLibs") {
    doLast {
        val jniDir = file("src/main/jniLibs")
        val arm64So = file("src/main/jniLibs/arm64-v8a/libadesh_mobile_bridge.so")
        val x8664So = file("src/main/jniLibs/x86_64/libadesh_mobile_bridge.so")
        if (!arm64So.exists() && !x8664So.exists()) {
            logger.warn("[WARNING] AdeshLang Mobile Bridge libraries (.so) not found in ${jniDir.absolutePath}. Run `mobile/build_bridge.bat` (or `build_bridge.ps1`/`build_bridge.sh`) to compile the native Rust bridge.")
        }
    }
}

tasks.matching { it.name.startsWith("merge") && it.name.endsWith("JniLibFolders") }.configureEach {
    dependsOn("checkAdeshBridgeLibs")
}
