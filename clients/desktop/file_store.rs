use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use dyapp_identity::Identity;
use zeroize::Zeroizing;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub fn save(path: &Path, identity: &Identity) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Identity file has no parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(&*identity.secret())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Identity, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Identity path is not a regular file".into());
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err("Identity file is accessible to other users".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(33));
    file.take(33)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("Invalid identity file length".into());
    }
    let mut secret = Zeroizing::new([0_u8; 32]);
    secret.copy_from_slice(&bytes);
    Ok(Identity::from_secret(&secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn round_trip_never_overwrites_and_rejects_corruption() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("dyapp-file-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("identity.key");
        let identity = Identity::generate();
        save(&path, &identity).unwrap();
        assert_eq!(load(&path).unwrap().peer_id(), identity.peer_id());
        assert!(save(&path, &Identity::generate()).is_err());
        assert_eq!(load(&path).unwrap().peer_id(), identity.peer_id());
        #[cfg(unix)]
        {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(load(&path).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        fs::write(&path, b"corrupt").unwrap();
        assert!(load(&path).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
