use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(super) fn fresh(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())
}

pub(super) fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let temp = std::path::PathBuf::from(name);
    // A stranded temporary file fails closed; it is never followed, truncated or deleted.
    fresh(&temp, bytes)?;
    fs::rename(&temp, path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
