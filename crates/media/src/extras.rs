use crate::{CancellationToken, Error, Lyrics, LyricsFormat};
use std::path::Path;
use url::Url;

#[derive(Clone)]
pub struct Cover {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}
impl Cover {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        let extension = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "jpg"
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "png"
        } else {
            return Err(Error::Artwork);
        };
        Ok(Self { bytes, extension })
    }
}
#[derive(Clone, Default)]
pub struct Extras {
    pub lyrics: Option<Lyrics>,
    pub cover: Option<Cover>,
    pub save_lyrics: bool,
    pub save_cover: bool,
    pub lyrics_format: LyricsFormat,
}

pub async fn fetch_cover(
    client: &reqwest::Client,
    template: &str,
    cancel: &CancellationToken,
) -> Result<Cover, Error> {
    let template = template
        .replace("{w}", "1200")
        .replace("{h}", "1200")
        .replace("{f}", "jpg");
    let url = Url::parse(&template).map_err(|_| Error::Artwork)?;
    crate::hls::join(&url, "")?;
    let (bytes, _) = crate::transfer::fetch(client, &url, None, 20 * 1024 * 1024, cancel).await?;
    Cover::from_bytes(bytes)
}

pub(crate) fn embed(path: &Path, extras: &Extras, tags: &crate::Tags) -> Result<(), Error> {
    if extras.lyrics.is_none()
        && extras.cover.is_none()
        && tags.apple_music_id.is_none()
        && tags.apple_music_url.is_none()
    {
        return Ok(());
    }
    let mut tag = mp4ameta::Tag::read_from_path(path).map_err(|_| Error::Tag)?;
    for (name, value) in [
        ("APPLE_MUSIC_ID", &tags.apple_music_id),
        ("APPLE_MUSIC_URL", &tags.apple_music_url),
    ] {
        if let Some(value) = value {
            tag.set_data(
                mp4ameta::FreeformIdent::new_static("com.gamdl-rs", name),
                mp4ameta::Data::Utf8(value.clone()),
            );
        }
    }
    if let Some(lyrics) = &extras.lyrics {
        tag.set_lyrics(lyrics.embedded());
        // Preserve Apple's full timing, singer roles and metadata without lossy conversion.
        tag.set_data(
            mp4ameta::FreeformIdent::new_static("com.gamdl-rs", "LYRICS_TTML"),
            mp4ameta::Data::Utf8(lyrics.ttml.clone()),
        );
    }
    if let Some(cover) = &extras.cover {
        tag.set_artwork(if cover.extension == "png" {
            mp4ameta::Img::png(cover.bytes.clone())
        } else {
            mp4ameta::Img::jpeg(cover.bytes.clone())
        });
    }
    tag.write_to_path(path).map_err(|_| Error::Tag)
}

pub(crate) async fn sidecars(
    extras: &Extras,
    temporary: &Path,
    output: &Path,
) -> Result<(), Error> {
    let mut files = Vec::new();
    if extras.save_cover
        && let Some(cover) = &extras.cover
    {
        files.push((cover.extension, cover.bytes.as_slice()));
    }
    if extras.save_lyrics
        && let Some(lyrics) = &extras.lyrics
    {
        let (extension, text) = lyrics.sidecar(extras.lyrics_format);
        files.push((extension, text.as_bytes()));
    }
    for (extension, bytes) in files {
        let staged = temporary.join(format!("sidecar.{extension}"));
        tokio::fs::write(&staged, bytes).await?;
        match tokio::fs::hard_link(&staged, output.with_extension(extension)).await {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
