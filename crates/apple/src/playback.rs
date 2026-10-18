use crate::{Error, Session};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use url::Url;

pub struct Playback {
    pub url: Url,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    failure_type: Option<String>,
    #[serde(default)]
    song_list: Vec<Song>,
}
#[derive(Deserialize)]
struct Song {
    #[serde(default)]
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    flavor: String,
    #[serde(rename = "URL")]
    url: Url,
}

pub async fn playback(session: &Session, id: &str, language: &str) -> Result<Playback, Error> {
    let response: Response = session
        .client
        .post("https://play.itunes.apple.com/WebObjects/MZPlay.woa/wa/webPlayback")
        .json(&serde_json::json!({"salableAdamId":id,"language":language}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if let Some(code) = response.failure_type {
        if code == "3076" {
            return Err(Error::TrackUnavailable);
        }
        return Err(Error::PlaybackRejected { code });
    }
    let asset = response
        .song_list
        .into_iter()
        .next()
        .into_iter()
        .flat_map(|song| song.assets)
        .find(|asset| asset.flavor == "28:ctrp256")
        .ok_or(Error::PlaybackUnavailable)?;
    if asset.url.scheme() != "https" {
        return Err(Error::PlaybackUnavailable);
    }
    Ok(Playback { url: asset.url })
}

#[derive(Deserialize)]
struct License {
    status: i64,
    license: Option<String>,
}

pub async fn exchange_license(
    session: &Session,
    id: &str,
    uri: &str,
    challenge: &[u8],
) -> Result<Vec<u8>, Error> {
    let response: License = session.client.post("https://play.itunes.apple.com/WebObjects/MZPlay.woa/wa/acquireWebPlaybackLicense")
        .json(&serde_json::json!({"challenge":STANDARD.encode(challenge),"key-system":"com.widevine.alpha","uri":uri,"adamId":id,"isLibrary":false,"user-initiated":true}))
        .send().await?.error_for_status()?.json().await?;
    if response.status != 0 {
        return Err(Error::LicenseRejected);
    }
    STANDARD
        .decode(response.license.ok_or(Error::LicenseRejected)?)
        .map_err(|_| Error::LicenseRejected)
}
