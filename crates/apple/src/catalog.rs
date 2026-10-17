use crate::{Error, Resource, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub resource: Resource,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_count: Option<u64>,
}

#[derive(Deserialize)]
struct CatalogResponse {
    data: Vec<CatalogItem>,
}
#[derive(Deserialize)]
struct CatalogItem {
    attributes: Attributes,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attributes {
    name: String,
    artist_name: Option<String>,
    album_name: Option<String>,
    track_count: Option<u64>,
}

pub async fn inspect(
    session: &Session,
    resource: Resource,
    language: &str,
) -> Result<Metadata, Error> {
    let endpoint = format!(
        "https://amp-api.music.apple.com/v1/catalog/{}/{}/{}",
        resource.storefront,
        resource.kind.endpoint(),
        resource.id
    );
    let response: CatalogResponse = session
        .client
        .get(endpoint)
        .query(&[("l", language)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let attributes = response
        .data
        .into_iter()
        .next()
        .ok_or(Error::InvalidResponse("catalog item"))?
        .attributes;
    Ok(Metadata {
        title: attributes.name,
        artist: attributes.artist_name,
        album: attributes.album_name,
        track_count: attributes.track_count,
        resource,
    })
}
