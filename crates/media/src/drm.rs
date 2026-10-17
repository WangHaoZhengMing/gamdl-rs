use crate::Error;
use std::path::Path;

// No Debug or Serialize: never expose key material in logs or events.
pub struct ContentKey(pub(crate) [u8; 16]);
impl Drop for ContentKey {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}
impl ContentKey {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

pub struct LicenseRequest {
    #[cfg(feature = "widevine")]
    request: widevine::CdmLicenseRequest,
    #[cfg(feature = "widevine")]
    kid: Option<[u8; 16]>,
}

pub fn license_request(wvd: Option<&Path>, pssh: &str) -> Result<LicenseRequest, Error> {
    #[cfg(feature = "widevine")]
    {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let input = pssh.rsplit(',').next().ok_or(Error::InvalidKey)?;
        let bytes = STANDARD.decode(input).map_err(|_| Error::InvalidKey)?;
        let bytes = if bytes.get(4..8) == Some(b"pssh") {
            bytes
        } else {
            // Wrap raw init data in a PSSH box so the CDM preserves protobuf
            // field ordering (including the legacy algorithm field).
            let short_kid = (bytes.len() == 16).then_some(bytes.as_slice());
            let mut init = Vec::new();
            if short_kid.is_some() {
                init.extend_from_slice(&[0x08, 0x01, 0x12, 0x10]);
            }
            init.extend_from_slice(&bytes);
            let size = 32usize + if short_kid.is_some() { 20 } else { 0 } + init.len();
            let size = u32::try_from(size).map_err(|_| Error::InvalidKey)?;
            let mut boxed = Vec::with_capacity(size as usize);
            boxed.extend_from_slice(&size.to_be_bytes());
            boxed.extend_from_slice(b"pssh");
            boxed.extend_from_slice(
                &(if short_kid.is_some() { 1u32 << 24 } else { 0 }).to_be_bytes(),
            );
            boxed.extend_from_slice(&[
                0xed, 0xef, 0x8b, 0xa9, 0x79, 0xd6, 0x4a, 0xce, 0xa3, 0xc8, 0x27, 0xdc, 0xd5, 0x1d,
                0x21, 0xed,
            ]);
            if let Some(kid) = short_kid {
                boxed.extend_from_slice(&1u32.to_be_bytes());
                boxed.extend_from_slice(kid);
            }
            boxed.extend_from_slice(&(init.len() as u32).to_be_bytes());
            boxed.extend_from_slice(&init);
            boxed
        };
        let pssh = widevine::Pssh::from_bytes(&bytes).map_err(|_| Error::InvalidKey)?;
        let kid = pssh.key_ids.first().copied();
        let device = match wvd {
            Some(path) => {
                widevine::Device::read_wvd(std::io::BufReader::new(std::fs::File::open(path)?))
            }
            None => widevine::Device::read_wvd(std::io::Cursor::new(include_bytes!(
                "../assets/default.wvd"
            ))),
        }
        .map_err(|_| Error::Cdm)?;
        let request = widevine::Cdm::new(device)
            .open()
            .get_license_request(pssh, widevine::LicenseType::STREAMING)
            .map_err(|_| Error::Cdm)?;
        Ok(LicenseRequest { request, kid })
    }
    #[cfg(not(feature = "widevine"))]
    {
        let _ = (wvd, pssh);
        Err(Error::CdmRequired)
    }
}

impl LicenseRequest {
    pub fn challenge(&self) -> Result<Vec<u8>, Error> {
        #[cfg(feature = "widevine")]
        {
            self.request.challenge().map_err(|_| Error::Cdm)
        }
        #[cfg(not(feature = "widevine"))]
        {
            Err(Error::CdmRequired)
        }
    }
    pub fn finish(self, license: &[u8]) -> Result<ContentKey, Error> {
        #[cfg(feature = "widevine")]
        {
            let keys = self.request.get_keys(license).map_err(|_| Error::Cdm)?;
            let key = match self.kid {
                Some(kid) => keys.content_key(&kid),
                None => keys.first_of_type(widevine::KeyType::CONTENT),
            }
            .map_err(|_| Error::Cdm)?;
            Ok(ContentKey(
                key.key
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::InvalidKey)?,
            ))
        }
        #[cfg(not(feature = "widevine"))]
        {
            let _ = license;
            Err(Error::CdmRequired)
        }
    }
}

#[cfg(all(test, feature = "widevine"))]
mod tests {
    use super::*;
    const PSSH: &str = "data:text/plain;base64,AgICAgICAgICAgICAgICAg==";

    #[test]
    fn bundled_device_generates_a_license_challenge_without_a_file() {
        let request = license_request(None, PSSH).unwrap();
        assert!(!request.challenge().unwrap().is_empty());
    }

    #[test]
    fn explicit_device_path_does_not_silently_fall_back() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.wvd");
        assert!(matches!(
            license_request(Some(&missing), PSSH),
            Err(Error::Io(_))
        ));
        let invalid = directory.path().join("invalid.wvd");
        std::fs::write(&invalid, b"invalid device").unwrap();
        assert!(matches!(
            license_request(Some(&invalid), PSSH),
            Err(Error::Cdm)
        ));
    }
}
