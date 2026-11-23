use super::{Cache, Checkpoint, Error, Platform, Snapshot, Version};
use crate::updates::{CheckState, Interval};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    schema: u32,
    jitter: u64,
    interval_seconds: u64,
    last_attempt: Option<u64>,
    last_success: Option<u64>,
    consecutive_failures: u32,
    in_flight: bool,
    highest: Vec<High>,
    #[serde(default)]
    dismissed: Vec<High>,
    #[serde(default)]
    notified: Vec<High>,
    feed: Option<String>,
    etag: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct High {
    platform: String,
    version: String,
}

pub(super) fn value(snapshot: &Snapshot) -> Result<Value, Error> {
    let checkpoint = snapshot.checkpoint;
    let feed = snapshot
        .cache
        .as_ref()
        .map(|cache| std::str::from_utf8(cache.bytes()).map_err(|_| Error::Invalid))
        .transpose()?;
    let highest = encode_marks(&snapshot.highest);
    Ok(json!({
        "schema": 1,
        "jitter": snapshot.jitter,
        "interval_seconds": checkpoint.interval.seconds(),
        "last_attempt": checkpoint.state.last_attempt,
        "last_success": checkpoint.state.last_success,
        "consecutive_failures": checkpoint.state.consecutive_failures,
        "in_flight": checkpoint.in_flight,
        "highest": highest,
        "dismissed": encode_marks(&snapshot.dismissed),
        "notified": encode_marks(&snapshot.notified),
        "feed": feed,
        "etag": snapshot.cache.as_ref().and_then(Cache::etag),
    }))
}

fn encode_marks(marks: &BTreeMap<Platform, Version>) -> Vec<Value> {
    marks.iter().map(|(platform, version)| {
        json!({"platform": platform.key(), "version": version.to_string()})
    }).collect()
}

fn decode_marks(saved: Vec<High>) -> Result<BTreeMap<Platform, Version>, Error> {
    if saved.len() > Platform::ALL.len() {
        return Err(Error::Invalid);
    }
    let mut marks = BTreeMap::new();
    for high in saved {
        let platform = Platform::parse(&high.platform).ok_or(Error::Invalid)?;
        let version = Version::parse(&high.version).ok_or(Error::Invalid)?;
        if marks.insert(platform, version).is_some() {
            return Err(Error::Invalid);
        }
    }
    Ok(marks)
}

impl Saved {
    pub(super) fn into_snapshot(self) -> Result<Snapshot, Error> {
        if self.schema != 1 || self.highest.len() > Platform::ALL.len() {
            return Err(Error::Invalid);
        }
        let interval = match self.interval_seconds {
            21_600 => Interval::SixHours,
            43_200 => Interval::TwelveHours,
            86_400 => Interval::Day,
            604_800 => Interval::Week,
            _ => return Err(Error::Invalid),
        };
        let highest = decode_marks(self.highest)?;
        let dismissed = decode_marks(self.dismissed)?;
        let notified = decode_marks(self.notified)?;
        let cache = match self.feed {
            Some(feed) => Some(Cache::restore(feed.into_bytes(), self.etag.as_deref())?),
            None if self.etag.is_none() => None,
            None => return Err(Error::Invalid),
        };
        Ok(Snapshot {
            jitter: self.jitter,
            checkpoint: Checkpoint {
                interval,
                state: CheckState {
                    last_attempt: self.last_attempt,
                    last_success: self.last_success,
                    consecutive_failures: self.consecutive_failures,
                },
                in_flight: self.in_flight,
            },
            cache,
            highest,
            dismissed,
            notified,
        })
    }
}
