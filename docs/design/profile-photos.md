# Profile photos

How a client publishes, fetches and keeps public profile photos. The wire format is in
[protobuf-schema.md](../architecture/protobuf-schema.md); node rules are in
[bootstrap.md](../architecture/bootstrap.md#signed-profile). `rust/client` implements storing and
publishing, and the desktop has the editor. The viewing below and the keep refresh of step 4 are
planned on every client, and the editor is planned on Android and Apple.

## Limits

| Limit | Value | Why |
|-------|-------|-----|
| Photos a client publishes | 6 | The schema allows 16; six fit the quota below with room for chats |
| Full image | One JPEG blob, at most 512 KiB, long side 1600 px, then 1280 px | Enough for a full-screen view; stays a whole-copy blob |
| Thumbnail | One JPEG blob, at most 24 KiB, long side at most 320 px | The existing `Photo.thumbnail` rule |
| Picked file | JPEG or PNG, at most 64 MiB and 64 megapixels | The bounds chat photos already use |

The node bills a whole-copy blob at five times its size, so a photo at the limit costs 2.6 MiB and
six cost 15.7 MiB. `limits.media_per_owner_mb` therefore defaults to 32: chat attachments share
the quota, and one sharded 6 MiB chat photo costs about 9 MiB. An operator may set less; the client
then shows the node's `FULL` reply and keeps the photo local.

## Publishing

1. The page decodes the picked file and draws it on a canvas, which writes a bare JPEG. EXIF, GPS,
   colour profiles and anything appended to the file never leave the computer, and a rotated
   photo comes out upright.
2. The core refuses anything but a JPEG within the size and pixel limits. It keeps both blobs in
   the local store under their SHA-256 hashes and lists them in the profile draft.
3. Publishing sends `keep` with every hash of the draft, puts each blob the node reports missing,
   then publishes the profile. A failure before the profile publish leaves the previous profile
   valid. A photo removed from the draft leaves the store and the keep list with that publish, so
   a reader of the older profile briefly sees the placeholder. Withdrawing the identity sends an
   empty keep after the tombstone.
4. The catch-up pass refreshes the keep with a newer signed `time`, at most once a day, and puts
   back whatever it reports missing, so photos outlive node churn while the owner is active.

`keep` uses the profile `version`. A node refuses a different list at an equal version, which a
failed publish can leave behind, so the client then moves one version past it. The store holds own
photos until a publish drops them, so a repair needs no original file. The bare JPEG and byte
limits are checked where the blob enters the store; the desktop checks pixels before that, because
`rust/client` has no image decoder.

## Viewing

Lists and results fetch thumbnails; `profile-view` fetches full images. `get_media` checks each
blob against its hash, and the core applies the size and pixel bounds before the page decodes it.
The page draws to a canvas, because the content security policy blocks `data:` and `blob:` images.

Other people's photos stay in memory, in a least-recently-used cache of 256 thumbnails and 8 full
images: at most about 10 MiB, nothing about whom the user looked at on disk, and no eviction job.
The cost is a refetch after a restart. Under Settings → Photos, a metered or unknown network
shows thumbnails and loads a full image on a tap, as chat originals do.

## States

| State | What shows |
|-------|------------|
| No photos | The avatar letters, and Add photo in the editor |
| Six photos | Add photo is disabled and says why |
| Wrong type, too large, cannot be read | The status line names the problem; nothing changes |
| Node refused or unreachable | The status line says so and a full node gets its own text; the photo stays local for a retry |
| Blob missing, fails its checks, or offline | The avatar letters on that photo; a later view tries again |
| Removing a photo | A confirmation first |

Order changes with Move left and Move right, and the first photo is the main one, so every step
works from the keyboard. Drag is not planned.

## Abuse

Upload limits are the quota above, the per-peer request and byte limits, and the proof of work on
the profile. An operator refuses a named blob hash or owner key
([abuse and legal requests](../operations/abuse-and-legal-requests.md)). No scanning runs on the
client or the node. Automated tests cover a non-JPEG, an oversized blob, a blob of another hash and
a JPEG that claims more pixels than the bound.
