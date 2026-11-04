# iOS client

The `DYApp` client for iOS runs on iOS 15 or later. It covers identities, the network connection,
the public profile, search, the Chats lists and the chat screen with message sending.

**Chats** on the Ready screen lists Liked you, message requests and conversations of the current
identity. Counts come from local data, and the app checks the mailbox once a minute while it runs.
A row opens its chat; **Chats** at the top of the chat goes back. A first message waits for a reply
before you can write more. Text that could not go out yet stays queued and is not sent twice. The
**...** button beside your own message edits it, or deletes it for both sides after a confirmation.
The menu at the top right of a chat names the person on this device, compares the safety number, clears
this device's copy, mutes notifications and blocks the person. **Compare safety number** shows the 60
digits and the QR code of the pair to check with that person; this app does not scan a code or record
a match yet. A name you gave shows on the chat and its row; without one, the
person's public name shows with its mark and the short Peer ID. A request has its **Block** under the notice. After a block, Chats says whether the node has
revoked that person's access yet.

A Liked you row, a search result and the name at the top of a chat open that person's page. **Like**
sends a like and a mutual like shows a match; **Message** opens the chat. **Share** sends the page to
one of your open chats, where it shows as a profile button that opens the page. **Liked** takes the like back
after a confirmation. **Pass** hides the person from Liked you and can be undone on the page, and
**Block** asks first and closes the page.

**Search** on the Ready screen asks the search nodes of the active network for a sample of
published profiles. A sample is not a complete list, and the app lists only profiles whose signature
and filters it verified. After the first sample, filters appear for the fields the nodes offer: age,
height, country and place, **Only people looking for someone like me** (it needs your gender and a
birth date that gives your exact age in the saved profile) and more values. Each result shows the end
of the Peer ID, the possible age and the country. Opening a result shows its full Peer ID and public
fields with the same actions, and **Back to results** returns to the same sample without a new query. When some nodes fail,
the matches found stay and each node problem is listed.

After creating or reopening an identity without an active network, the connection form opens
automatically. An identity with an active network reaches Ready; **Network connection** opens
the form again. If the active file cannot be read, the app shows an error with Retry and keeps
the identity intact. The form accepts bootstrap nodes
(one address per line) and an optional network name. **Save draft** keeps edits for this
identity across restarts without changing the active connection.
**Check connection** saves the draft and shows an outcome beside each address; it does not
activate the network until you tap **Continue** after a compatible result. Cancel stops a running
check; Retry reuses the draft. Continue opens the profile editor after activation succeeds.

The public profile draft editor has optional pickers; **Not published** leaves each one
unset. Name, height, about, who you are looking for (genders and partner age) and the **Not
looking now** switch are optional too, as are birth year, month and day. **Save draft** checks any
supplied combination with the shared profile rules and stores it in this identity's `client.db`; an invalid
value shows what to fix and leaves the prior draft unchanged. **Preview public profile** reads the saved draft, shows
only selected public values, the exact birth parts and possible age, and warns that others may
keep copies after deletion. A full date has an additional warning.
The editor also shows separate 18+ and public-data confirmations and the eligibility result for
the saved draft. Changing or saving any field clears both confirmations. They are never needed to
save or preview. Unsaved edits disable confirmation until saved. After both confirmations and the
eligibility check, **Publish profile** runs the shared proof in short saved steps, then publishes
through the active network. **Cancel preparation** keeps the draft and progress. On success the
screen shows the verified version and exact public fields; after failure, repeat both confirmations
and retry. After a publication, **Hide from search** takes the profile out of search and keeps the
draft; publishing again shows it. Anyone with your Peer ID can still open it, and fetched copies remain.

**Settings** lists the device's identities, marks the current one, and opens its network form.
Once the network is set up it also holds **Send read receipts** and the **Blocked people** and
**Passed people** lists: each row undoes the block or the pass, and a blocked row can retry a
pending token revocation.
**Updates** shows the check interval, the last check and that updates for this platform are not
published yet; its controls stay disabled and it sends no request.
**Create another identity** makes a separate Peer ID only after you tap it. **Switch to this
identity** reopens an existing key before selecting it; an unreadable identity stays unavailable.
A failed Create or Switch shows an error and keeps your current selection. For a selected UUID
identity, **Remove this identity** shows its Peer ID and a backup warning; **Keep identity**
cancels. **Remove permanently** erases that local key and returns to setup or the identity picker.
A published profile is withdrawn from search first; if no node takes the withdrawal, choose **Retry**,
**Keep identity** or **Remove anyway**, and the profile then disappears from search within 30 days.
Legacy identities cannot be removed here. In the picker, select a surviving Peer ID or create
another identity explicitly; an unreadable or deleting entry cannot be selected.

This project has no Apple Developer Program account. The published
`dyapp-<version>-arm64-ios-unsigned.ipa` is **not signed**, and iOS installs only signed apps, so
nobody can install it as published. It is the exact output of the native CI build (`release` on
`macos-15`); the release waits for the unit and UI tests of the same tag on the iOS Simulator (never
a physical device). To use the app, build and sign it with your own Apple ID from the same tag. That
signed copy is yours, not an upstream release. The
[release page](https://github.com/abbat/dyapp/releases) carries the file with `SHA256SUMS` and build
provenance; no release has been published yet.

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
