use crate::Config;
use media::{CancellationToken, Extras, LyricsKind};

pub async fn fetch(
    config: &Config,
    session: &apple::Session,
    client: &reqwest::Client,
    track: &apple::Track,
    cancel: &CancellationToken,
) -> Result<(Extras, Vec<String>), media::Error> {
    let mut extras = Extras {
        save_cover: config.save_cover,
        save_lyrics: config.save_lyrics,
        lyrics_format: config.lyrics_format,
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let mut failed = false;
    for level in [apple::LyricsLevel::Word, apple::LyricsLevel::Normal] {
        let result = tokio::select! {biased;_=cancel.cancelled()=>return Err(media::Error::Cancelled),result=apple::lyric_ttml(session,track,&config.language,level)=>result};
        match result {
            Ok(Some(ttml)) => match media::parse_lyrics(&ttml) {
                Ok(lyrics) => {
                    if extras
                        .lyrics
                        .as_ref()
                        .is_none_or(|old| lyrics.kind > old.kind)
                    {
                        extras.lyrics = Some(lyrics);
                    }
                    if extras
                        .lyrics
                        .as_ref()
                        .is_some_and(|lyrics| lyrics.kind == LyricsKind::Word)
                    {
                        break;
                    }
                }
                Err(_) => failed = true,
            },
            Ok(None) => (),
            Err(_) => failed = true,
        }
    }
    if extras.lyrics.is_none() && failed {
        warnings.push("歌词不可用或读取失败，继续保存音频。".into());
    }
    if let Some(template) = &track.artwork_url {
        match media::fetch_cover(client, template, cancel).await {
            Ok(cover) => extras.cover = Some(cover),
            Err(media::Error::Cancelled) => return Err(media::Error::Cancelled),
            Err(_) => warnings.push("封面读取失败，继续保存音频。".into()),
        }
    }
    Ok((extras, warnings))
}
