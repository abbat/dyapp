//! Native exports: identity keys, MLS encryption, photo previews and the bootstrap seed check; the
//! message queue, profiles and video are prototypes with explicit stubs, and chat delivery
//! (`dyapp-client`) is not exported yet.

use dyapp_video::VideoSession as RustVideoSession;
use std::sync::Arc;
use tokio::sync::RwLock;
use uniffi::*;
use zeroize::Zeroizing;

mod mls;
pub use mls::{MlsDevice, MlsGroup, MlsNewGroup, MlsReceived};
mod network;
pub use network::{
    load_network, save_network_draft, CheckError, ConnectionCheck, NetworkFile, NetworkSettings,
    SeedRow,
};
mod pin;
pub use pin::{hash_app_pin, verify_app_pin};
mod profile_draft;
pub use profile_draft::{load_profile_draft, save_profile_draft, PublicProfileDraft};
mod profile_network;
pub use profile_network::{ProfilePublished, ProfilePublisher};
mod profile_publish;
pub use profile_publish::{profile_publish_block, ProfileSolve, ProfileWorkStep};
mod profile_search;
pub use profile_search::{
    FoundProfile, ProfileSearchFailure, ProfileSearchResult, ProfileSearcher, SearchCondition,
    SearchEmployer, SearchOp,
};
mod preview;
pub use preview::{check_preview, make_preview, network_metered, set_network_metered, Rgba};
mod strip;
pub use strip::strip_metadata;

// Public FFI interface for Swift/Kotlin
// Minimal surface: peer operations, key management, message/video

/// Caller-supplied peer label; this object does not open a network connection.
#[derive(Clone, uniffi::Object)]
pub struct PeerConnection {
    id: String,
}

#[uniffi::export]
impl PeerConnection {
    /// Stores the label without validating it as a peer ID.
    #[uniffi::constructor]
    pub fn new(peer_id: String) -> Arc<Self> {
        Arc::new(Self { id: peer_id })
    }

    /// Returns a copy of the stored label.
    pub fn get_id(&self) -> String {
        self.id.clone()
    }
}

/// Messaging placeholder; no queue, encryption or delivery is implemented here.
#[derive(Clone, uniffi::Object)]
pub struct MessageService {}

#[uniffi::export]
impl MessageService {
    /// Creates the stateless messaging placeholder.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    /// Ignores the text and returns a status string; nothing is queued or sent.
    pub fn send_message(&self, peer_id: String, text: String) -> Result<String, CoreError> {
        let _ = text;
        Ok(format!("Message queued to {}", peer_id))
    }

    /// Returns zero; there is no backing queue.
    pub fn queue_size(&self) -> i32 {
        0
    }
}

/// Lazy WebRTC session whose synchronous methods block on a private Tokio runtime.
#[derive(Clone, uniffi::Object)]
pub struct VideoSession {
    inner: Arc<RwLock<Option<RustVideoSession>>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[uniffi::export]
impl VideoSession {
    /// Creates the private runtime; the WebRTC session is not initialized yet.
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, CoreError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| CoreError::Failed(format!("Failed to create runtime: {}", e)))?;

        Ok(Arc::new(Self {
            inner: Arc::new(RwLock::new(None)),
            runtime: Arc::new(runtime),
        }))
    }

    /// Initializes the inner session if absent and returns its SDP offer.
    pub fn create_offer(&self) -> Result<String, CoreError> {
        self.runtime.block_on(async {
            self.initialized()
                .await?
                .as_ref()
                .ok_or_else(|| CoreError::Failed("Failed to create session".to_string()))?
                .create_offer()
                .await
                .map_err(|e| CoreError::Failed(format!("{:?}", e)))
        })
    }

    /// Callee side: initializes the inner session if absent and applies the caller's SDP offer,
    /// then queued ICE candidates.
    pub fn set_remote_offer(&self, sdp: String) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            self.initialized()
                .await?
                .as_ref()
                .ok_or_else(|| CoreError::Failed("Failed to create session".to_string()))?
                .set_remote_offer(sdp)
                .await
                .map_err(|e| CoreError::Failed(format!("{:?}", e)))
        })
    }

    /// Returns the SDP answer to the offer applied by `set_remote_offer`.
    pub fn create_answer(&self) -> Result<String, CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .create_answer()
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Applies the remote SDP answer, then queued ICE candidates, to the connection.
    pub fn receive_answer(&self, sdp: String) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .receive_answer(sdp)
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Advances the initialized session from offer-created to offer-sent.
    pub fn mark_offer_sent(&self) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .mark_offer_sent()
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Closes the initialized inner session; the wrapper remains allocated.
    pub fn end_session(&self) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .close()
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Returns the initialized inner session's UUID, or an initialization error.
    pub fn get_session_id(&self) -> Result<String, CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                Ok(session.session_id().to_string())
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }
}

impl VideoSession {
    /// The inner session, created on first use.
    async fn initialized(
        &self,
    ) -> Result<tokio::sync::RwLockWriteGuard<'_, Option<RustVideoSession>>, CoreError> {
        let mut session_guard = self.inner.write().await;
        if session_guard.is_none() {
            *session_guard = Some(
                RustVideoSession::new()
                    .await
                    .map_err(|error| CoreError::Failed(error.to_string()))?,
            );
        }
        Ok(session_guard)
    }
}

/// Error returned by every fallible export; the message may echo internal state.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    /// Operation-specific failure text, without a stable machine-readable code.
    #[error("{0}")]
    Failed(String),
}

/// A failed [`KeyStore`] call. Unlike the flat `CoreError`, it can be thrown by the app.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum WrapError {
    /// The keystore's reason, for logs; it must not contain key material.
    #[error("{reason}")]
    Failed {
        /// The keystore's reason.
        reason: String,
    },
}

/// The app threw something other than a `WrapError`.
impl From<uniffi::UnexpectedUniFFICallbackError> for WrapError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed {
            reason: error.reason,
        }
    }
}

impl From<WrapError> for CoreError {
    fn from(error: WrapError) -> Self {
        Self::Failed(error.to_string())
    }
}

/// The platform keystore (Android Keystore, Keychain, Secret Service, DPAPI), implemented by
/// the app: it encrypts with a key that never leaves the keystore.
#[uniffi::export(with_foreign)]
pub trait KeyStore: Send + Sync {
    /// Encrypts a secret key; the app persists the result.
    fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError>;
    /// Decrypts what `wrap` returned.
    fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError>;
}

/// An Ed25519 identity or device key; its secret leaves Rust only through a [`KeyStore`].
#[derive(uniffi::Object)]
pub struct IdentityKey {
    identity: dyapp_identity::Identity,
}

#[uniffi::export]
impl IdentityKey {
    /// A new key from the OS random number generator.
    #[uniffi::constructor]
    pub fn generate() -> Arc<Self> {
        Arc::new(Self {
            identity: dyapp_identity::Identity::generate(),
        })
    }

    /// The key that `store` unwraps from `wrapped`, a blob from [`IdentityKey::wrap`].
    #[uniffi::constructor]
    pub fn unwrap(store: Arc<dyn KeyStore>, wrapped: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let secret = Zeroizing::new(store.unwrap(wrapped)?);
        let secret: &[u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Failed("unwrapped key is not 32 bytes".to_string()))?;
        Ok(Arc::new(Self {
            identity: dyapp_identity::Identity::from_secret(secret),
        }))
    }

    /// The secret encrypted by `store`, for the app to persist.
    pub fn wrap(&self, store: Arc<dyn KeyStore>) -> Result<Vec<u8>, CoreError> {
        // ponytail: the FFI buffer that carries the secret to the app is freed, not wiped.
        Ok(store.wrap(self.identity.secret().to_vec())?)
    }

    /// Hex SHA-256 of the public key.
    pub fn peer_id(&self) -> String {
        self.identity.peer_id()
    }

    /// The Ed25519 public key bytes.
    pub fn public_key(&self) -> Vec<u8> {
        self.identity.public_key().to_vec()
    }
}

/// One localized picker choice for a closed public profile scale.
#[derive(Clone, uniffi::Record)]
pub struct ProfileChoice {
    /// Wire value; zero is intentionally absent.
    pub code: u32,
    /// Stable key that clients translate for display.
    pub key: String,
}

/// A public profile scale supplied by the shared profile core.
#[derive(Clone, uniffi::Record)]
pub struct ProfileScale {
    /// Protobuf field number.
    pub field: u32,
    /// Stable field name.
    pub key: String,
    /// Whether the scale permits range search.
    pub ordered: bool,
    /// Choices in display order.
    pub choices: Vec<ProfileChoice>,
}

/// Shared profile picker vocabulary for native clients.
#[uniffi::export]
pub fn profile_scales() -> Vec<ProfileScale> {
    dyapp_profile::SCALES
        .iter()
        .map(|scale| ProfileScale {
            field: scale.field,
            key: scale.key.to_string(),
            ordered: scale.ordered,
            choices: scale
                .options
                .iter()
                .map(|choice| ProfileChoice {
                    code: choice.code,
                    key: choice.key.to_string(),
                })
                .collect(),
        })
        .collect()
}

/// Shared validation and inclusive age bounds for independently optional public birth parts.
#[derive(Clone, uniffi::Record)]
pub struct BirthAssessment {
    /// False when no Gregorian date can match the supplied parts.
    pub valid: bool,
    /// Youngest possible age on the caller's UTC date; absent without a year or valid range.
    pub youngest: Option<u32>,
    /// Oldest possible age on the caller's UTC date; absent without a year or valid range.
    pub oldest: Option<u32>,
}

/// Assess exactly the supplied parts; no missing year, month or day is invented.
#[uniffi::export]
pub fn assess_birth_parts(
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    today_unix_days: u64,
) -> Result<BirthAssessment, CoreError> {
    let today = dyapp_profile::CivilDate::from_unix_days(today_unix_days)
        .ok_or_else(|| CoreError::Failed("invalid UTC date".into()))?;
    let profile = dyapp_profile::Profile {
        birth_year: year,
        birth_month: month,
        birth_day: day,
        ..Default::default()
    };
    let valid = profile.valid_birth_parts();
    let ages = valid.then(|| profile.possible_age_range(today)).flatten();
    Ok(BirthAssessment {
        valid,
        youngest: ages.map(|(youngest, _)| youngest),
        oldest: ages.map(|(_, oldest)| oldest),
    })
}

/// A profile place named in the UI language.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Place {
    /// GeoNames ID, the profile `place` value in decimal.
    pub id: u32,
    /// Place name.
    pub name: String,
    /// First-level region, empty when unknown.
    pub region: String,
}

impl From<dyapp_client::places::Place> for Place {
    fn from(p: dyapp_client::places::Place) -> Self {
        Self {
            id: p.id,
            name: p.name,
            region: p.region,
        }
    }
}

/// The place `id` of `country` named in `lang` (`ru`, else English); none outside the picker set.
#[uniffi::export]
pub fn place_name(country: String, id: u32, lang: String) -> Option<Place> {
    dyapp_client::places::place(&country, id, &lang).map(Place::from)
}

/// At most `limit` places of `country` whose English or Russian name starts with `prefix`,
/// largest first.
#[uniffi::export]
pub fn search_places(country: String, prefix: String, lang: String, limit: u32) -> Vec<Place> {
    dyapp_client::places::search(&country, &prefix, &lang, limit as usize)
        .into_iter()
        .map(Place::from)
        .collect()
}

/// Profile placeholder; it does not read or persist user data.
#[derive(Clone, uniffi::Object)]
pub struct ProfileService {}

#[uniffi::export]
impl ProfileService {
    /// Creates the stateless profile placeholder.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    /// Returns the constant placeholder ID, unknown birth date and unverified status.
    pub fn get_profile(&self) -> Result<UserProfile, CoreError> {
        Ok(UserProfile {
            user_id: "placeholder".to_string(),
            birth_year: None,
            birth_month: None,
            birth_day: None,
            verified: false,
        })
    }

    /// Accepts and discards both arguments; no profile field is changed.
    pub fn update_field(&self, field: String, value: String) -> Result<(), CoreError> {
        let _ = (field, value);
        Ok(())
    }
}

/// Native-facing profile fields, currently populated by the placeholder service.
#[derive(Clone, uniffi::Record)]
pub struct UserProfile {
    /// User label; the placeholder service returns `placeholder`.
    pub user_id: String,
    /// Public birth year, if shared by the owner.
    pub birth_year: Option<u32>,
    /// Public birth month, if shared by the owner.
    pub birth_month: Option<u32>,
    /// Public birth day, if shared by the owner.
    pub birth_day: Option<u32>,
    /// Verification flag; the placeholder service returns false.
    pub verified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn birth_assessment_keeps_optional_parts_and_rejects_impossible_dates() {
        let empty = assess_birth_parts(None, None, None, 0).unwrap();
        assert!(empty.valid);
        assert_eq!((empty.youngest, empty.oldest), (None, None));

        let yearless_leap_day = assess_birth_parts(None, Some(2), Some(29), 0).unwrap();
        assert!(yearless_leap_day.valid);
        assert_eq!(
            (yearless_leap_day.youngest, yearless_leap_day.oldest),
            (None, None)
        );

        let year_only = assess_birth_parts(Some(1950), None, None, 0).unwrap();
        assert!(year_only.valid);
        assert_eq!((year_only.youngest, year_only.oldest), (Some(19), Some(20)));

        let impossible = assess_birth_parts(Some(1900), Some(2), Some(29), 0).unwrap();
        assert!(!impossible.valid);
        assert_eq!((impossible.youngest, impossible.oldest), (None, None));
    }

    #[test]
    fn places_are_named_and_found_through_the_core() {
        let berlin = place_name("DE".into(), 2950159, "ru".into()).expect("Berlin");
        assert_eq!(berlin.name, "Берлин");
        let found = search_places("DE".into(), "ber".into(), "en".into(), 3);
        assert_eq!(found.len(), 3);
        assert_eq!((found[0].id, found[0].name.as_str()), (berlin.id, "Berlin"));
    }

    /// XOR with a fixed byte: stands in for the platform keystore.
    struct TestStore;

    impl KeyStore for TestStore {
        fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
        }

        fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            self.wrap(wrapped)
        }
    }

    /// A keystore whose key is gone.
    struct LostStore;

    impl KeyStore for LostStore {
        fn wrap(&self, _: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            Err(WrapError::Failed {
                reason: "no key".to_string(),
            })
        }

        fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            self.wrap(wrapped)
        }
    }

    #[test]
    fn keystore_errors_reach_the_caller() {
        let store: Arc<dyn KeyStore> = Arc::new(LostStore);
        let error = IdentityKey::generate().wrap(store.clone()).unwrap_err();
        assert_eq!(error.to_string(), "no key");
        assert!(IdentityKey::unwrap(store, vec![0; 32]).is_err());
    }

    #[test]
    fn wrapped_key_unwraps_to_the_same_peer_id() {
        let store: Arc<dyn KeyStore> = Arc::new(TestStore);
        let key = IdentityKey::generate();
        let wrapped = key.wrap(store.clone()).unwrap();
        assert_eq!(wrapped.len(), 32);
        assert_ne!(wrapped, key.identity.secret().to_vec());
        let opened = IdentityKey::unwrap(store, wrapped).unwrap();
        assert_eq!(opened.peer_id(), key.peer_id());
        assert_eq!(opened.public_key(), key.public_key());
    }

    #[test]
    fn video_sessions_exchange_an_offer_and_an_answer() {
        let caller = VideoSession::new().unwrap();
        let callee = VideoSession::new().unwrap();
        assert!(callee.create_answer().is_err());
        callee
            .set_remote_offer(caller.create_offer().unwrap())
            .unwrap();
        let answer = callee.create_answer().unwrap();
        caller.mark_offer_sent().unwrap();
        caller.receive_answer(answer).unwrap();
        assert!(callee.set_remote_offer("v=0".to_string()).is_err());
    }

    #[test]
    fn unwrap_rejects_a_key_of_the_wrong_length() {
        let store: Arc<dyn KeyStore> = Arc::new(TestStore);
        for wrapped in [vec![], vec![0; 31], vec![0; 33]] {
            assert!(IdentityKey::unwrap(store.clone(), wrapped).is_err());
        }
    }
}

// Generated UniFFI scaffolding has large metadata const arrays.
mod scaffolding {
    #![expect(
        clippy::large_const_arrays,
        reason = "generated UniFFI metadata is emitted as large const arrays"
    )]
    include!(concat!(env!("OUT_DIR"), "/dyapp.uniffi.rs"));
}
pub use scaffolding::*;
