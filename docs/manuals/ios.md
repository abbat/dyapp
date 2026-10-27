# iOS client

The `DYApp` client for iOS runs on iOS 15 or later. This first version is a minimal shell: it
starts, renders and closes. Public profile publication, search and chats are planned.

After creating or reopening an identity without an active network, the connection form opens
automatically. An identity with an active network reaches Ready; **Network connection** opens
the form again. If the active file cannot be read, the app shows an error with Retry and keeps
the identity intact. The form accepts bootstrap nodes
(one address per line) and an optional network name. **Save draft** keeps edits for this
identity across restarts without changing the active connection.
**Check connection** saves the draft and shows an outcome beside each address; it does not
activate the network until you tap **Continue** after a compatible result. Cancel stops a running
check; Retry reuses the draft. Continue opens the profile editor after activation succeeds.

The public profile draft editor has nine optional pickers; **Not published** leaves each one
unset. Birth year, month and day are also independent optional fields. **Save draft** checks any
supplied combination with the shared profile rules and stores it in this identity's `client.db`; an invalid
date leaves the prior draft unchanged. **Preview public profile** reads the saved draft, shows
only selected public values, the exact birth parts and possible age, and warns that others may
keep copies after deletion. A full date has an additional warning. Publication is planned.
The editor also shows separate 18+ and public-data confirmations and the eligibility result for
the saved draft. Changing or saving any field clears both confirmations. They are never needed to
save or preview. Unsaved edits disable confirmation until saved; the publish action is planned.

**Settings** lists the device's identities, marks the current one, and opens its network form.
**Create another identity** makes a separate Peer ID only after you tap it. **Switch to this
identity** reopens an existing key before selecting it; an unreadable identity stays unavailable.
A failed Create or Switch shows an error and keeps your current selection. For a selected UUID
identity, **Remove this identity** shows its Peer ID and a backup warning; **Keep identity**
cancels. **Remove permanently** erases that local key and returns to setup or the identity picker.
Legacy identities cannot be removed here. In the picker, select a surviving Peer ID or create
another identity explicitly; an unreadable or deleting entry cannot be selected.

This project has no Apple Developer Program account. The published `DYApp-unsigned.ipa` is **not
signed**, and iOS installs only signed apps, so nobody can install it as published. It is the exact
output of the native CI build (`release-sizes` on `macos-15`, artifact `apple-ios-unsigned`); the
CI test run (`ci.yml`) also runs the unit and UI tests on the iOS Simulator. To use the app, build
and sign it with your own Apple ID from the same tag. That signed copy is yours, not an upstream
release. Published release downloads with checksum and provenance are planned.

## Build from source

You need a Mac with Xcode 16, Rust 1.99 (`rustup`) and [XcodeGen](https://github.com/yonaskolb/XcodeGen)
2.46. Check out the released tag, then build the Rust core and generate the Xcode project:

```sh
git clone --branch v0.0.1 https://github.com/abbat/dyapp.git && cd dyapp
rustup target add aarch64-apple-ios
bash tools/scripts/build-apple-core.sh ios-device release
xcodegen generate --spec clients/ios/project.yml --project clients/ios
```

## Install on your own iPhone

The simplest path is Xcode: open `clients/ios/DYApp.xcodeproj`, select the `DYApp` target, and in
"Signing & Capabilities":

- turn on "Automatically manage signing" and pick your team. A free Apple ID works too; its
  profile expires after 7 days;
- set a bundle identifier of your own, for example `com.<you>.dyapp`.

The project sets `CODE_SIGNING_ALLOWED = NO` so CI builds without an account: under "Build
Settings", set it to `Yes` for the `DYApp` target. Connect the iPhone, enable Developer Mode on it
(Settings → Privacy & Security), choose it as the run destination and press Run.

## Sign for several devices

A paid Apple Developer Program member can archive the app and export it for the devices
registered in their account ([Apple: distributing to registered devices](https://developer.apple.com/documentation/xcode/distributing-your-app-to-registered-devices)):

```sh
xcodebuild -project clients/ios/DYApp.xcodeproj -scheme DYApp -configuration Release \
    -destination generic/platform=iOS -archivePath build/DYApp.xcarchive \
    -allowProvisioningUpdates CODE_SIGNING_ALLOWED=YES CODE_SIGN_STYLE=Automatic \
    DEVELOPMENT_TEAM=<TEAMID> PRODUCT_BUNDLE_IDENTIFIER=com.<you>.dyapp archive
xcodebuild -exportArchive -archivePath build/DYApp.xcarchive -exportPath build/export \
    -exportOptionsPlist options.plist -allowProvisioningUpdates
```

Here `options.plist` is a property list with `method` set to `release-testing` and `teamID` set to
your team. Install the resulting `build/export/DYApp.ipa` with Finder or Apple Configurator, then
check that the app starts and closes on each device.
