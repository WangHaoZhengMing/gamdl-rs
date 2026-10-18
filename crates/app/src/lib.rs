//! Shared application operations for CLI and desktop.
mod enrichment;
mod queue;
pub use apple::Error as AppleError;
pub use apple::{Account, Metadata, Resource, ResourceKind, Track, UserToken};
pub use media::ToolVersion;
pub use media::{CancellationToken, LyricsFormat};
pub use queue::{Event, QueueItem, State, Summary, download, download_with_credentials};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Authentication is supplied separately so secrets never enter Config or TOML.
#[derive(Clone, Default)]
pub enum Credentials {
    #[default]
    CookieFile,
    Browser(UserToken),
}

async fn session(
    config: &Config,
    credentials: &Credentials,
) -> Result<(apple::Session, Account), Error> {
    Ok(match credentials {
        Credentials::CookieFile => {
            let cookies = tokio::fs::read_to_string(&config.cookies_path).await?;
            apple::check_session(&cookies).await?
        }
        Credentials::Browser(token) => apple::check_user_token(token).await?,
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub cookies_path: PathBuf,
    pub output_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub language: String,
    pub jobs: usize,
    pub wvd_path: Option<PathBuf>,
    pub save_cover: bool,
    pub save_lyrics: bool,
    pub lyrics_format: LyricsFormat,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            cookies_path: "cookies.txt".into(),
            output_path: dirs::download_dir()
                .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
                .unwrap_or_else(|| PathBuf::from("Downloads"))
                .join("AMd"),
            ffmpeg_path: "ffmpeg".into(),
            language: "en-US".into(),
            jobs: 2,
            wvd_path: None,
            save_cover: false,
            save_lyrics: false,
            lyrics_format: LyricsFormat::Lrc,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("jobs must be between 1 and 16")]
    InvalidJobs,
    #[error("an active Apple Music subscription is required")]
    SubscriptionRequired,
    #[error("no supported songs were found")]
    EmptyQueue,
    #[error("download worker stopped unexpectedly")]
    Worker,
    #[error(transparent)]
    Apple(#[from] apple::Error),
    #[error(transparent)]
    Media(#[from] media::Error),
    #[error("could not read local file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid TOML configuration: {0}")]
    Config(#[from] toml::de::Error),
}

pub async fn load_config(path: Option<&Path>) -> Result<Config, Error> {
    match path {
        Some(path) => Ok(toml::from_str(&tokio::fs::read_to_string(path).await?)?),
        None => Ok(Config::default()),
    }
}

pub fn parse_url(input: &str) -> Result<Resource, Error> {
    Ok(apple::parse_url(input)?)
}

pub async fn check_auth(config: &Config) -> Result<Account, Error> {
    check_auth_with_credentials(config, &Credentials::CookieFile).await
}

pub async fn check_auth_with_credentials(
    config: &Config,
    credentials: &Credentials,
) -> Result<Account, Error> {
    let (_, account) = session(config, credentials).await?;
    Ok(account)
}

pub struct Resolution {
    pub account: Account,
    pub tracks: Vec<Track>,
}

pub async fn resolve(config: &Config, urls: &[String]) -> Result<Resolution, Error> {
    resolve_with_credentials(config, urls, &Credentials::CookieFile).await
}

pub async fn resolve_with_credentials(
    config: &Config,
    urls: &[String],
    credentials: &Credentials,
) -> Result<Resolution, Error> {
    let resources = urls
        .iter()
        .map(|url| parse_url(url))
        .collect::<Result<Vec<_>, _>>()?;
    if resources.is_empty() {
        return Err(Error::EmptyQueue);
    }
    let (session, account) = session(config, credentials).await?;
    if !account.active_subscription {
        return Err(Error::SubscriptionRequired);
    }
    let mut seen = std::collections::HashSet::new();
    let mut tracks = Vec::new();
    for resource in resources {
        for track in apple::tracks(&session, &resource, &config.language).await? {
            if seen.insert(track.id.clone()) {
                tracks.push(track);
            }
        }
    }
    if tracks.is_empty() {
        return Err(Error::EmptyQueue);
    }
    Ok(Resolution { account, tracks })
}

pub async fn inspect(config: &Config, url: &str) -> Result<Metadata, Error> {
    let resource = parse_url(url)?;
    let cookies = tokio::fs::read_to_string(&config.cookies_path).await?;
    let (session, _) = apple::check_session(&cookies).await?;
    Ok(apple::inspect(&session, resource, &config.language).await?)
}

pub async fn check_ffmpeg(config: &Config) -> Result<ToolVersion, Error> {
    Ok(media::check_ffmpeg(&config.ffmpeg_path).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolution_validates_all_links_before_reading_credentials() {
        let config = Config {
            cookies_path: "missing-cookies-for-resolution-test.txt".into(),
            ..Config::default()
        };
        assert!(matches!(
            resolve(&config, &[]).await,
            Err(Error::EmptyQueue)
        ));
        let urls = vec![
            "https://music.apple.com/us/song/1440812303".into(),
            "https://example.com/not-apple-music".into(),
        ];
        assert!(matches!(
            resolve(&config, &urls).await,
            Err(Error::Apple(apple::Error::InvalidUrl(_)))
        ));
    }
}
