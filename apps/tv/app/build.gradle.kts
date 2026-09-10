plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}
android {
    namespace = "dev.madari.tv"
    compileSdk = 36
    defaultConfig {
        applicationId = "dev.madari.tv"
        minSdk = 26
        targetSdk = 36
        // Release Please rewrites the versionName line in its release PR. The
        // versionCode is derived from it so the two cannot disagree, and it
        // stays monotonic as Android requires (0.2.1 -> 201).
        versionName = "0.1.0" // x-release-please-version
        versionCode =
            requireNotNull(versionName)
                .substringBefore('-')
                .split('.')
                .fold(0) { code, part -> code * 100 + part.toInt() }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
    }
    buildFeatures { compose = true }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
    buildTypes {
        release { isMinifyEnabled = true; proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro") }
        create("performance") {
            initWith(getByName("release"))
            signingConfig = signingConfigs.getByName("debug")
            matchingFallbacks += "release"
        }
    }
}
val buildNative by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir.resolve("../.."))
    commandLine("bash", "scripts/build-tv-native.sh")
    inputs.files(fileTree(rootProject.projectDir.resolve("../../vendor/librqbit")) { include("**/*.rs", "Cargo.toml") })
    inputs.files(fileTree(rootProject.projectDir.resolve("../../crates")) { include("**/*.rs", "**/Cargo.toml", "**/web/*.html", "**/web/*.js", "**/web/*.css") }, rootProject.file("../../Cargo.lock"), rootProject.file("../../Cargo.toml"), rootProject.file("../../scripts/build-tv-native.sh"))
    outputs.dir("src/main/jniLibs/arm64-v8a")
    outputs.dir("src/main/jniLibs/armeabi-v7a")
    outputs.dir("libs")
}
tasks.named("preBuild") { dependsOn(buildNative) }
dependencies {
    implementation(files("libs/rustls-platform-verifier-0.1.1.aar"))
    implementation(platform("androidx.compose:compose-bom:2025.04.01"))
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.tv:tv-material:1.0.1")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.0")
    implementation("androidx.media3:media3-exoplayer:1.7.1")
    implementation("androidx.media3:media3-exoplayer-hls:1.7.1")
    implementation("androidx.media3:media3-exoplayer-dash:1.7.1")
    implementation("androidx.media3:media3-ui:1.7.1")
    implementation("androidx.media3:media3-session:1.7.1")
    implementation("io.coil-kt:coil-compose:2.7.0")
    implementation("io.coil-kt:coil-svg:2.7.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
    androidTestImplementation(platform("androidx.compose:compose-bom:2025.04.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
