mod faults;
mod safety;

use super::{root, slot, FIRST};
use crate::identity_slots::{Credential, KeyReader, KeyRemover, Layout, Registry, Slot, Storage};
use dyapp_identity::Identity;
use std::cell::{Cell, RefCell};
use std::fs::{self, OpenOptions, TryLockError};
use std::path::PathBuf;

struct Backend {
    identity: Identity,
    present: RefCell<[bool; 3]>,
    erased: RefCell<Vec<Credential>>,
    fail: Cell<Option<Credential>>,
    lie: Cell<bool>,
    denied: Cell<bool>,
    stale_open: Cell<bool>,
    lock: PathBuf,
}

fn index(role: Credential) -> usize {
    match role {
        Credential::Seal => 0,
        Credential::Device => 1,
        Credential::Identity => 2,
    }
}

impl KeyReader for Backend {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        if self.denied.get() {
            return Err("denied".into());
        }
        Ok(self.present.borrow()[2])
    }

    fn open(&self, slot: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        self.contains(slot).map(|present| {
            (present || self.stale_open.get())
                .then(|| Identity::from_secret(&self.identity.secret()))
        })
    }
}

impl KeyRemover for Backend {
    fn credential_present(&self, _: &Slot, role: Credential) -> Result<bool, String> {
        if self.denied.get() {
            return Err("denied".into());
        }
        Ok(self.present.borrow()[index(role)])
    }

    fn erase_credential(&self, _: &Slot, role: Credential) -> Result<(), String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.lock)
            .unwrap();
        assert!(matches!(
            file.try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
        self.erased.borrow_mut().push(role);
        if self.fail.get() == Some(role) {
            return Err("erase failed".into());
        }
        if !self.lie.get() {
            self.present.borrow_mut()[index(role)] = false;
        }
        Ok(())
    }
}

fn fixture(layout: Layout, storage: Storage) -> (PathBuf, Registry, Backend) {
    let root = root();
    let dir = slot(&root, FIRST, b"ready\n");
    let identity = Identity::generate();
    let bytes = if storage == Storage::File {
        identity.secret().to_vec()
    } else {
        b"wrapped".to_vec()
    };
    if storage != Storage::System {
        fs::write(dir.join("identity.key"), bytes).unwrap();
    }
    #[cfg(unix)]
    if storage == Storage::File {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.join("identity.key"), fs::Permissions::from_mode(0o600)).unwrap();
    }
    if layout == Layout::Desktop {
        fs::write(
            dir.join("storage-mode"),
            if storage == Storage::File {
                b"file\n".as_slice()
            } else {
                b"system\n"
            },
        )
        .unwrap();
    }
    fs::write(root.join("active-profile"), format!("{FIRST}\n")).unwrap();
    let lock = root.join(if layout == Layout::Ios {
        ".identity-slots.lock"
    } else {
        "profiles.lock"
    });
    let present = match storage {
        Storage::System => [true, true, true],
        Storage::Wrapped => [false, false, true],
        Storage::File => [false, false, false],
    };
    let backend = Backend {
        identity,
        present: RefCell::new(present),
        erased: RefCell::new(Vec::new()),
        fail: Cell::new(None),
        lie: Cell::new(false),
        denied: Cell::new(false),
        stale_open: Cell::new(false),
        lock,
    };
    (root.clone(), Registry::new(root, layout), backend)
}
