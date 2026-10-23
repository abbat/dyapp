#[cfg(target_os = "linux")]
mod platform {
    use std::collections::HashMap;
    use std::fmt::Write;

    use dyapp_identity::Identity;
    use secret_service::blocking::SecretService;
    use secret_service::{EncryptionType, Error};
    use zeroize::Zeroizing;

    fn attributes(slot: &str) -> HashMap<&str, &str> {
        HashMap::from([
            ("application", "dyapp"),
            ("kind", "identity"),
            ("slot", slot),
        ])
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Unavailable {
        NoSessionBus,
        NoService,
        NoDefaultCollection,
        Locked,
        AccessDenied,
    }

    impl Unavailable {
        pub fn key(&self) -> &'static str {
            match self {
                Self::NoSessionBus => "storage.system.noSessionBus",
                Self::NoService => "storage.system.noService",
                Self::NoDefaultCollection => "storage.system.noCollection",
                Self::Locked => "storage.system.locked",
                Self::AccessDenied => "storage.system.denied",
            }
        }
    }

    pub fn probe() -> Result<(), Unavailable> {
        if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
            return Err(Unavailable::NoSessionBus);
        }
        let service = SecretService::connect(EncryptionType::Dh).map_err(map_service_error)?;
        let collection = service
            .get_default_collection()
            .map_err(map_collection_error)?;
        if collection.is_locked().map_err(map_collection_error)? {
            return Err(Unavailable::Locked);
        }
        let item = collection
            .create_item(
                "DYApp storage check",
                attributes("capability-probe"),
                b"probe",
                true,
                "text/plain",
            )
            .map_err(map_collection_error)?;
        let secret = item.get_secret();
        item.delete().map_err(map_collection_error)?;
        if secret.map_err(map_collection_error)?.as_slice() != b"probe" {
            return Err(Unavailable::AccessDenied);
        }
        Ok(())
    }

    fn map_service_error(error: Error) -> Unavailable {
        match error {
            Error::Locked => Unavailable::Locked,
            Error::Prompt | Error::PromptDisconnected => Unavailable::AccessDenied,
            _ => Unavailable::NoService,
        }
    }

    fn map_collection_error(error: Error) -> Unavailable {
        match error {
            Error::Locked => Unavailable::Locked,
            Error::NoResult => Unavailable::NoDefaultCollection,
            _ => Unavailable::AccessDenied,
        }
    }

    pub fn load() -> Result<Option<Identity>, String> {
        load_slot("primary")
    }

    fn load_slot(slot: &str) -> Result<Option<Identity>, String> {
        let service = SecretService::connect(EncryptionType::Dh).map_err(|e| e.to_string())?;
        let collection = service
            .get_default_collection()
            .map_err(|e| e.to_string())?;
        if collection.is_locked().map_err(|e| e.to_string())? {
            return Err("Secret Service collection is locked".into());
        }
        let items = collection
            .search_items(attributes(slot))
            .map_err(|e| e.to_string())?;
        match items.as_slice() {
            [] => Ok(None),
            [item] => {
                let secret = Zeroizing::new(item.get_secret().map_err(|e| e.to_string())?);
                decode_identity(&secret).map(Some)
            }
            _ => Err("Multiple DYApp identities found in Secret Service".into()),
        }
    }

    pub fn save(identity: &Identity) -> Result<(), String> {
        save_slot("primary", identity)
    }

    fn save_slot(slot: &str, identity: &Identity) -> Result<(), String> {
        let service = SecretService::connect(EncryptionType::Dh).map_err(|e| e.to_string())?;
        let collection = service
            .get_default_collection()
            .map_err(|e| e.to_string())?;
        if collection.is_locked().map_err(|e| e.to_string())? {
            return Err("Secret Service collection is locked".into());
        }
        if !collection
            .search_items(attributes(slot))
            .map_err(|e| e.to_string())?
            .is_empty()
        {
            return Err("DYApp identity already exists in Secret Service".into());
        }
        let secret = encode_identity(identity);
        collection
            .create_item(
                "DYApp identity",
                attributes(slot),
                secret.as_bytes(),
                false,
                "text/plain",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn encode_identity(identity: &Identity) -> Zeroizing<String> {
        let mut result = Zeroizing::new(String::with_capacity(64));
        let secret = identity.secret();
        for byte in secret.iter() {
            write!(&mut *result, "{byte:02x}").expect("writing to a String cannot fail");
        }
        result
    }

    fn decode_identity(secret: &[u8]) -> Result<Identity, String> {
        if secret.len() != 64 {
            return Err("Invalid DYApp identity length in Secret Service".into());
        }
        let mut bytes = Zeroizing::new([0_u8; 32]);
        for (index, pair) in secret.chunks_exact(2).enumerate() {
            let digits =
                std::str::from_utf8(pair).map_err(|_| "Invalid DYApp identity encoding")?;
            bytes[index] =
                u8::from_str_radix(digits, 16).map_err(|_| "Invalid DYApp identity encoding")?;
        }
        Ok(Identity::from_secret(&bytes))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn identity_encoding_round_trips_and_rejects_corruption() {
            let identity = Identity::generate();
            let encoded = encode_identity(&identity);
            assert_eq!(
                decode_identity(encoded.as_bytes()).unwrap().peer_id(),
                identity.peer_id()
            );
            assert!(decode_identity(b"short").is_err());
            assert!(decode_identity(&[b'z'; 64]).is_err());
        }

        #[test]
        fn controlled_secret_service_round_trip() {
            if std::env::var_os("DYAPP_TEST_SECRET_SERVICE").is_none() {
                return;
            }
            assert_eq!(probe(), Ok(()));
            let slot = "controlled-test";
            assert!(load_slot(slot).unwrap().is_none());
            let identity = Identity::generate();
            save_slot(slot, &identity).unwrap();
            assert_eq!(
                load_slot(slot).unwrap().unwrap().peer_id(),
                identity.peer_id()
            );
            assert!(save_slot(slot, &Identity::generate()).is_err());
            assert_eq!(
                load_slot(slot).unwrap().unwrap().peer_id(),
                identity.peer_id()
            );
        }

        #[test]
        fn provider_errors_have_distinct_unavailable_reasons() {
            // CI launches the desktop in a controlled D-Bus session; this test only checks
            // the classifier so it remains safe in a developer's active desktop session.
            assert_eq!(
                map_service_error(Error::Unavailable),
                Unavailable::NoService
            );
            assert_eq!(
                map_collection_error(Error::NoResult),
                Unavailable::NoDefaultCollection
            );
            assert_eq!(map_collection_error(Error::Locked), Unavailable::Locked);
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::collections::HashMap;

    use dyapp_identity::Identity;
    use keyring_core::api::CredentialStoreApi;
    use keyring_core::{Entry, Error};
    use windows_native_keyring_store::Store;
    use zeroize::Zeroizing;

    const SERVICE: &str = "com.dyapp.identity";
    const USER: &str = "primary";

    #[derive(Debug, PartialEq, Eq)]
    pub enum Unavailable {
        CredentialManagerUnavailable,
        AccessDenied,
    }

    impl Unavailable {
        pub fn key(&self) -> &'static str {
            match self {
                Self::CredentialManagerUnavailable => "storage.system.unavailable",
                Self::AccessDenied => "storage.system.denied",
            }
        }
    }

    fn entry(user: &str) -> Result<Entry, Error> {
        let store = Store::new()?;
        store.build(
            SERVICE,
            user,
            Some(&HashMap::from([("persistence", "Local")])),
        )
    }

    pub fn probe() -> Result<(), Unavailable> {
        let entry = entry("capability-probe").map_err(map_probe_error)?;
        entry.set_secret(b"probe").map_err(map_probe_error)?;
        let secret = entry.get_secret();
        let attributes = entry.get_attributes();
        entry.delete_credential().map_err(map_probe_error)?;
        if secret.map_err(map_probe_error)?.as_slice() != b"probe"
            || attributes
                .map_err(map_probe_error)?
                .get("persistence")
                .map(String::as_str)
                != Some("Local")
        {
            return Err(Unavailable::CredentialManagerUnavailable);
        }
        Ok(())
    }

    fn map_probe_error(error: Error) -> Unavailable {
        match error {
            Error::NoStorageAccess(_) => Unavailable::AccessDenied,
            _ => Unavailable::CredentialManagerUnavailable,
        }
    }

    pub fn load() -> Result<Option<Identity>, String> {
        load_user(USER)
    }

    fn load_user(user: &str) -> Result<Option<Identity>, String> {
        let entry = entry(user).map_err(|e| e.to_string())?;
        let secret = match entry.get_secret() {
            Ok(secret) => Zeroizing::new(secret),
            Err(Error::NoEntry) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let bytes: &[u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid DYApp identity length in Credential Manager")?;
        Ok(Some(Identity::from_secret(bytes)))
    }

    pub fn save(identity: &Identity) -> Result<(), String> {
        save_user(USER, identity)
    }

    fn save_user(user: &str, identity: &Identity) -> Result<(), String> {
        let entry = entry(user).map_err(|e| e.to_string())?;
        match entry.get_secret() {
            Err(Error::NoEntry) => (),
            Ok(secret) => {
                let _secret = Zeroizing::new(secret);
                return Err("DYApp identity already exists in Credential Manager".into());
            }
            Err(error) => return Err(error.to_string()),
        }
        entry
            .set_secret(&*identity.secret())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn controlled_credential_manager_round_trip() {
            if std::env::var_os("DYAPP_TEST_CREDENTIAL_MANAGER").is_none() {
                return;
            }
            assert_eq!(probe(), Ok(()));
            let user = "controlled-test";
            assert!(load_user(user).unwrap().is_none());
            let identity = Identity::generate();
            save_user(user, &identity).unwrap();
            assert_eq!(
                load_user(user).unwrap().unwrap().peer_id(),
                identity.peer_id()
            );
            assert!(save_user(user, &Identity::generate()).is_err());
            assert_eq!(
                load_user(user).unwrap().unwrap().peer_id(),
                identity.peer_id()
            );
            assert_eq!(
                entry(user).unwrap().get_attributes().unwrap()["persistence"],
                "Local"
            );
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub use platform::{load, probe, save, Unavailable};
