#!/bin/bash
# Signs Treehouse.app with the Developer ID certificate, has Apple notarize
# it and staples the ticket, so macOS opens the downloaded app without a
# warning, even offline. Called by release.yml with the app's path; the
# certificate and the App Store Connect API key come from repository secrets.
set -euo pipefail

app=$1
keychain=$RUNNER_TEMP/signing.keychain-db
keychain_password=$(uuidgen)

echo "$MACOS_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/certificate.p12"
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$RUNNER_TEMP/certificate.p12" -k "$keychain" \
  -P "$MACOS_CERTIFICATE_PASSWORD" -f pkcs12 -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: \
  -s -k "$keychain_password" "$keychain" > /dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
rm "$RUNNER_TEMP/certificate.p12"

# The certificate's own identity, so its name needn't be a secret too.
identity=$(security find-identity -v -p codesigning "$keychain" \
  | awk '/Developer ID Application/ { print $2; exit }')
if [ -z "$identity" ]; then
  echo "::error::No Developer ID Application identity in MACOS_CERTIFICATE"
  exit 1
fi

# Notarization requires the hardened runtime and a secure timestamp.
codesign --force --options runtime --timestamp --sign "$identity" "$app"
codesign --verify --strict --verbose=2 "$app"

key=$RUNNER_TEMP/notary.p8
printf '%s\n' "$APPLE_API_KEY" > "$key"
ditto -c -k --keepParent "$app" "$RUNNER_TEMP/notarize.zip"
result=$(xcrun notarytool submit "$RUNNER_TEMP/notarize.zip" \
  --key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER" \
  --wait --output-format json)
echo "$result"
# notarytool exits 0 even when Apple rejects the app, so read the verdict.
id=$(echo "$result" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
status=$(echo "$result" | python3 -c 'import json,sys; print(json.load(sys.stdin)["status"])')
if [ "$status" != "Accepted" ]; then
  xcrun notarytool log "$id" --key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER"
  echo "::error::Notarization ended as $status"
  exit 1
fi
rm "$key"

xcrun stapler staple "$app"
xcrun stapler validate "$app"
