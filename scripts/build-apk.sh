#!/usr/bin/env bash
# lowdeck-phone release APK: cargo-ndk → aapt2 link → .so ekle → zipalign → apksigner.
# Gerekli env: ANDROID_HOME, KS (keystore yolu), KS_PASS. İsteğe bağlı: BUILD_TOOLS (34.0.0), OUT.
# Java eklenince: javac --release 8 + d8 --min-api 29 → $STAGE/classes.dex, aşağıdaki zip adımına eklenir.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
: "${ANDROID_HOME:?ANDROID_HOME yok}" "${KS:?KS yok}" "${KS_PASS:?KS_PASS yok}"
BT="$ANDROID_HOME/build-tools/${BUILD_TOOLS:-34.0.0}"
OUT="${OUT:-target/apk/lowdeck-phone.apk}"
WORK="target/apk/work"
STAGE="$WORK/stage"

# Sürüm tek kaynaktan: [workspace.package] version. versionCode cargo-apk ile aynı formül
# (1<<24 | major<<16 | minor<<8 | patch), telefonda üstüne güncelleme bozulmasın.
VER=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
IFS=. read -r MA MI PA <<<"${VER%%[-+]*}"
CODE=$(( 1<<24 | MA<<16 | MI<<8 | PA ))

cargo ndk -t arm64-v8a build -p lowdeck-phone --release --lib

rm -rf "$WORK"; mkdir -p "$STAGE/lib/arm64-v8a" "$(dirname "$OUT")"
cp target/aarch64-linux-android/release/liblowdeck_phone.so "$STAGE/lib/arm64-v8a/"

"$BT/aapt2" link -o "$WORK/base.apk" \
  -I "$ANDROID_HOME/platforms/android-34/android.jar" \
  --manifest apps/lowdeck-phone/android/AndroidManifest.xml \
  --version-code "$CODE" --version-name "$VER"

(cd "$STAGE" && zip -q -r ../base.apk lib)
"$BT/zipalign" -f -p 4 "$WORK/base.apk" "$WORK/aligned.apk"
"$BT/apksigner" sign --ks "$KS" --ks-pass env:KS_PASS --ks-key-alias lowdeck \
  --out "$OUT" "$WORK/aligned.apk"
"$BT/apksigner" verify "$OUT"
echo "APK: $OUT ($VER, versionCode $CODE)"
