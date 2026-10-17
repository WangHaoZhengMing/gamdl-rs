use crate::{Error, Resource, ResourceKind, Session};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub release_date: Option<String>,
}
#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    data: Vec<Item>,
    next: Option<String>,
}
#[derive(Deserialize)]
struct Item {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    attributes: Option<Attributes>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attributes {
    name: String,
    #[serde(default)]
    artist_name: String,
    #[serde(default)]
    album_name: String,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    release_date: Option<String>,
}

fn next_url(path: &str) -> Result<Url, Error> {
    let base = Url::parse("https://amp-api.music.apple.com").expect("constant URL");
    let url = base.join(path).map_err(|_| Error::InvalidPagination)?;
    if url.scheme() != "https"
        || url.host_str() != base.host_str()
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().starts_with("/v1/catalog/")
    {
        return Err(Error::InvalidPagination);
    }
    Ok(url)
}

pub async fn tracks(
    session: &Session,
    resource: &Resource,
    language: &str,
) -> Result<Vec<Track>, Error> {
    let suffix = if resource.kind == ResourceKind::Song {
        ""
    } else {
        "/tracks"
    };
    let mut url = next_url(&format!(
        "/v1/catalog/{}/{}/{}{suffix}",
        resource.storefront,
        resource.kind.endpoint(),
        resource.id
    ))?;
    url.query_pairs_mut().append_pair("l", language);
    let mut visited = HashSet::new();
    let mut result = Vec::new();
    loop {
        if visited.len() >= 1000 || !visited.insert(url.to_string()) {
            return Err(Error::InvalidPagination);
        }
        let page: Page = session
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        for item in page.data {
            if item.kind != "songs" {
                continue;
            }
            if !item.id.bytes().all(|b| b.is_ascii_digit()) || item.id.is_empty() {
                return Err(Error::InvalidResponse("song id"));
            }
            let Some(attributes) = item.attributes else {
                continue;
            };
            result.push(Track {
                id: item.id,
                title: attributes.name,
                artist: attributes.artist_name,
                album: attributes.album_name,
                track_number: attributes.track_number,
                disc_number: attributes.disc_number,
                release_date: attributes.release_date,
            });
        }
        let Some(next) = page.next else {
            break;
        };
        url = next_url(&next)?;
        if !url.query_pairs().any(|(key, _)| key == "l") {
            url.query_pairs_mut().append_pair("l", language);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_cannot_forward_credentials_to_another_origin() {
        for path in [
            "https://evil.test/v1/catalog/us/songs",
            "//evil.test/v1/catalog/us/songs",
            "http://amp-api.music.apple.com/v1/catalog/us/songs",
            "/other",
        ] {
            assert!(next_url(path).is_err());
        }
        assert!(next_url("/v1/catalog/us/albums/123/tracks?offset=100").is_ok());
    }
}
