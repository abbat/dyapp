// Shows the peer ID that the Rust core derives from this run's identity key.
window.peerIdShown = window.__TAURI__.core.invoke('peer_id').then((id) => {
  document.getElementById('peer-id').textContent = id;
});
