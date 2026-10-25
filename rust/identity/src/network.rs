//! Per-identity bootstrap settings: the seed rows and network name a client dials with.
//!
//! Each identity folder keeps two files: `network-draft`, the rows as the user last typed them,
//! and `network`, the configuration the last successful check activated. Rows are stored
//! verbatim, so an explicit port such as 7070 is never rewritten. A missing file means nothing
//! was saved yet: an empty draft and no active configuration.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;

const HEADER: &str = "dyapp-network 1";
// Far above any real list; a larger file is not ours.
const MAX_BYTES: u64 = 64 * 1024;

/// Which of an identity's two settings files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settings {
    /// The rows being edited, saved before every check so a restart keeps them.
    Draft,
    /// The configuration in use; replaced only after a successful check.
    Active,
}

impl Settings {
    /// The file name inside the identity folder; removal deletes it with the folder.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Draft => "network-draft",
            Self::Active => "network",
        }
    }
}

/// Ordered seed rows and the network name; empty name is the public network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Network {
    /// `host:port` or `host:port/<peer ID>` rows, in the user's order.
    pub seeds: Vec<String>,
    /// The ADR 0017 network name, empty for public.
    pub name: String,
}

/// Checks an ADR 0017 network name: at most 64 printable ASCII bytes, without edge spaces.
pub fn check_name(name: &str) -> Result<(), String> {
    if let Some(c) = name.chars().find(|c| !(' '..='~').contains(c)) {
        return Err(format!("network name: {c:?} is not printable ASCII"));
    }
    if name.len() > 64 {
        return Err(format!("network name: {} bytes, at most 64", name.len()));
    }
    if name.starts_with(' ') || name.ends_with(' ') {
        return Err("network name: leading or trailing space".into());
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn encode(network: &Network) -> io::Result<String> {
    let fields = || std::iter::once(&network.name).chain(&network.seeds);
    if fields().any(|field| field.contains(['\n', '\r'])) {
        return Err(invalid("network settings: a field holds a line break"));
    }
    let mut text = format!("{HEADER}\nname {}\n", network.name);
    for seed in &network.seeds {
        text += &format!("seed {seed}\n");
    }
    Ok(text)
}

fn decode(text: &str) -> io::Result<Network> {
    let mut lines = text
        .strip_suffix('\n')
        .ok_or_else(|| invalid("network settings: truncated"))?
        .split('\n');
    if lines.next() != Some(HEADER) {
        return Err(invalid("network settings: unknown format"));
    }
    let name = lines.next().and_then(|line| line.strip_prefix("name "));
    let name = name.ok_or_else(|| invalid("network settings: no name"))?;
    let seeds = lines.map(|line| line.strip_prefix("seed ").map(str::to_owned));
    Ok(Network {
        seeds: seeds
            .collect::<Option<_>>()
            .ok_or_else(|| invalid("network settings: unknown line"))?,
        name: name.into(),
    })
}

/// Reads one settings file of the identity folder `dir`; `None` when it was never saved.
/// A damaged file is an error, never an empty default a later save would overwrite blindly.
pub fn load(dir: &Path, which: Settings) -> io::Result<Option<Network>> {
    let file = match fs::File::open(dir.join(which.file_name())) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        file => file?,
    };
    let mut text = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(invalid("network settings: file too large"));
    }
    decode(&text).map(Some)
}

/// Replaces one settings file of `dir` atomically: a crash leaves the old or the new content.
pub fn save(dir: &Path, which: Settings, network: &Network) -> io::Result<()> {
    let text = encode(network)?;
    let path = dir.join(which.file_name());
    let temp = path.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(&temp, &path)?;
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-network-{tag}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_round_trip_verbatim_and_stay_per_identity() {
        let root = temp_dir("trip");
        let (alice, bob) = (root.join("a"), root.join("b"));
        fs::create_dir_all(&alice).unwrap();
        fs::create_dir_all(&bob).unwrap();
        assert_eq!(load(&alice, Settings::Draft).unwrap(), None);
        assert_eq!(load(&alice, Settings::Active).unwrap(), None);

        let draft = Network {
            seeds: vec![
                "dns.example:7070".into(),
                " [::1]:33445/abc ".into(),
                "not an address".into(),
            ],
            name: " city=1 ".into(),
        };
        save(&alice, Settings::Draft, &draft).unwrap();
        // A fresh read stands in for a restart: nothing is cached in memory.
        assert_eq!(load(&alice, Settings::Draft).unwrap(), Some(draft.clone()));
        assert_eq!(load(&alice, Settings::Active).unwrap(), None);
        assert_eq!(load(&bob, Settings::Draft).unwrap(), None);

        let active = Network {
            seeds: vec!["dns.example:7070".into()],
            name: String::new(),
        };
        save(&alice, Settings::Active, &active).unwrap();
        save(&alice, Settings::Draft, &Network::default()).unwrap();
        assert_eq!(load(&alice, Settings::Active).unwrap(), Some(active));
        assert_eq!(
            load(&alice, Settings::Draft).unwrap(),
            Some(Network::default())
        );
        let mut names: Vec<_> = fs::read_dir(&alice)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["network", "network-draft"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_root_keeps_its_key_files_untouched() {
        let root = temp_dir("legacy");
        fs::write(root.join("identity.key"), b"key").unwrap();
        fs::write(root.join("identity-storage"), b"file").unwrap();
        let network = Network {
            seeds: vec!["seed.example:33445".into()],
            name: "org".into(),
        };
        save(&root, Settings::Active, &network).unwrap();
        assert_eq!(load(&root, Settings::Active).unwrap(), Some(network));
        assert_eq!(fs::read(root.join("identity.key")).unwrap(), b"key");
        assert_eq!(fs::read(root.join("identity-storage")).unwrap(), b"file");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn damaged_files_and_line_breaks_are_refused() {
        let dir = temp_dir("bad");
        let line_break = Network {
            seeds: vec!["a:1\nseed b:2".into()],
            name: String::new(),
        };
        assert!(save(&dir, Settings::Draft, &line_break).is_err());
        assert_eq!(load(&dir, Settings::Draft).unwrap(), None);
        let path = dir.join("network");
        for text in [
            "",
            "dyapp-network 1\nname x",
            "dyapp-network 2\nname x\n",
            "dyapp-network 1\nseed a:1\n",
            "dyapp-network 1\nname \nport 1\n",
        ] {
            fs::write(&path, text).unwrap();
            assert!(load(&dir, Settings::Active).is_err(), "{text:?}");
        }
        fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        assert!(load(&dir, Settings::Active).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn network_names_follow_adr_0017() {
        for name in ["", "city", "a b", &"x".repeat(64), "~!"] {
            assert_eq!(check_name(name), Ok(()), "{name:?}");
        }
        for name in [" a", "a ", &"x".repeat(65), "é", "a\tb"] {
            assert!(check_name(name).is_err(), "{name:?}");
        }
    }
}
