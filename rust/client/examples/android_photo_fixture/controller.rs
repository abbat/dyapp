//! Single-guest loopback adapter; the node and application logic remain in Session.

use super::wire::{self, Command, Result};
use std::io::{self, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TRANSITION: Duration = Duration::from_secs(120);

struct DeadlineStream {
    stream: TcpStream,
    deadline: Instant,
}

impl Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "controller frame deadline",
            ));
        }
        self.stream.set_read_timeout(Some(remaining))?;
        self.stream.read(buffer)
    }
}

enum Source {
    Stdin,
    Socket(BufReader<DeadlineStream>),
}

impl Source {
    fn command(&mut self) -> Result<Option<Command>> {
        match self {
            Self::Stdin => wire::read_from(io::stdin().lock()),
            Self::Socket(reader) => {
                // Reset after a complete command, never for individual arriving fragments.
                reader.get_mut().deadline = Instant::now() + TRANSITION;
                wire::read_from(reader)
            }
        }
    }
}

pub(super) struct Controller {
    input: Arc<Mutex<Source>>,
    output: Box<dyn Write + Send>,
}

impl Controller {
    pub(super) async fn open(selector: &str) -> Result<(Self, String)> {
        if selector != "--listen" {
            wire::peer_id(selector)?;
            return Ok((
                Self {
                    input: Arc::new(Mutex::new(Source::Stdin)),
                    output: Box::new(io::stdout()),
                },
                selector.to_owned(),
            ));
        }
        let listener = TcpListener::bind(("127.0.0.1", 33446))?;
        wire::reply(&serde_json::json!({"listening": listener.local_addr()?.port()}))?;
        let stream = tokio::task::spawn_blocking(move || listener.accept())
            .await??
            .0;
        let mut controller = Self::from_socket(stream)?;
        let peer = controller.initialize().await?;
        Ok((controller, peer))
    }

    async fn initialize(&mut self) -> Result<String> {
        let Some(Command::Init { peer }) = self.read().await? else {
            return Err("first guest command must initialize its public Peer ID".into());
        };
        wire::peer_id(&peer)?;
        self.reply(&serde_json::json!({"initialized": true}))?;
        Ok(peer)
    }

    fn from_socket(stream: TcpStream) -> Result<Self> {
        stream.set_write_timeout(Some(TRANSITION))?;
        let source = DeadlineStream {
            stream: stream.try_clone()?,
            deadline: Instant::now() + TRANSITION,
        };
        Ok(Self {
            input: Arc::new(Mutex::new(Source::Socket(BufReader::new(source)))),
            output: Box::new(stream),
        })
    }

    pub(super) async fn read(&self) -> Result<Option<Command>> {
        let input = Arc::clone(&self.input);
        tokio::task::spawn_blocking(move || {
            input
                .lock()
                .map_err(|_| "controller reader poisoned")?
                .command()
        })
        .await?
    }

    pub(super) fn reply(&mut self, value: &serde_json::Value) -> Result<()> {
        wire::write_to(&mut self.output, value)
    }
}

#[cfg(test)]
mod tests;
