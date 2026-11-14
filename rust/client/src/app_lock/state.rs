use super::records::{Saved, MAX_FAILURES};
use super::{pin, Attempt, Entry, Error, Format, Mode, Vault};
use std::sync::{Arc, Mutex, MutexGuard};
use zeroize::Zeroizing;

#[cfg(test)]
mod tests;

/// App-wide lock transitions, serialized through actual protected IO/KDF completion.
/// Reuse a handle, or additionally serialize vault access across handles on the platform.
pub struct AppLock {
    format: Format,
    vault: Arc<dyn Vault>,
    operation: Mutex<()>,
}

impl AppLock {
    /// Bind an existing storage layout without reading, changing or migrating its records.
    #[must_use]
    pub fn new(format: Format, vault: Arc<dyn Vault>) -> Self {
        Self {
            format,
            vault,
            operation: Mutex::new(()),
        }
    }

    fn guard(&self) -> Result<MutexGuard<'_, ()>, Error> {
        self.operation.lock().map_err(|_| Error::Poisoned)
    }

    fn load(&self) -> Result<Option<Saved>, Error> {
        self.vault
            .read(Entry::Record)?
            .map(|data| {
                let data = Zeroizing::new(data);
                Saved::decode(self.format, &data)
            })
            .transpose()
    }

    fn store(&self, saved: &Saved) -> Result<(), Error> {
        self.vault.write(Entry::Record, &saved.encode(self.format)?)
    }

    fn mode_inner(&self) -> Result<Mode, Error> {
        if let Some(saved) = self.load()? {
            return Ok(saved.mode);
        }
        if self.format == Format::Android {
            if self.vault.read(Entry::Biometric)?.is_some() {
                return Ok(Mode::Biometric);
            }
            if self.vault.read(Entry::Skipped)?.is_some() {
                return Ok(Mode::Off);
            }
        }
        Ok(Mode::New)
    }

    /// Read the existing choice; a corrupt record is an error and never fresh setup.
    ///
    /// # Errors
    /// Returns invalid-record, storage or synchronization failure.
    pub fn mode(&self) -> Result<Mode, Error> {
        let _guard = self.guard()?;
        self.mode_inner()
    }

    /// Enroll only when no PIN exists; authenticated replacement uses recovery instead.
    ///
    /// # Errors
    /// Returns existing-PIN, invalid-record, PIN, storage or synchronization failure.
    pub fn enroll(&self, value: String) -> Result<(), Error> {
        let value = Zeroizing::new(value);
        let _guard = self.guard()?;
        if self.load()?.is_some_and(|saved| saved.mode == Mode::Pin) {
            return Err(Error::AlreadyEnrolled);
        }
        self.store(&Saved::pin(pin::hash(value.to_string())?))
    }

    /// Persist a consumed attempt before verification, and reset before reporting success.
    ///
    /// # Errors
    /// Returns missing-PIN, invalid-record, PIN, storage or synchronization failure.
    /// Even a verifier input/error consumes the already-persisted attempt.
    pub fn attempt(&self, value: String) -> Result<Attempt, Error> {
        let value = Zeroizing::new(value);
        let _guard = self.guard()?;
        self.attempt_inner(&value)
    }

    fn attempt_inner(&self, value: &str) -> Result<Attempt, Error> {
        self.attempt_with(value, |value, record| pin::verify(value.to_owned(), record))
    }

    // Private injection boundary tests persistence before/after the real verifier call.
    fn attempt_with(
        &self,
        value: &str,
        verify: impl FnOnce(&str, &[u8]) -> Result<bool, pin::Error>,
    ) -> Result<Attempt, Error> {
        let mut saved = self
            .load()?
            .filter(|saved| saved.mode == Mode::Pin)
            .ok_or(Error::NotEnrolled)?;
        if saved.failures >= MAX_FAILURES {
            return Ok(Attempt::DeviceCredentialRequired);
        }
        saved.failures += 1;
        self.store(&saved)?;
        if verify(value, &saved.verifier)? {
            saved.failures = 0;
            self.store(&saved)?;
            return Ok(Attempt::Unlocked);
        }
        Ok(if saved.failures == MAX_FAILURES {
            Attempt::DeviceCredentialRequired
        } else {
            Attempt::Wrong
        })
    }

    /// Change or disable only after authenticating the current PIN with normal attempt limits.
    ///
    /// # Errors
    /// Returns the same errors as attempt/recovery. A protected-IO error does not prove rollback.
    pub fn change(&self, current: String, new_pin: Option<String>) -> Result<Attempt, Error> {
        let current = Zeroizing::new(current);
        let new_pin = new_pin.map(Zeroizing::new);
        let _guard = self.guard()?;
        let result = self.attempt_inner(&current)?;
        if result == Attempt::Unlocked {
            self.reset_inner(new_pin.as_ref().map(|value| value.as_str()))?;
        }
        Ok(result)
    }

    /// Replace/disable only after fresh OS authentication performed by the platform caller.
    /// Core does not authenticate with the OS or manufacture evidence of that authentication.
    ///
    /// # Errors
    /// Returns not-enabled, invalid-record, PIN, storage or synchronization failure.
    pub fn reset_after_device_credential(&self, new_pin: Option<String>) -> Result<(), Error> {
        let new_pin = new_pin.map(Zeroizing::new);
        let _guard = self.guard()?;
        self.reset_inner(new_pin.as_ref().map(|value| value.as_str()))
    }

    fn reset_inner(&self, new_pin: Option<&str>) -> Result<(), Error> {
        let mode = self.mode_inner()?;
        if !matches!(mode, Mode::Pin | Mode::Biometric) {
            return Err(Error::NotEnabled);
        }
        if let Some(value) = new_pin {
            self.store(&Saved::pin(pin::hash(value.to_owned())?))?;
        } else if self.format == Format::Ios {
            self.store(&Saved::mode(Mode::Off))?;
        } else {
            if mode == Mode::Pin {
                self.vault.remove(Entry::Record)?;
            }
            self.vault.write(Entry::Skipped, &[])?;
        }
        if self.format == Format::Android {
            self.vault.remove(Entry::Biometric)?;
        }
        Ok(())
    }

    /// Store Skip without disabling a lock. Android retains its separate marker; iOS writes
    /// Off only when its single protected item is absent.
    ///
    /// # Errors
    /// Returns invalid-record, storage or synchronization failure.
    pub fn skip(&self) -> Result<(), Error> {
        let _guard = self.guard()?;
        match self.format {
            Format::Android => self.vault.write(Entry::Skipped, &[]),
            Format::Ios if self.load()?.is_none() => self.store(&Saved::mode(Mode::Off)),
            Format::Ios => Ok(()),
        }
    }

    /// Choose biometric/device authentication on fresh setup, while off, or after fresh OS
    /// authentication. Android writes the durable marker before removing a PIN, whose presence
    /// continues to take precedence across a failed/interrupted switch.
    ///
    /// # Errors
    /// Returns invalid-record, storage or synchronization failure.
    pub fn use_biometric(&self) -> Result<(), Error> {
        let _guard = self.guard()?;
        if self.format == Format::Ios {
            return self.store(&Saved::mode(Mode::Biometric));
        }
        self.vault.write(Entry::Biometric, &[])?;
        if self.load()?.is_some() {
            self.vault.remove(Entry::Record)?;
        }
        Ok(())
    }
}
