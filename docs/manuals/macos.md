# macOS client

The `DYApp` client for macOS is a universal app (Apple silicon and Intel) for macOS 12 or later.
This first version is a minimal shell: it starts, renders and quits. Public profile publication,
search and chats are planned.

After creating or reopening an identity without an active network, the connection form opens
automatically. An identity with an active network reaches Ready; **Network connection** opens
the form again. If the active file cannot be read, the app shows an error with Retry and keeps
the identity intact. The form accepts bootstrap nodes
(one address per line) and an optional network name. **Save draft** keeps edits for this
identity across restarts without changing the active connection.
**Check connection** saves the draft and shows an outcome beside each address; it does not
activate the network until you click **Continue** after a compatible result. Cancel stops a running
check; Retry reuses the draft. Continue opens the profile editor after activation succeeds.

The public profile draft editor has nine optional pickers; **Not published** leaves each one
unset. Birth year, month and day are also independent optional fields. **Save draft** checks any
supplied combination with the shared profile rules and stores it beside this identity; an invalid
date leaves the prior draft unchanged. Public preview and publication are planned.

**Settings** lists the device's identities, marks the current one, and opens its network form.
**Create another identity** makes a separate Peer ID only after you click it. **Switch to this
identity** reopens an existing key before selecting it; an unreadable identity stays unavailable.
A failed Create or Switch shows an error and keeps your current selection. For a selected UUID
identity, **Remove this identity** shows its Peer ID and a backup warning; **Keep identity**
cancels. **Remove permanently** erases that local key and returns to setup or the identity picker.
Legacy identities cannot be removed here. In the picker, select a surviving Peer ID or create
another identity explicitly; an unreadable or deleting entry cannot be selected.

This project has no Apple Developer ID. The published `DYApp-unsigned.zip` is **not signed or
notarized**: it is the exact output of the native macOS CI build, which also runs the unit and UI
tests. A Mac does not open it as downloaded. An Apple Developer Program member can build and sign
a copy from the same tag; that signed copy is theirs, not an upstream release.

## Where the ZIP comes from

The scheduled `release-sizes` workflow builds Release on `macos-15` with
`CODE_SIGNING_ALLOWED=NO`, checks the bundle (`tools/scripts/package-apple-release.py`: both
architectures, executable, layout) and keeps the ZIP as the run's `apple-macos-unsigned` artifact.
Published release downloads with checksum and provenance are planned.

## Build from source

You need a Mac with Xcode 16, Rust 1.99 (`rustup`) and [XcodeGen](https://github.com/yonaskolb/XcodeGen)
2.46. Check out the released tag, then build the Rust core and generate the Xcode project:

```sh
git clone --branch v0.0.1 https://github.com/abbat/dyapp.git && cd dyapp
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash tools/scripts/build-apple-core.sh macos release
xcodegen generate --spec clients/macos/project.yml --project clients/macos
xcodebuild -project clients/macos/DYApp.xcodeproj -scheme DYApp -configuration Release \
    -destination generic/platform=macOS -derivedDataPath build \
    CODE_SIGNING_ALLOWED=NO ONLY_ACTIVE_ARCH=NO ARCHS="arm64 x86_64" build
```

The app is `build/Build/Products/Release/DYApp.app`. The project links the Rust core statically
and embeds no frameworks, so one signature covers the bundle.

## Run your own build

A local build carries no quarantine flag. To open it on the Mac that built it, sign it ad hoc:

```sh
codesign --force --sign - build/Build/Products/Release/DYApp.app
open build/Build/Products/Release/DYApp.app
```

The unsigned ZIP from CI can be run the same way after
`xattr -dr com.apple.quarantine DYApp.app`. Only do that if the ZIP's SHA-256 matches the
published one: nobody vouches for these bytes except the CI run.

## Sign and notarize for other Macs

For other Macs, an Apple Developer Program member signs the app with their own Developer ID and has
Apple notarize it ([Apple: notarizing macOS software](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)):

```sh
app=build/Build/Products/Release/DYApp.app
codesign --force --options runtime --timestamp \
    --sign "Developer ID Application: <Name> (<TEAMID>)" "$app"
codesign --verify --strict --verbose=2 "$app"
ditto -c -k --keepParent "$app" DYApp.zip
xcrun notarytool store-credentials dyapp --apple-id <id> --team-id <TEAMID>  # once
xcrun notarytool submit DYApp.zip --keychain-profile dyapp --wait
xcrun stapler staple "$app"
spctl --assess --type execute --verbose "$app"  # accepted, source=Notarized Developer ID
ditto -c -k --keepParent "$app" DYApp.zip       # the stapled copy to distribute
```

Then copy the ZIP to a Mac that has never seen the app, open it, and check that it starts and
quits. Publish that copy under your own name: its signature is yours.
