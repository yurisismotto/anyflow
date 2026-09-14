pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "AnyFlow"
include(":app")

// The notifications.v1 hardware fixture. **Test only, and structurally so:**
// `:app` does not depend on it, so it cannot reach the AnyFlow APK. It is
// built and installed by hand for a certification run and uninstalled after.
// See fixture/build.gradle.kts for why it is a module rather than a debug
// source set.
include(":fixture")
