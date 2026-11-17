use super::*;
use crate::identity_slots::{KeyWriter, LEGACY};

fn legacy_binding(f: &Fixture) -> (ChatBinding, String) {
    let root = f.keys.lock.parent().unwrap();
    let legacy = Slot {
        id: LEGACY.into(),
        dir: root.into(),
        state: State::Ready,
        storage: Some(Storage::Wrapped),
    };
    let owner = Identity::try_generate().unwrap();
    let wrapped = f.keys.base.store(&legacy, &owner).unwrap().unwrap();
    fs::write(root.join("identity.key"), wrapped).unwrap();
    let operation = f
        .registry
        .chat_operation(LEGACY, &owner.peer_id(), &f.keys, &f.keys.base)
        .unwrap();
    let store = Store::open(&root.join("client.db"), operation.identity()).unwrap();
    drop(
        Session::open_or_create(
            store,
            operation.identity(),
            operation.device(),
            operation.state_key(),
        )
        .unwrap(),
    );
    (operation.binding(), owner.peer_id())
}

#[test]
fn valid_legacy_binding_cannot_remove_the_shared_installation_root() {
    let f = Fixture::new();
    let (binding, legacy_owner) = legacy_binding(&f);
    let root = f.keys.lock.parent().unwrap();
    fs::write(root.join("app-lock"), b"installation lock").unwrap();
    fs::write(root.join("unknown-file"), b"preserve unknown data").unwrap();
    let retained: Vec<_> = [
        "identity.key",
        "device.key",
        "chat-state.key",
        "client.db",
        "active-profile",
        "app-lock",
        "unknown-file",
    ]
    .into_iter()
    .map(|name| (root.join(name), fs::read(root.join(name)).unwrap()))
    .collect();
    assert_eq!(
        f.registry
            .remove_bound_chat(&binding, &f.keys, &f.keys)
            .unwrap_err(),
        "This identity cannot be removed"
    );
    for (path, bytes) in retained {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    assert!(!root.join("state").exists());
    assert!(f.slot.dir.is_dir());
    assert_eq!(fs::read(f.slot.dir.join("identity.key")).unwrap(), vec![1]);
    f.unchanged();
    let reopened = f
        .registry
        .existing_chat_operation(LEGACY, &legacy_owner, &f.keys, &f.keys.base)
        .unwrap();
    assert_eq!(reopened.identity().peer_id(), legacy_owner);
}
