//! Shared application operations for CLI and desktop.
mod queue;
pub use apple::{Account, Metadata, Resource, ResourceKind};
pub use media::CancellationToken;
pub use media::ToolVersion;
pub use queue::{Event, QueueItem, State, Summary, download};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub cookies_path: PathBuf,
    pub output_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub language: String,
    pub jobs: usize,
    pub wvd_path: Option<PathBuf>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            cookies_path: "cookies.txt".into(),
            output_path: "Apple Music".into(),
            ffmpeg_path: "ffmpeg".into(),
            language: "en-US".into(),
            jobs: 2,
            wvd_path: None,
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
    let cookies = tokio::fs::read_to_string(&config.cookies_path).await?;
    let (_, account) = apple::check_session(&cookies).await?;
    Ok(account)
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
