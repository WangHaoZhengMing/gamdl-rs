//! Apple Music URLs and authenticated catalog access.
mod auth;
mod catalog;
mod playback;
mod resource;
mod tracks;
pub use playback::{Playback, exchange_license, playback};
pub use tracks::{Track, tracks};

pub use auth::{Account, Session, check_session};
pub use catalog::{Metadata, inspect};
pub use resource::{Resource, ResourceKind, parse_url};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("playback unavailable for this track or account")]
    PlaybackUnavailable,
    #[error("license exchange was rejected")]
    LicenseRejected,
    #[error("invalid catalog pagination URL")]
    InvalidPagination,
    #[error("invalid Apple Music URL: {0}")]
    InvalidUrl(&'static str),
    #[error("invalid cookie file: {0}")]
    InvalidCookies(&'static str),
    #[error("cookie file has expired")]
    ExpiredCookies,
    #[error("Apple Music page format changed; developer token unavailable")]
    TokenUnavailable,
    #[error("Apple Music response is missing {0}")]
    InvalidResponse(&'static str),
    #[error("network request failed: {0}")]
    Http(#[from] reqwest::Error),
}
