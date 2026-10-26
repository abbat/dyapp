# Windows desktop client

The `dyapp` client for Windows is a portable ZIP with one EXE and no installer. This first version
is a minimal shell: it starts, renders and closes. Profiles, search and chats in the window are
planned.

In an open identity, **Settings** lists local identities. **Add a new identity** opens the
key-storage choice, and **Switch or add an identity** opens the identity picker. Creating an
identity still requires an explicit storage choice and shows the file-key backup warning.
**Remove this identity** opens the same second-confirmation screen; Cancel returns to Settings.
Settings also displays the current identity's bootstrap-address editor and connection status;
Back returns the same editor to Ready with its edits intact.

## Build

The scheduled `release-sizes` workflow builds the release EXE on `windows-2022`, packs it with
`tools/scripts/package-windows-zip.py` into `dyapp-<version>-x86_64-windows.zip` (`dyapp.exe` and
`LICENSE` in one folder), extracts that ZIP and opens and closes the window. The ZIP is the run's
`windows-zip` artifact. Published release downloads are planned.

## Run

1. Check the ZIP's SHA-256 against the published one: `Get-FileHash dyapp-<version>-x86_64-windows.zip`.
2. Extract it anywhere, for example to `%LOCALAPPDATA%\Programs\`.
3. Run `dyapp.exe`.

The window uses the Microsoft Edge WebView2 Runtime. Windows 11 ships it, and Windows 10 gets it
with updates. If the window does not open and WebView2 is missing (it is not in "Apps"), install
the Evergreen Runtime from Microsoft's
[WebView2 page](https://developer.microsoft.com/microsoft-edge/webview2/).

The EXE is not code-signed yet, so SmartScreen may say "Windows protected your PC" on the first
run. Choose "More info", then "Run anyway", only for a ZIP whose checksum matched. Code signing is
planned.

## Remove

Delete the extracted folder. WebView2 keeps its cache under `%LOCALAPPDATA%\com.dyapp`; delete
that folder too to remove everything.
