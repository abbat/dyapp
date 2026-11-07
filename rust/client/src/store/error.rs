/// Why a profile database could not be opened safely.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The app supplied a relative database path.
    #[error("client store path must be absolute")]
    RelativePath,
    /// The database path itself is a symbolic link.
    #[error("client store path is a symbolic link")]
    Symlink,
    /// An existing database has no recognized schema version.
    #[error("client store has an unversioned schema")]
    Unversioned,
    /// The database was written by a newer client.
    #[error("client store schema version {0} is unsupported")]
    FutureVersion(i64),
    /// The selected identity differs from the database's bound identity.
    #[error("client store belongs to another identity")]
    WrongIdentity,
    /// A stored setting has an unsupported format or value.
    #[error("client store setting is invalid")]
    InvalidSetting,
    /// A photo is not a JPEG within its size, or the profile has no room or no such photo.
    #[error("profile photo refused")]
    PhotoRefused,
    /// A clock value does not fit the SQLite integer representation.
    #[error("client store clock value is out of range")]
    ClockRange,
    /// A prepared write conflicts with stored bytes or an outbox row is corrupt.
    #[error("client store outbox record conflicts or is corrupt")]
    OutboxConflict,
    /// A group ID is empty or already belongs to another peer.
    #[error("client store group record conflicts or is corrupt")]
    GroupConflict,
    /// A cataloged group is absent from the sealed Device or lacks its recorded peer.
    #[error("client store group is missing or belongs to another peer")]
    GroupMismatch,
    /// MLS rejected a stored group.
    #[error("client store group: {0}")]
    Mls(#[from] dyapp_messaging::mls::Error),
    /// A caller tried to persist an empty sealed MLS snapshot.
    #[error("client store MLS snapshot is empty")]
    EmptyMlsState,
    /// A chat message or inbox row conflicts or is corrupt.
    #[error("client store message or inbox record conflicts or is corrupt")]
    InboxConflict,
    /// An interaction reuses an event ID with other bytes, or does not involve this profile.
    #[error("client store interaction conflicts or names another profile")]
    InteractionConflict,
    /// A blocked peer cannot receive a new outgoing interaction.
    #[error("client store peer is blocked")]
    BlockedPeer,
    /// A second own text to a peer who has not answered the first; one request stays pending.
    #[error("client store message request is still pending")]
    RequestPending,
    /// Outstanding pack requests have reached the identity-local bound.
    #[error("too many pending sticker pack requests")]
    PackRequestsFull,
    /// A match ended; a new match reopens the chat.
    #[error("client store chat is closed")]
    ChatClosed,
    /// A received timestamp is too far ahead of the local clock.
    #[error("client store interaction: {0}")]
    ClockSkew(#[from] dyapp_messaging::hlc::ClockSkew),
    /// A token row is missing, opens under another key, or names another contact.
    #[error("client store write token is missing, conflicts or is corrupt")]
    TokenConflict,
    /// A local contact name is longer than 64 characters or has hidden characters.
    #[error("client store local name is too long or not clean text")]
    InvalidName,
    /// The system random source failed.
    #[error("client store could not read the system random source")]
    Random,
    /// SQLite refused the database.
    #[error("client store: {0}")]
    Sql(#[from] rusqlite::Error),
}
