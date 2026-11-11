# Windows desktop client

The `dyapp` client for Windows is a portable ZIP with one EXE and no installer. This first version
uses the shared desktop interface for identities, profiles, search and chats.

In an open identity, **Settings** lists local identities. **Add a new identity** opens the
key-storage choice, and **Switch or add an identity** opens the identity picker. Creating an
identity still requires an explicit storage choice and shows the file-key backup warning.
**Remove this identity** opens the same second-confirmation screen; Cancel returns to Settings.
Settings also displays the current identity's bootstrap-address editor and connection status;
Back returns the same editor to Ready with its edits intact.

## Optional node

Settings → Node can enable the store/search node while the app is open and set its disk and
connection limits. It is off by default. The node belongs to this installation: switching,
adding or removing identities leaves it running. Saving enabled node settings or explicitly
activating a network applies that network to the node. Relaunch restores its saved configuration;
Quit stops it. Its `node.json` settings and `node/` data sit outside identity folders.

## Share a profile

On a profile page, **Share** offers existing open chats, excluding the profile owner.
A failed send keeps the picker open for retry. The recipient sees a profile card;
opening it fetches the verified profile, where **Message** opens a chat.
Unavailable or locally blocked profiles show an error. Blocked profiles are checked
before the network request and again before displaying its result. If the active
network changes while the profile loads, retry the card on the new network.
A search still running on the previous network also fails without showing its results;
run it again on the active network.

## Save a profile privately

On a loaded profile page, **Save privately** adds it to this identity's saved state;
**Remove from saved** reverses it. This sends no like or notification to its owner.
Your own devices share the change. If saved status cannot be read, retry that read
before changing anything. **Search → Saved profiles** lists saved peers from this
device; **Refresh saved profiles** retries loading. A row fetches the verified
profile only when opened. An unavailable or deleted profile can still be removed.
Blocked peers are hidden from this list.

## Private contact notes

On a loaded profile, **Private note** is separate from the public profile. Enter up
to 2000 characters; case and line breaks are kept. **Save note** stores the change
for this identity and shares it only with your own devices when connected. The
person receives no note, like or notification. **Clear saved note** reverses a saved
note. Failed writes keep your text for retry; failed reads require **Retry loading
note** before editing. Save before leaving the profile: unsaved edits are discarded
on Back or identity change.

## Sticker packs

In Settings → Stickers → Saved packs, choose **Import TAR…** to save a local pack
(up to 8 MiB). Importing sends nothing. Refresh restarts the list; Next page shows
more packs. The 300 MiB limit never removes a saved pack automatically. Import/read errors keep
existing packs; open Details and retry. **Remove…** next to a pack asks for confirmation;
Cancel or Escape changes nothing. Removal affects only this saved copy and also works for
unreadable packs. After an uncertain error, refresh before retrying. Sending remains planned.

Sticker history is prepared to show emoji and pack name without downloading media.
Image display, chat delivery and Add pack remain planned.

## Interests

Enter **Interests** with one tag per line (up to 32 tags, 64 characters each).
Saving shows their canonical lowercase spelling; clearing all lines removes them.
Interests appear in the public preview and profile page. When the connected search
providers support it, **Interest** filters by one tag, ignoring case and surrounding
spaces; leave it empty for no interest filter.

## Build

The scheduled `release-sizes` workflow builds the release EXE on `windows-2022`, packs it with
`tools/scripts/package-windows-zip.py` into `dyapp-<version>-x86_64-windows.zip` (`dyapp.exe` and
`LICENSE` in one folder), extracts that ZIP and opens and closes the window. The ZIP is the run's
`windows-zip` artifact.

## Download

A tagged release builds the same ZIP and publishes it on the
[release page](https://github.com/abbat/dyapp/releases) with `SHA256SUMS` and build provenance.
No release has been published yet. Download `dyapp-<version>-x86_64-windows.zip` and
`SHA256SUMS` into one folder and check them in PowerShell:

```powershell
$zip = 'dyapp-0.0.1-x86_64-windows.zip'
$want = (Select-String -Path SHA256SUMS -Pattern " $zip$").Line.Split(' ')[0]
(Get-FileHash $zip -Algorithm SHA256).Hash.ToLower() -eq $want
gh attestation verify $zip --repo abbat/dyapp
```

The third line must print `True`, and `gh attestation verify` must report that the file was built
by this repository's `release` workflow. The [GitHub CLI](https://cli.github.com/) is only needed
for that last check.

## Run

1. Check the ZIP as in [Download](#download).
2. Extract it anywhere, for example to `%LOCALAPPDATA%\Programs\`.
3. Run `dyapp.exe`.

The window uses the Microsoft Edge WebView2 Runtime. Windows 11 ships it, and Windows 10 gets it
with updates. If the window does not open and WebView2 is missing (it is not in "Apps"), install
the Evergreen Runtime from Microsoft's
[WebView2 page](https://developer.microsoft.com/microsoft-edge/webview2/).

The EXE is not code-signed yet, so SmartScreen may say "Windows protected your PC" on the first
run. Choose "More info", then "Run anyway", only for a ZIP whose checksum matched. Code signing is
planned.

## Notifications

While the app runs, also with the window hidden, new messages and likes from people you did not
mute post one toast unless the window has focus. A new one replaces the last, and a click on it
shows the window. By default it says only "New message"; Settings can turn it off or add the name,
or the name and the message, per identity. Windows settings decide whether toasts show and whether
they appear on the lock screen. So that Windows shows toasts from a folder app, each launch writes
the sender name under `HKCU\Software\Classes\AppUserModelId\com.dyapp`.

## Remove

Delete the extracted folder. WebView2 keeps its cache under `%LOCALAPPDATA%\com.dyapp`; delete
that folder too, and the `HKCU\Software\Classes\AppUserModelId\com.dyapp` registry key, to
remove everything.
