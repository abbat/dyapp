# Your privacy in dyapp

A one-minute version of [Privacy & Metadata Visibility](../security/privacy.md). A Russian
translation is in [privacy.ru.md](privacy.ru.md). dyapp does not make you anonymous: it keeps
your messages unreadable to everyone but the chat, and it keeps your profile public on purpose.

## Your profile is public

Everything you publish in your profile can be read by anyone, not only people who find you in
search. Nodes keep copies, so a published field cannot be reliably taken back.

- Fields you leave empty are not published.
- Your place is a city or district name, never coordinates, and you can leave it out.
- Your Peer ID, IP address, age and place together can identify you. Publish only what you are
  ready to show a stranger.

**Hide from search** takes your profile out of new search results. It is a pause, not deletion:
anyone who already knows your Peer ID can still open the profile and write to you.

## Your messages

Messages are end-to-end encrypted: only the devices in the chat can read them. A node that
carries a message sees when it was sent, its rough size, which mailbox it goes to and the
sender's IP address, but not its text. Your internet provider or Wi-Fi sees only that you
connect to a node.

A node keeps a message until your device collects it, usually no longer than 24 hours; a chat
attachment stays for 7 days by default. Deleting a message does not remove a copy that a
recipient's offline device has not collected yet, and nobody can erase backups or screenshots.

The Linux and Android apps send a chat photo as a new JPEG, without the location and camera data
of the original file.

## What a node knows about you

A node is a server someone runs for the network. It sees your IP address whenever your app
connects and learns roughly when you were last active. If you keep several profiles, a node can
link them through your IP address.

A node drops your profile 30 days after you were last active. The operator may delete data
earlier; it may also keep backups.

## What stays on your device

Contacts, chats, saved profiles, private notes and muted peers live in the app on your device.
Saving a profile or muting someone tells them nothing; your other devices receive these changes,
encrypted. The local chat store is not encrypted, so protect the device itself with a
screen lock.

On Android and iOS the app checks for mail in the background while it is closed. The app lock
covers only the screens: the background check still opens your identity key and contacts nodes.

## Notifications

By default a notice says only "New message", so a lock screen shows neither a name nor text.
Settings can add the sender's name (a short Peer ID on Android) or also the first 200 characters
of the message; then they pass through the system notification service and may show on the lock
screen. A message request or a like never adds a stranger's words.

## More detail

- [Privacy & Metadata Visibility](../security/privacy.md): every field, who sees it and for
  how long.
- [Threat model](../security/threat-model.md): attacks and what stops them.
