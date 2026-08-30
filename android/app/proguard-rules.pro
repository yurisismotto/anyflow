# protobuf-javalite generated classes are accessed reflectively by the
# runtime's own machinery; stripping them breaks parsing at runtime only,
# which is exactly the kind of bug that escapes testing.
-keep class dev.fedroid.bridge.proto.** { *; }
-keepclassmembers class * extends com.google.protobuf.GeneratedMessageLite {
    <fields>;
}
