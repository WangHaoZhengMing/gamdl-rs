//! Apple Music URLs and authenticated catalog access.
mod auth;
mod catalog;
mod lyrics;
mod playback;
mod resource;
mod tracks;
pub use lyrics::{LyricsLevel, lyric_ttml};
pub use playback::{Playback, exchange_license, playback};
pub use tracks::{Track, tracks};

pub use auth::{Account, Session, UserToken, check_session, check_user_token};
pub use catalog::{Metadata, inspect};
pub use resource::{Resource, ResourceKind, parse_url};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Apple Music 返回 3076：这首歌在当前账号中不可播放。请检查账号地区与歌曲可用性；修改链接地区不会改变账号地区。"
    )]
    TrackUnavailable,
    #[error("Apple Music 拒绝播放请求（错误代码 {code}）")]
    PlaybackRejected { code: String },
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
