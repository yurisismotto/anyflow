// The `notifications.v1` hardware fixture. **Test-only, and it must stay that
// way.**
//
// # Why this module exists
//
// Every hardware gate from N1 onwards has used `cmd notification post`, which
// posts as `com.android.shell`. That works for a clearable notification and
// fails at everything else:
//
//  * `com.android.shell` has no launcher entry, so OmniBridge's app picker can
//    only see it *while it is already notifying* — which needs the listener
//    bound, which needs a granted peer connected. N3 debt 3.
//  * `cmd notification` has no `cancel`, no `setOngoing`, no group, no
//    progress and no tag control, so the ongoing / non-clearable half of the
//    dismissal contract has never been proved on hardware at all. N4 debt 2.
//
// # Why a separate module rather than a debug source set
//
// A `debug`-only source set inside `:app` would share OmniBridge's package, its
// manifest, its permissions and its signing identity — and one day somebody
// would build a release with it. A separate module with a separate
// `applicationId` cannot end up in the OmniBridge APK, because nothing depends
// on it: `:app` does not, and `settings.gradle.kts` includes it beside `:app`
// rather than underneath it. That is a structural guarantee rather than a
// convention.
//
// # The permission list is the point
//
// This app holds **no** `INTERNET`, no storage, no contacts, no location, no
// camera and no microphone. It holds `POST_NOTIFICATIONS`, which is the one
// thing it exists to do. A fixture with a wide permission set would be a
// worse thing to leave installed on certification hardware than the gap it
// closes.

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "io.github.yurisismotto.omnibridge.fixture"
    compileSdk = 35

    defaultConfig {
        // Deterministic, and deliberately not a sub-package of the app's own
        // `applicationId`: two apps, two rows in the picker, no ambiguity
        // about which one a mirrored notification came from.
        applicationId = "io.github.yurisismotto.omnibridge.fixture"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            // Never published, never minified: a fixture that behaves
            // differently from the one the gates were written against is not
            // a fixture.
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(libs.androidx.core.ktx)
}
