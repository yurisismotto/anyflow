# AnyFlow notification fixture — **test only**

A one-activity Android app that puts a notification of a chosen shape on a
device's shade, on demand, from `adb`. It exists so that the `notifications.v1`
hardware gates can be run without depending on `com.android.shell`.

**It is never part of the AnyFlow APK.** It is a separate Gradle module with its
own `applicationId`, and nothing depends on it — `:app` does not, and
`settings.gradle.kts` includes it beside `:app` rather than underneath it. That
is a structural guarantee, not a convention; `unzip -l app-debug.apk | grep
fixture` returns nothing.

## Why it exists

| Route | What it cannot do |
| --- | --- |
| `cmd notification post` (posts as `com.android.shell`) | no launcher entry, so AnyFlow's app picker cannot see it until it is *already* notifying — which needs the listener bound, which needs a granted peer connected (N3 debt 3); no `cancel`; no ongoing; no group; no progress; no tag control (N4 debt 2) |
| a real third-party app | not deterministic, and not something a certification run may install on somebody's device |

The launcher entry is the part that retires N3 debt 3: AnyFlow's picker offers
apps a person can open from their home screen, plus apps that happen to be
notifying right now. A fixture with a launcher icon is visible in the picker
*before* it has posted anything, so the allow-list can be set up first and the
notification posted second — which is the order a person actually works in.

## What it holds, and what it does not

```
uses-permission: android.permission.POST_NOTIFICATIONS
```

That is the whole list. **No `INTERNET`**, so there is nowhere for anything it
is told to display to go. No storage, no contacts, no location, no camera, no
microphone, no `QUERY_ALL_PACKAGES`, no listener service, no foreground
service, no boot receiver.

`FixtureActivity` is `exported`, because `adb shell am start` runs as the shell
user and cannot reach a component that is not. That is a real surface and it is
worth stating plainly: any app on the device can make this fixture post a
notification with text of its choosing. What it cannot do is make it post *as*
anybody else, reach the network, or read anything — so the surface is
equivalent to the caller posting its own notification, which needs no help from
here. Install it for a certification run and uninstall it after.

## Install

```console
cd android
JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk \
  ./gradlew :fixture:assembleDebug

adb install -r fixture/build/outputs/apk/debug/fixture-debug.apk
adb shell pm grant io.github.yurisismotto.anyflow.fixture \
  android.permission.POST_NOTIFICATIONS
```

Uninstall with `adb uninstall io.github.yurisismotto.anyflow.fixture`.

## Invoke

```console
FX=io.github.yurisismotto.anyflow.fixture/.FixtureActivity

# a plain, clearable notification
adb shell "am start -n $FX --es op post \
  --es id 1 --es tag n5 --es title 'Ana' --es body 'lunch'"

# update it in place — same id and tag, so the same identity
adb shell "am start -n $FX --es op post \
  --es id 1 --es tag n5 --es title 'Ana' --es body 'lunch-at-one'"

# remove it
adb shell "am start -n $FX --es op remove --es id 1 --es tag n5"

# ONGOING: isClearable() is false, which is what the dismissal contract turns on
adb shell "am start -n $FX --es op ongoing \
  --es id 2 --es tag n5ong --es title 'Playing' --es body 'a-track'"

# a grouped child plus its summary (two StatusBarNotifications)
adb shell "am start -n $FX --es op group --es id 3 --es tag n5grp"

# progress, and a notification that expires by itself
adb shell "am start -n $FX --es op progress --es id 4 --es progress 40"
adb shell "am start -n $FX --es op timeout --es id 5 --es timeout_ms 4000"

# everything this fixture put up, gone
adb shell "am start -n $FX --es op clear"
```

**Quote the whole `am start` command** as above and avoid spaces inside extra
values. `adb shell` re-splits the command line, and an unquoted
`--es body 'two words'` makes `am` read the second word as a package name —
the intent still launches, but with the extra silently mangled.

`op` accepts: `post` · `update` (a synonym for `post`) · `remove` / `cancel` ·
`ongoing` / `nonclearable` · `group` · `progress` · `timeout` · `clear`.

Every operation writes one line to logcat under the tag `AnyFlowFixture`,
carrying the op, the id and the tag — **never** the title or the body. The
fixture is used in the same runs as the NOTIF-SEC-25 logging canaries, and a
fixture that logged its own payload would fail them itself.

## Non-clearable, precisely

`setOngoing(true)` sets `FLAG_ONGOING_EVENT`, and
`StatusBarNotification.isClearable()` is false whenever that flag or
`FLAG_NO_CLEAR` is set. `isClearable()` is exactly what AnyFlow's source
consults before honouring a `DismissRequest`, so `op=ongoing` drives the real
gate rather than an approximation of it.

What it is *not* is a promise about the shade. Since Android 14 a person can
usually swipe away an ongoing notification posted by an app that is not running
a foreground service; that is a platform UI behaviour and it does not change
`isClearable()`. Producing a notification the *user* cannot remove would need a
foreground service, which needs `FOREGROUND_SERVICE` and a service type —
permissions this fixture deliberately does not hold, for a distinction the code
under test does not make.
