//! Exact durable contact authority, serialized with device observations.
use super::super::{Error, Store};
use dyapp_profile::{DeviceCertificate, VerifiedProfile};
use prost::Message as _;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

/// A missing exact marker at the legacy clamp is ambiguous, never an exact version.
#[derive(Clone, Copy)]
pub(in crate::store) enum Floor {
    Absent,
    Exact(u64),
    Ambiguous,
}

pub(in crate::store) fn floor(legacy: Option<i64>, exact: Option<&[u8]>) -> Result<Floor, Error> {
    match (legacy, exact) {
        (None, None) => Ok(Floor::Absent),
        (Some(value), Some(bytes)) => {
            let bytes = <[u8; 8]>::try_from(bytes).map_err(|_| Error::InvalidSetting)?;
            let version = u64::from_be_bytes(bytes);
            if version == 0 || clamped(version) != value {
                return Err(Error::InvalidSetting);
            }
            Ok(Floor::Exact(version))
        }
        (Some(i64::MAX), None) => Ok(Floor::Ambiguous),
        (Some(value), None) if value > 0 => Ok(Floor::Exact(
            u64::try_from(value).map_err(|_| Error::InvalidSetting)?,
        )),
        _ => Err(Error::InvalidSetting),
    }
}

fn clamped(version: u64) -> i64 {
    i64::try_from(version).unwrap_or(i64::MAX)
}

impl Floor {
    fn admits(self, incoming: u64) -> bool {
        match self {
            Self::Absent => true,
            Self::Exact(held) => incoming >= held,
            Self::Ambiguous => incoming == u64::MAX,
        }
    }
}

struct Observation {
    devices: Vec<u8>,
    floor: Floor,
    verified: bool,
}

type StoredObservation = (Option<Vec<u8>>, Option<i64>, Option<Vec<u8>>, bool);

fn observation(db: &Connection, peer: &str) -> Result<Option<Observation>, Error> {
    let row: Option<StoredObservation> = db
        .query_row(
            "SELECT `seen_devices`, `seen_version`, `seen_version_exact`, `verified`
         FROM `peers` WHERE `peer_id` = ?1",
            [peer],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((devices, legacy, exact, verified)) = row else {
        return Ok(None);
    };
    let floor = floor(legacy, exact.as_deref())?;
    match (devices, floor) {
        (None, Floor::Absent) => Ok(None),
        (Some(devices), Floor::Exact(_) | Floor::Ambiguous) if devices.len().is_multiple_of(32) => {
            Ok(Some(Observation {
                devices,
                floor,
                verified,
            }))
        }
        _ => Err(Error::InvalidSetting),
    }
}

// Import already holds the writer. A snapshot cannot erase a locally observed floor or
// forget equal-version devices; linking is not permission to roll back contact authority.
pub(in crate::store) fn admit_snapshot(
    db: &Connection,
    peer: &str,
    legacy: Option<i64>,
    exact: Option<&[u8]>,
    devices: Option<&[u8]>,
) -> Result<(), Error> {
    let incoming = floor(legacy, exact)?;
    if matches!(incoming, Floor::Absent) != devices.is_none()
        || devices.is_some_and(|devices| !devices.len().is_multiple_of(32))
    {
        return Err(Error::InvalidSetting);
    }
    let Some(known) = observation(db, peer)? else {
        return Ok(());
    };
    let same_devices = devices == Some(known.devices.as_slice());
    let admitted = match (known.floor, incoming) {
        (Floor::Exact(held), Floor::Exact(version)) => {
            version > held || (version == held && same_devices)
        }
        (Floor::Ambiguous, Floor::Exact(u64::MAX)) => true,
        (Floor::Ambiguous, Floor::Ambiguous) => same_devices,
        _ => false,
    };
    if !admitted {
        return Err(Error::ProfileCompletionSuperseded);
    }
    Ok(())
}

// Preserve an enclosing caller transaction; otherwise admit the writer before the first read.
fn writer(db: &Connection) -> Result<Option<Transaction<'_>>, Error> {
    Ok(if db.is_autocommit() {
        Some(Transaction::new_unchecked(
            db,
            TransactionBehavior::Immediate,
        )?)
    } else {
        None
    })
}

fn device_keys(profile: &VerifiedProfile) -> Vec<u8> {
    profile
        .profile
        .devices
        .iter()
        .filter_map(|signed| DeviceCertificate::decode(signed.payload.as_slice()).ok())
        .flat_map(|certificate| certificate.device_key)
        .collect()
}

impl Store {
    /// Mark the owner verified after a fresh safety-number or QR comparison.
    ///
    /// # Errors
    /// Rejects an older profile or an ambiguous legacy floor not proven current by
    /// the maximum unsigned version. Corrupt metadata and database failures preserve trust.
    pub fn set_verified(&self, profile: &VerifiedProfile) -> Result<(), Error> {
        let transaction = writer(&self.db)?;
        let known = observation(&self.db, &profile.peer_id)?;
        let version = profile.profile.version;
        if version == 0 || known.is_some_and(|known| !known.floor.admits(version)) {
            return Err(Error::ProfileCompletionSuperseded);
        }
        self.db.execute(
            "INSERT INTO `peers` (`peer_id`, `seen_devices`, `seen_version`, `seen_version_exact`, `verified`)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT (`peer_id`) DO UPDATE SET
                 `seen_devices` = excluded.`seen_devices`,
                 `seen_version` = excluded.`seen_version`,
                 `seen_version_exact` = excluded.`seen_version_exact`,
                 `verified` = 1, `new_device` = 0",
            params![profile.peer_id, device_keys(profile), clamped(version), version.to_be_bytes().as_slice()],
        )?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(())
    }

    /// Observe the held profile's devices and return its existing verification mark.
    /// Older profiles cannot replace observed authority. Equal-version forks accumulate
    /// devices so repeated observations do not alternate the new-device notice.
    ///
    /// # Errors
    /// Corrupt metadata or database failures leave the observation unchanged.
    /// Within a caller transaction all changes remain subject to that caller's commit.
    pub fn verified(&self, profile: &VerifiedProfile) -> Result<bool, Error> {
        let transaction = writer(&self.db)?;
        let verified = self.observe_devices(profile)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(verified)
    }

    // Caller has already admitted the writer and found no existing device observation.
    fn record_first_devices(&self, profile: &VerifiedProfile, devices: &[u8]) -> Result<(), Error> {
        let version = profile.profile.version;
        self.db.execute(
            "INSERT INTO `peers` (`peer_id`, `seen_devices`, `seen_version`, `seen_version_exact`)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (`peer_id`) DO UPDATE SET
                 `seen_devices` = excluded.`seen_devices`,
                 `seen_version` = excluded.`seen_version`,
                 `seen_version_exact` = excluded.`seen_version_exact`",
            params![
                profile.peer_id,
                devices,
                clamped(version),
                version.to_be_bytes().as_slice()
            ],
        )?;
        Ok(())
    }

    fn observe_devices(&self, profile: &VerifiedProfile) -> Result<bool, Error> {
        let version = profile.profile.version;
        if version == 0 {
            return Err(Error::InvalidSetting);
        }
        let devices = device_keys(profile);
        let Some(known) = observation(&self.db, &profile.peer_id)? else {
            self.record_first_devices(profile, &devices)?;
            return Ok(false);
        };
        if !known.floor.admits(version) {
            return Ok(match known.floor {
                Floor::Ambiguous => false,
                _ => known.verified,
            });
        }
        let added: Vec<u8> = devices
            .chunks(32)
            .filter(|key| !known.devices.chunks(32).any(|held| held == *key))
            .flatten()
            .copied()
            .collect();
        let unchanged = added.is_empty();
        let equal = matches!(known.floor, Floor::Exact(held) if held == version);
        if equal && unchanged {
            return Ok(known.verified);
        }
        let devices = if equal {
            [known.devices, added].concat()
        } else {
            devices
        };
        let verified = known.verified && unchanged && !matches!(known.floor, Floor::Ambiguous);
        self.db.execute(
            "UPDATE `peers`
             SET `seen_devices` = ?2, `seen_version` = ?3, `seen_version_exact` = ?4,
                 `verified` = ?5, `new_device` = `new_device` OR ?6
             WHERE `peer_id` = ?1",
            params![
                profile.peer_id,
                devices,
                clamped(version),
                version.to_be_bytes().as_slice(),
                verified,
                !unchanged
            ],
        )?;
        Ok(verified)
    }
}
