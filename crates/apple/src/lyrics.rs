use crate::{Error, Session, Track};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub enum LyricsLevel {
    Word,
    Normal,
}
#[derive(Deserialize, Default)]
struct Page {
    #[serde(default)]
    data: Vec<Item>,
}
#[derive(Deserialize)]
struct Item {
    attributes: Option<Attributes>,
    #[serde(default)]
    relationships: HashMap<String, Page>,
}
#[derive(Deserialize)]
struct Attributes {
    ttml: Option<String>,
}

pub async fn lyric_ttml(
    session: &Session,
    track: &Track,
    language: &str,
    level: LyricsLevel,
) -> Result<Option<String>, Error> {
    let relationship = match level {
        LyricsLevel::Word => "syllable-lyrics",
        LyricsLevel::Normal => "lyrics",
    };
    let endpoint = format!(
        "https://amp-api.music.apple.com/v1/catalog/{}/songs/{}",
        track.storefront, track.id
    );
    let page: Page = session
        .client
        .get(endpoint)
        .query(&[
            ("l", language),
            ("include", relationship),
            ("include[songs]", relationship),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(page
        .data
        .into_iter()
        .flat_map(|item| {
            item.relationships
                .into_iter()
                .filter(|(key, _)| key == relationship)
                .flat_map(|(_, page)| page.data)
        })
        .filter_map(|item| item.attributes.and_then(|attributes| attributes.ttml))
        .find(|ttml| !ttml.trim().is_empty()))
}
