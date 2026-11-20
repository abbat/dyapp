//! Bounded public-only controller protocol; never serialize attachment capabilities.

use dyapp_client::session::Submission;
use libp2p::{multiaddr::Protocol, Multiaddr};
use serde::Deserialize;
use std::io::{BufRead, Write};

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_PNG: usize = 1024 * 1024;
const MAX_COMMAND: u64 = (MAX_PNG as u64) * 4 + 1024;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    Receive {},
    Text {},
    Photo {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    Quit {},
}

pub(super) fn loopback(value: &str) -> Result<Multiaddr> {
    let address: Multiaddr = value.parse()?;
    let mut protocols = address.iter();
    let local = match protocols.next() {
        Some(Protocol::Ip4(ip)) => ip.is_loopback(),
        Some(Protocol::Ip6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !local
        || !matches!(protocols.next(), Some(Protocol::Tcp(1..=u16::MAX)))
        || protocols.next().is_some()
    {
        return Err("only a literal loopback IP and TCP port are permitted".into());
    }
    Ok(address)
}

pub(super) fn peer_id(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected a lowercase 64-digit owner Peer ID".into());
    }
    Ok(())
}

pub(super) fn png(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_PNG || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("fixture requires a PNG of at most 1 MiB".into());
    }
    Ok(())
}

fn read_from(reader: impl BufRead) -> Result<Option<Command>> {
    let mut bytes = Vec::new();
    let read = reader.take(MAX_COMMAND + 1).read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    if u64::try_from(read)? > MAX_COMMAND || bytes.last() != Some(&b'\n') {
        return Err("controller line is oversized or unfinished".into());
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub(super) fn read() -> Result<Option<Command>> {
    read_from(std::io::stdin().lock())
}

pub(super) fn submitted(submission: &Submission) -> serde_json::Value {
    serde_json::json!({
        "id": submission.id(),
        "accepted": matches!(submission, Submission::Accepted { .. }),
    })
}

pub(super) fn reply(value: &serde_json::Value) -> Result<()> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests;
