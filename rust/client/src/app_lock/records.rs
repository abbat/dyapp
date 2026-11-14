use super::{Error, Format, Mode};
use zeroize::Zeroizing;

pub(super) const MAX_FAILURES: u8 = 5;

pub(super) struct Saved {
    pub(super) mode: Mode,
    pub(super) failures: u8,
    pub(super) verifier: Zeroizing<Vec<u8>>,
}

impl Saved {
    pub(super) fn pin(verifier: Vec<u8>) -> Self {
        Self {
            mode: Mode::Pin,
            failures: 0,
            verifier: Zeroizing::new(verifier),
        }
    }

    pub(super) fn mode(mode: Mode) -> Self {
        Self {
            mode,
            failures: 0,
            verifier: Zeroizing::new(Vec::new()),
        }
    }

    pub(super) fn decode(format: Format, data: &[u8]) -> Result<Self, Error> {
        if data.first() != Some(&1) {
            return Err(Error::InvalidRecord);
        }
        let (mode, failures, start) = match format {
            Format::Android if data.len() == 51 && data[2] == 1 => (Mode::Pin, data[1], 2),
            Format::Ios if data.len() >= 3 => {
                let mode = match data[1] {
                    0 => Mode::Off,
                    1 => Mode::Pin,
                    2 => Mode::Biometric,
                    _ => return Err(Error::InvalidRecord),
                };
                let size = if mode == Mode::Pin { 52 } else { 3 };
                if data.len() != size {
                    return Err(Error::InvalidRecord);
                }
                (mode, data[2], 3)
            }
            Format::Android | Format::Ios => return Err(Error::InvalidRecord),
        };
        if failures > MAX_FAILURES {
            return Err(Error::InvalidRecord);
        }
        Ok(Self {
            mode,
            failures,
            verifier: Zeroizing::new(data[start..].to_vec()),
        })
    }

    pub(super) fn encode(&self, format: Format) -> Result<Zeroizing<Vec<u8>>, Error> {
        let record_size = if self.mode == Mode::Pin { 49 } else { 0 };
        if self.verifier.len() != record_size || self.failures > MAX_FAILURES {
            return Err(Error::InvalidRecord);
        }
        let mut data = Zeroizing::new(match (format, self.mode) {
            (Format::Android, Mode::Pin) => vec![1, self.failures],
            (Format::Android, _) | (Format::Ios, Mode::New) => return Err(Error::InvalidRecord),
            (Format::Ios, Mode::Off) => vec![1, 0, self.failures],
            (Format::Ios, Mode::Pin) => vec![1, 1, self.failures],
            (Format::Ios, Mode::Biometric) => vec![1, 2, self.failures],
        });
        data.extend_from_slice(&self.verifier);
        Ok(data)
    }
}
