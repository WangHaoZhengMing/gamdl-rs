//! HLS download, AES-128/CENC decryption and verified M4A publishing.
mod drm;
mod hls;
mod mux;
mod transfer;
pub use drm::{ContentKey, LicenseRequest, license_request};
pub use hls::{Plan, Protection, prepare};
pub use mux::{Output, SaveOptions, Tags, save};
use serde::Serialize;
use std::path::Path;
pub use tokio_util::sync::CancellationToken;
pub use transfer::download;

#[derive(Debug, Serialize)]
pub struct ToolVersion {
    pub version: String,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("local media I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("media request failed: {0}")]
    Http(reqwest::Error),
    #[error("invalid or unsupported HLS: {0}")]
    Playlist(&'static str),
    #[error("download cancelled")]
    Cancelled,
    #[error("AES-128 decryption failed")]
    Aes,
    #[error("CENC requires Widevine support; rebuild with --features widevine")]
    CdmRequired,
    #[error("CDM operation failed")]
    Cdm,
    #[error("media key or IV is invalid")]
    InvalidKey,
    #[error("FFmpeg failed; output was not published")]
    Failed,
    #[error("media request or process timed out")]
    Timeout,
}
impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error.without_url())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Progress {
    Download {
        completed: usize,
        total: usize,
        bytes: u64,
    },
    Muxing,
    Verifying,
}

pub fn client() -> Result<reqwest::Client, Error> {
    Ok(reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

pub async fn check_ffmpeg(path: &Path) -> Result<ToolVersion, Error> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new(path)
            .arg("-version")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| Error::Timeout)??;
    if !output.status.success() {
        return Err(Error::Failed);
    }
    Ok(ToolVersion {
        version: String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned(),
    })
}
