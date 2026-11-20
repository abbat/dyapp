//! Host-only loopback client for Android incoming-PNG instrumentation.

mod wire;

use dyapp_client::{attachment, session::Session, store::Store, Node};
use dyapp_identity::Identity;
use dyapp_p2p_net::Protocols;
use dyapp_profile::Profile;
use std::{path::Path, sync::Arc};
use wire::{Command, Result};
use zeroize::Zeroizing;

struct Fixture {
    owner: Identity,
    device: Identity,
    seal: Zeroizing<[u8; 32]>,
    session: Session,
    nodes: [Node; 1],
    peer: String,
}

impl Fixture {
    async fn start(address: &str, database: &Path, peer: String) -> Result<Self> {
        let address = wire::loopback(address)?;
        wire::peer_id(&peer)?;
        if !database.is_absolute() {
            return Err("fixture database must be absolute".into());
        }
        // Never reopen an existing user database or silently reuse old fixture state.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(database)?;
        dyapp_p2p_net::set_id_pow_bits(8);
        dyapp_profile::set_required_pow_bits(4);
        let owner = Identity::try_generate()?;
        let device = Identity::try_generate()?;
        let mut seal = Zeroizing::new([0; 32]);
        getrandom::fill(seal.as_mut())?;
        let session =
            Session::open_or_create(Store::open(database, &owner)?, &owner, &device, &seal)?;
        let node = Node::connect(address, Arc::new(Protocols::public())).await?;
        let mut fixture = Self {
            owner,
            device,
            seal,
            session,
            nodes: [node],
            peer,
        };
        fixture.publish().await?;
        Ok(fixture)
    }

    async fn publish(&mut self) -> Result<()> {
        let profile = Profile {
            version: 1,
            gender: 1,
            birth_year: Some(1990),
            birth_month: Some(1),
            birth_day: Some(1),
            devices: vec![self.session.certificate()?],
            device_owners: vec![dyapp_client::owner_binding(
                &self.device,
                &self.owner.public_key(),
            )],
            pow_bits: 4,
            pow_nonce: dyapp_profile::solve(&self.owner.public_key(), 4, 0, 4096)
                .map_err(|_| "fixture profile proof exhausted")?,
            ..Profile::default()
        };
        self.nodes[0]
            .publish_profile(profile.sign(&self.owner))
            .await?;
        self.session.store().note_device_listed(1)?;
        self.session
            .publish_key_packages(&mut self.nodes[0], &self.device, 4, &self.seal)
            .await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<serde_json::Value> {
        let own = self.nodes[0].get_profile(&self.owner.peer_id()).await?;
        let other = self.nodes[0].get_profile(&self.peer).await?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
        self.session
            .retry_due(&mut self.nodes, i64::try_from(now.as_millis())?, &self.seal)
            .await?;
        let (events, more) = self
            .session
            .receive_page(&mut self.nodes, &self.device, &[own, other], &self.seal)
            .await?;
        Ok(serde_json::json!({
            "events": events.len(), "more": more,
            "open": self.session.conversation(&self.peer)? == dyapp_client::store::Conversation::Open,
        }))
    }

    async fn text(&mut self) -> Result<serde_json::Value> {
        let peer = self.nodes[0].get_profile(&self.peer).await?;
        let result = self
            .session
            .submit_text_to(
                &mut self.nodes,
                &[peer],
                b"Android photo fixture",
                &self.seal,
            )
            .await?;
        Ok(wire::submitted(&result))
    }

    async fn photo(&mut self, bytes: &[u8], width: u32, height: u32) -> Result<serde_json::Value> {
        wire::png(bytes)?;
        let (mut descriptor, blob) = attachment::seal(bytes)?;
        descriptor.mime = "image/png".into();
        descriptor.width = width;
        descriptor.height = height;
        let result = self
            .session
            .submit_attachment_to(
                &mut self.nodes,
                &self.owner,
                &self.peer,
                &descriptor,
                &blob,
                &self.seal,
            )
            .await?;
        let mut reply = wire::submitted(&result);
        reply["sha256"] = serde_json::json!(descriptor.plaintext_sha256);
        Ok(reply)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [address, database, peer] = args.as_slice() else {
        return Err(
            "usage: android_photo_fixture LOOPBACK_MULTIADDR ABSOLUTE_NEW_DATABASE PEER_ID".into(),
        );
    };
    let mut fixture = Fixture::start(address, Path::new(database), peer.clone()).await?;
    wire::reply(&serde_json::json!({"ready": true, "peer": fixture.owner.peer_id()}))?;
    loop {
        // A blocked controller must not block the current-thread libp2p runtime.
        let command = tokio::task::spawn_blocking(wire::read).await??;
        let Some(command) = command else { break };
        let response = match command {
            Command::Receive {} => fixture.receive().await?,
            Command::Text {} => fixture.text().await?,
            Command::Photo {
                bytes,
                width,
                height,
            } => fixture.photo(&bytes, width, height).await?,
            Command::Quit {} => break,
        };
        wire::reply(&response)?;
    }
    Ok(())
}
