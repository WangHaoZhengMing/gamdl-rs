use crate::{Error, transfer::fetch};
use m3u8_rs::{KeyMethod, Playlist};
use reqwest::Client;
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Clone, PartialEq, Eq)]
pub enum Protection {
    Clear,
    Aes128,
    Cenc { pssh: String },
}
#[derive(Clone)]
pub(crate) struct AesKey {
    pub url: Url,
    pub iv: [u8; 16],
}
#[derive(Clone)]
pub(crate) struct Piece {
    pub url: Url,
    pub range: Option<(u64, u64)>,
    pub aes: Option<AesKey>,
}
pub struct Plan {
    pub protection: Protection,
    pub(crate) pieces: Vec<Piece>,
}
impl Plan {
    pub fn segments(&self) -> usize {
        self.pieces.len()
    }
}

pub(crate) fn join(base: &Url, path: &str) -> Result<Url, Error> {
    let url = base
        .join(path)
        .map_err(|_| Error::Playlist("invalid resource URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::Playlist("only HTTP(S) resources are supported"));
    }
    Ok(url)
}

pub async fn prepare(
    client: &Client,
    mut url: Url,
    cancel: &CancellationToken,
) -> Result<Plan, Error> {
    let mut visited = std::collections::HashSet::new();
    for _ in 0..5 {
        join(&url, "")?;
        if !visited.insert(url.to_string()) {
            return Err(Error::Playlist("playlist cycle"));
        }
        let (bytes, actual) = fetch(client, &url, None, 2 * 1024 * 1024, cancel).await?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| Error::Playlist("playlist is not UTF-8"))?;
        let normalized = normalize_keys(text);
        let parsed = m3u8_rs::parse_playlist_res(normalized.as_bytes())
            .map_err(|_| Error::Playlist("could not parse manifest"))?;
        match parsed {
            Playlist::MasterPlaylist(master) => {
                let variant = master
                    .variants
                    .iter()
                    .filter(|v| !v.is_i_frame && v.resolution.is_none())
                    .max_by_key(|v| v.bandwidth)
                    .ok_or(Error::Playlist("no audio variant"))?;
                let audio = variant.audio.as_ref().and_then(|group| {
                    master
                        .alternatives
                        .iter()
                        .filter(|a| {
                            a.media_type == m3u8_rs::AlternativeMediaType::Audio
                                && &a.group_id == group
                                && a.uri.is_some()
                        })
                        .max_by_key(|a| (a.default, a.autoselect))
                        .and_then(|a| a.uri.as_deref())
                });
                url = join(&actual, audio.unwrap_or(&variant.uri))?;
            }
            Playlist::MediaPlaylist(playlist) => return build_plan(&actual, playlist),
        }
    }
    Err(Error::Playlist("too many nested playlists"))
}

// Keep Widevine's key declaration when parallel DRM descriptions refer to the same content.
// m3u8-rs also requires an IV on METHOD=NONE; the dummy IV is never used.
fn normalize_keys(text: &str) -> String {
    let has_widevine = text.lines().any(|l| {
        l.starts_with("#EXT-X-KEY:") && l.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed")
    });
    text.lines()
        .filter(|line| {
            !(has_widevine
                && line.starts_with("#EXT-X-KEY:")
                && line.contains("KEYFORMAT=")
                && !line.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed")
                && !line.contains("KEYFORMAT=\"identity\""))
        })
        .map(|line| {
            if line == "#EXT-X-KEY:METHOD=NONE" {
                format!("{line},IV=0x0")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn byte_range(
    range: Option<&m3u8_rs::ByteRange>,
    url: &Url,
    previous: &mut Option<(Url, u64)>,
) -> Result<Option<(u64, u64)>, Error> {
    let Some(range) = range else {
        *previous = None;
        return Ok(None);
    };
    let start = match range.offset {
        Some(offset) => offset,
        None => previous
            .as_ref()
            .filter(|(old, _)| old == url)
            .map(|(_, end)| *end)
            .ok_or(Error::Playlist(
                "implicit byte range without preceding matching resource",
            ))?,
    };
    let end = start
        .checked_add(range.length)
        .filter(|end| *end > start)
        .ok_or(Error::Playlist("invalid byte range"))?;
    *previous = Some((url.clone(), end));
    Ok(Some((start, end - 1)))
}

fn aes_key(key: &m3u8_rs::Key, base: &Url, sequence: u64, init: bool) -> Result<AesKey, Error> {
    let iv = if let Some(iv) = &key.iv {
        let value = iv.strip_prefix("0x").unwrap_or(iv);
        if value.len() > 32 || value.is_empty() {
            return Err(Error::InvalidKey);
        }
        let bytes = hex::decode(format!("{value:0>32}")).map_err(|_| Error::InvalidKey)?;
        bytes.try_into().map_err(|_| Error::InvalidKey)?
    } else {
        if init {
            return Err(Error::Playlist(
                "encrypted init section requires explicit IV",
            ));
        }
        (sequence as u128).to_be_bytes()
    };
    Ok(AesKey {
        url: join(
            base,
            key.uri
                .as_deref()
                .ok_or(Error::Playlist("AES key URI missing"))?,
        )?,
        iv,
    })
}

fn build_plan(base: &Url, playlist: m3u8_rs::MediaPlaylist) -> Result<Plan, Error> {
    if !playlist.end_list {
        return Err(Error::Playlist("live streams are not supported"));
    }
    if playlist.segments.is_empty() {
        return Err(Error::Playlist("empty playlist"));
    }
    let mut protection = Protection::Clear;
    let mut pieces = Vec::new();
    let mut key = None;
    let mut map = None;
    let mut range = None;
    let mut map_range = None;
    for (index, segment) in playlist.segments.iter().enumerate() {
        if segment.discontinuity {
            return Err(Error::Playlist("discontinuous streams are not supported"));
        }
        if let Some(next) = &segment.key {
            key = Some(next.clone());
        }
        let mut aes = false;
        if let Some(key) = &key {
            match &key.method {
                KeyMethod::None => (),
                KeyMethod::AES128
                    if key
                        .keyformat
                        .as_deref()
                        .is_none_or(|format| format == "identity") =>
                {
                    if matches!(protection, Protection::Cenc { .. }) {
                        return Err(Error::Playlist("mixed encryption methods"));
                    }
                    protection = Protection::Aes128;
                    aes = true;
                }
                KeyMethod::Other(method)
                    if matches!(method.as_str(), "SAMPLE-AES-CTR" | "ISO-23001-7") =>
                {
                    let pssh = key
                        .uri
                        .clone()
                        .ok_or(Error::Playlist("missing CENC PSSH"))?;
                    match &protection {
                        Protection::Clear => protection = Protection::Cenc { pssh },
                        Protection::Cenc { pssh: old } if old == &pssh => (),
                        _ => return Err(Error::Playlist("CENC key rotation is not supported")),
                    }
                }
                _ => {
                    return Err(Error::Playlist(
                        "unsupported sample encryption; AAC/CENC is required",
                    ));
                }
            }
        }
        if let Some(next) = &segment.map {
            if map.as_ref().is_some_and(|old| old != next) {
                return Err(Error::Playlist("changing init sections"));
            }
            if map.is_none() {
                let url = join(base, &next.uri)?;
                let range = byte_range(next.byte_range.as_ref(), &url, &mut map_range)?;
                let aes = if aes {
                    Some(aes_key(key.as_ref().expect("AES key"), base, 0, true)?)
                } else {
                    None
                };
                pieces.push(Piece { url, range, aes });
                map = Some(next.clone());
            }
        }
        let url = join(base, &segment.uri)?;
        let range = byte_range(segment.byte_range.as_ref(), &url, &mut range)?;
        let sequence = playlist
            .media_sequence
            .checked_add(index as u64)
            .ok_or(Error::Playlist("sequence overflow"))?;
        let aes = if aes {
            Some(aes_key(
                key.as_ref().expect("AES key"),
                base,
                sequence,
                false,
            )?)
        } else {
            None
        };
        pieces.push(Piece { url, range, aes });
    }
    Ok(Plan { protection, pieces })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> Result<Plan, Error> {
        let Playlist::MediaPlaylist(list) =
            m3u8_rs::parse_playlist_res(normalize_keys(text).as_bytes()).unwrap()
        else {
            panic!()
        };
        build_plan(
            &Url::parse("https://example.test/a/list.m3u8").unwrap(),
            list,
        )
    }
    #[test]
    fn init_ranges_and_key_inheritance() {
        let plan=parse("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-KEY:METHOD=AES-128,URI=\"key\",IV=0x01\n#EXT-X-MAP:URI=\"init\"\n#EXTINF:1,\n#EXT-X-BYTERANGE:16@0\nfile\n#EXTINF:1,\n#EXT-X-BYTERANGE:16\nfile\n#EXT-X-ENDLIST\n").unwrap();
        assert_eq!(plan.pieces.len(), 3);
        assert_eq!(plan.pieces[2].range, Some((16, 31)));
        assert!(plan.pieces.iter().all(|piece| piece.aes.is_some()));
    }
    #[test]
    fn accepts_apple_legacy_cenc_method() {
        let pssh = "data:text/plain;base64,AgICAgICAgICAgICAgICAg==";
        let plan = parse(&format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-KEY:METHOD=ISO-23001-7,URI=\"{pssh}\"\n#EXT-X-MAP:URI=\"init\"\n#EXTINF:1,\nsegment\n#EXT-X-ENDLIST\n"
        ))
        .unwrap();
        assert!(matches!(plan.protection, Protection::Cenc { pssh: actual } if actual == pssh));
        assert_eq!(plan.pieces.len(), 2);
        assert!(plan.pieces.iter().all(|piece| piece.aes.is_none()));
    }
    #[test]
    fn rejects_live_and_invalid_implicit_ranges() {
        assert!(parse("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\na\n").is_err());
        assert!(parse("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\n#EXT-X-BYTERANGE:4\na\n#EXT-X-ENDLIST\n").is_err());
    }
}
