import com.google.protobuf.gradle.id
import com.google.protobuf.gradle.proto

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.protobuf)
}

android {
    namespace = "io.github.yurisismotto.anyflow"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.yurisismotto.anyflow"
        // API 29 (Android 10) is the floor: it is where TLS 1.3 is enabled by
        // default and where SSLParameters.setApplicationProtocols (ALPN)
        // became available. Below that we could not speak the protocol at all.
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }

    testOptions {
        unitTests {
            // `android.util.Log` is a stub on the unit-test classpath and
            // throws by default. Returning a default instead lets production
            // code keep its logging while the domain logic is tested on the
            // JVM. It is not a licence to test Android behaviour here: where
            // the platform's own behaviour is the subject — ClipboardManager,
            // the Keystore — the test is instrumented and runs on a device.
            isReturnDefaultValues = true
        }
    }

    sourceSets {
        getByName("main") {
            // Single source of truth: the same .proto files the Rust daemon
            // compiles. Neither side can drift from the other.
            proto { srcDir("../../protocol/proto") }
        }
        getByName("test") {
            // Same idea for the cross-language fixtures: the unit tests read
            // the very certificates the Rust suite reads, so the two
            // implementations cannot quietly disagree about what an identity
            // fingerprint is. Regenerate with:
            //   cargo run -p anyflow-core --example gen_test_vectors
            resources.srcDir("../../protocol/testdata")
            // Same idea for the design tokens: DesignTokensTest reads the very
            // file the desktop's own token test reads, so the two front ends
            // cannot quietly disagree about what "connected teal" is.
            resources.srcDir("../../docs/design")
        }
    }
}

protobuf {
    protoc { artifact = libs.protobuf.protoc.get().toString() }
    generateProtoTasks {
        all().forEach { task ->
            task.builtins {
                // "lite" keeps the generated code small and reflection-free,
                // which matters for an app that must stay tiny and start fast.
                id("java") { option("lite") }
            }
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.service)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.protobuf.javalite)
    implementation(libs.zxing.embedded)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.json)

    // Instrumented tests. The Keystore regression can only be proved on a
    // real device: the whole failure was the TEE refusing an operation, and
    // no JVM stand-in has a TEE to refuse it.
    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
