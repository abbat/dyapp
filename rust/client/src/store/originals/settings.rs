//! One atomic device-local preference: disabling retention preserves its configured budget.
use super::{prune, Error, Store, FOLDER, MIB, SETTING};
use rusqlite::{params, OptionalExtension};
use std::{collections::HashSet, fs, io, path::Path};

/// Independent retention switch and encrypted-file byte budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaCacheConfig {
    /// Whether opened originals are retained on this device.
    pub enabled: bool,
    /// Saved even while disabled; inclusive range 1 MiB through 10 GiB.
    pub limit_bytes: u64,
}

impl Default for MediaCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            limit_bytes: 256 * MIB,
        }
    }
}

impl MediaCacheConfig {
    fn validate(self) -> Result<Self, Error> {
        if !(MIB..=10 * 1024 * MIB).contains(&self.limit_bytes) {
            return Err(Error::InvalidSetting);
        }
        Ok(self)
    }
}

impl Store {
    /// Numeric cache preference. Pre-release development encodings are not migrated.
    pub fn media_cache_config(&self) -> Result<MediaCacheConfig, Error> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [SETTING],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((version, value)) = row else {
            return Ok(MediaCacheConfig::default());
        };
        if version != 1 || value.len() != 9 || value[0] > 1 {
            return Err(Error::InvalidSetting);
        }
        let bytes = value[1..].try_into().map_err(|_| Error::InvalidSetting)?;
        MediaCacheConfig {
            enabled: value[0] == 1,
            limit_bytes: u64::from_be_bytes(bytes),
        }
        .validate()
    }

    /// Persists both fields together; callers sweep afterwards to apply a lower budget.
    pub fn set_media_cache_config(&self, config: MediaCacheConfig) -> Result<(), Error> {
        let config = config.validate()?;
        let mut value = vec![u8::from(config.enabled)];
        value.extend_from_slice(&config.limit_bytes.to_be_bytes());
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![SETTING, value],
        )?;
        Ok(())
    }

    /// MiB view retained for existing client controls; off is represented by zero.
    pub fn kept_originals_mib(&self) -> Result<u32, Error> {
        let config = self.media_cache_config()?;
        if !config.enabled {
            return Ok(0);
        }
        u32::try_from(config.limit_bytes / MIB).map_err(|_| Error::InvalidSetting)
    }

    /// Numeric MiB wrapper. Zero disables retention without forgetting the configured quota.
    pub fn set_kept_originals_mib(&self, mib: u32) -> Result<(), Error> {
        let mut config = self.media_cache_config()?;
        config.enabled = mib != 0;
        if config.enabled {
            config.limit_bytes = u64::from(mib) * MIB;
        }
        self.set_media_cache_config(config)
    }

    /// Actual encrypted original-file bytes; a missing cache uses no space.
    pub fn media_cache_usage(&self, dir: &Path) -> io::Result<u64> {
        let folder = dir.join(FOLDER);
        match fs::symlink_metadata(&folder) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected media cache folder",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        }
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        };
        let mut total = 0u64;
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected media cache entry",
                ));
            }
            total = total
                .checked_add(entry.metadata()?.len())
                .ok_or_else(|| io::Error::other("media cache size overflow"))?;
        }
        Ok(total)
    }

    /// Explicit local cache clear; leaves history, saved packs and the configured quota intact.
    pub fn clear_media_cache(&self, dir: &Path) -> io::Result<()> {
        // Validate entries before pruning: never traverse an unexpected directory or symlink.
        self.media_cache_usage(dir)?;
        prune(dir, &HashSet::new(), 0)
    }
}

#[cfg(test)]
mod tests;
