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
        let (mode, failures, verifier) = match format {
            Format::Android => match data {
                [1, failures, verifier @ ..]
                    if verifier.len() == 49 && verifier.first() == Some(&1) =>
                {
                    (Mode::Pin, *failures, verifier)
                }
                _ => return Err(Error::InvalidRecord),
            },
            Format::Ios => match data {
                [1, 0, failures] => (Mode::Off, *failures, &[][..]),
                [1, 1, failures, verifier @ ..] if verifier.len() == 49 => {
                    (Mode::Pin, *failures, verifier)
                }
                [1, 2, failures] => (Mode::Biometric, *failures, &[][..]),
                _ => return Err(Error::InvalidRecord),
            },
        };
        if failures > MAX_FAILURES {
            return Err(Error::InvalidRecord);
        }
        Ok(Self {
            mode,
            failures,
            verifier: Zeroizing::new(verifier.to_vec()),
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
