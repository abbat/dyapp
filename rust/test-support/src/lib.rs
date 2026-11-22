//! Filesystem-backed test directories with deterministic cleanup.

use std::ops::Deref;
use std::path::Path;

/// A unique test directory that is removed on drop.
pub struct TestDir(tempfile::TempDir);

impl TestDir {
    /// Return the directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// Create a unique directory under the repository's build tree, removed on drop.
///
/// # Errors
///
/// Returns an error when the repository's `build/` directory is unavailable or not writable.
pub fn test_dir() -> std::io::Result<TestDir> {
    let build = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build");
    tempfile::Builder::new()
        .prefix("test-")
        .tempdir_in(build)
        .map(TestDir)
}

#[cfg(test)]
mod tests {
    #[test]
    fn directories_are_unique_and_removed_on_drop() {
        let (first, second) = (super::test_dir().unwrap(), super::test_dir().unwrap());
        assert_ne!(first.path(), second.path());
        let path = first.path().to_owned();
        drop(first);
        assert!(!path.exists());
    }
}
