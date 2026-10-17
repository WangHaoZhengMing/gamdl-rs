use crate::Error;
use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Song,
    Album,
    Playlist,
}

impl ResourceKind {
    pub(crate) fn endpoint(self) -> &'static str {
        match self {
            Self::Song => "songs",
            Self::Album => "albums",
            Self::Playlist => "playlists",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resource {
    pub storefront: String,
    pub kind: ResourceKind,
    pub id: String,
}

pub fn parse_url(input: &str) -> Result<Resource, Error> {
    let url = Url::parse(input).map_err(|_| Error::InvalidUrl("expected an absolute URL"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("music.apple.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(Error::InvalidUrl("expected https://music.apple.com"));
    }
    let parts: Vec<_> = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .collect();
    if !(parts.len() == 3 || parts.len() == 4) {
        return Err(Error::InvalidUrl("expected storefront/type/[title/]id"));
    }
    let storefront = parts[0];
    if storefront.len() != 2 || !storefront.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(Error::InvalidUrl("invalid storefront"));
    }
    let mut kind = match parts[1] {
        "song" => ResourceKind::Song,
        "album" => ResourceKind::Album,
        "playlist" => ResourceKind::Playlist,
        _ => return Err(Error::InvalidUrl("supported types: song, album, playlist")),
    };
    let mut id = *parts.last().ok_or(Error::InvalidUrl("missing id"))?;
    let song_ids: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key == "i")
        .map(|(_, value)| value.into_owned())
        .collect();
    if song_ids.len() > 1 {
        return Err(Error::InvalidUrl("ambiguous song id"));
    }
    if kind == ResourceKind::Album
        && let Some(song) = song_ids.first()
    {
        kind = ResourceKind::Song;
        id = song;
    }
    let valid = match kind {
        ResourceKind::Playlist => {
            id.starts_with("pl.")
                && id.len() > 3
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        }
        _ => !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()),
    };
    if !valid {
        return Err(Error::InvalidUrl("invalid resource id"));
    }
    Ok(Resource {
        storefront: storefront.into(),
        kind,
        id: id.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn album_song_query_selects_song() {
        let item = parse_url("https://music.apple.com/us/album/title/123?i=456&l=en").unwrap();
        assert_eq!(item.kind, ResourceKind::Song);
        assert_eq!(item.id, "456");
    }
    #[test]
    fn rejects_untrusted_hosts_and_path_injection() {
        for url in [
            "https://music.apple.com.evil.test/us/song/123",
            "http://music.apple.com/us/song/123",
            "https://music.apple.com/us/song/%2F123",
            "https://music.apple.com/us/album/123?i=1&i=2",
        ] {
            assert!(parse_url(url).is_err(), "{url}");
        }
    }
    #[test]
    fn supports_album_and_playlist_without_slug() {
        assert_eq!(
            parse_url("https://music.apple.com/cn/album/123")
                .unwrap()
                .kind,
            ResourceKind::Album
        );
        assert_eq!(
            parse_url("https://music.apple.com/us/playlist/name/pl.abc-123")
                .unwrap()
                .id,
            "pl.abc-123"
        );
    }
}
