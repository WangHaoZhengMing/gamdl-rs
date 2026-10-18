use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsFormat {
    #[default]
    Lrc,
    Ttml,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LyricsKind {
    Plain,
    Line,
    Word,
}
impl LyricsKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "纯文本",
            Self::Line => "逐行",
            Self::Word => "逐字",
        }
    }
}
#[derive(Clone)]
pub struct Lyrics {
    pub kind: LyricsKind,
    pub plain: String,
    pub synced: Option<String>,
    pub ttml: String,
}
impl Lyrics {
    pub fn embedded(&self) -> &str {
        self.synced.as_deref().unwrap_or(&self.plain)
    }
    pub fn sidecar(&self, format: LyricsFormat) -> (&'static str, &str) {
        match format {
            LyricsFormat::Ttml => ("ttml", &self.ttml),
            LyricsFormat::Lrc => match &self.synced {
                Some(text) => ("lrc", text),
                None => ("txt", &self.plain),
            },
        }
    }
}
fn timestamp(text: &str) -> Option<u64> {
    let text = text.trim();
    let seconds = if let Some(value) = text.strip_suffix("ms") {
        value.parse::<f64>().ok()? / 1000.
    } else if let Some(value) = text.strip_suffix('s') {
        value.parse().ok()?
    } else if let Some(value) = text.strip_suffix('m') {
        value.parse::<f64>().ok()? * 60.
    } else if let Some(value) = text.strip_suffix('h') {
        value.parse::<f64>().ok()? * 3600.
    } else {
        let mut total = 0.;
        let parts = text.split(':').collect::<Vec<_>>();
        if parts.len() > 3 {
            return None;
        }
        for part in parts {
            total = total * 60. + part.parse::<f64>().ok()?;
        }
        total
    };
    (seconds.is_finite() && (0. ..86_400. * 365.).contains(&seconds))
        .then_some((seconds * 1000.).round() as u64)
}
fn stamp(ms: u64) -> String {
    format!(
        "{:02}:{:02}.{:02}",
        ms / 60_000,
        ms / 1000 % 60,
        ms % 1000 / 10
    )
}
fn text(node: roxmltree::Node<'_, '_>) -> String {
    let mut output = String::new();
    for descendant in node.descendants() {
        if descendant.is_text() {
            let value = descendant.text().unwrap_or_default();
            if value.trim().is_empty() && (value.contains('\n') || value.contains('\r')) {
                continue;
            }
            output.push_str(value);
        } else if descendant.is_element() && descendant.tag_name().name() == "br" {
            output.push('\n');
        }
    }
    output.replace('\r', "")
}

pub fn parse_lyrics(ttml: &str) -> Result<Lyrics, Error> {
    if ttml.len() > 2 * 1024 * 1024 {
        return Err(Error::Lyrics("document too large"));
    }
    let document = roxmltree::Document::parse(ttml).map_err(|_| Error::Lyrics("invalid TTML"))?;
    let mut plain = Vec::new();
    let mut synced = Vec::new();
    let mut kind = LyricsKind::Plain;
    for paragraph in document.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "p"
            && node.ancestors().any(|parent| {
                parent.has_tag_name(("http://www.w3.org/ns/ttml", "body"))
                    || parent.has_tag_name("body")
            })
    }) {
        let line = text(paragraph).trim().to_owned();
        if line.is_empty() {
            continue;
        }
        plain.push(line.clone());
        let words = paragraph
            .descendants()
            .filter(|node| {
                node.is_element()
                    && node.tag_name().name() == "span"
                    && node.attribute("begin").is_some()
                    && !node.descendants().skip(1).any(|child| {
                        child.is_element()
                            && child.tag_name().name() == "span"
                            && child.attribute("begin").is_some()
                    })
            })
            .filter_map(|node| {
                let begin = timestamp(node.attribute("begin")?)?;
                let value = text(node);
                (!value.is_empty()).then_some((node, begin, value))
            })
            .collect::<Vec<_>>();
        let begin = paragraph
            .attribute("begin")
            .and_then(timestamp)
            .or_else(|| words.first().map(|(_, begin, _)| *begin));
        if let Some(begin) = begin {
            if !words.is_empty() {
                kind = LyricsKind::Word;
                // Preserve text nodes between timed spans, including word separators.
                let mut body = String::new();
                for node in paragraph.descendants() {
                    if let Some((_, start, _)) =
                        words.iter().find(|(word, _, _)| word.id() == node.id())
                    {
                        body.push_str(&format!("<{}>", stamp(*start)));
                    }
                    if node.is_text() {
                        let value = node.text().unwrap_or_default();
                        if value.trim().is_empty() && (value.contains('\n') || value.contains('\r'))
                        {
                            continue;
                        }
                        body.push_str(value);
                    }
                    if node.is_element() && node.tag_name().name() == "br" {
                        body.push(' ');
                    }
                }
                if let Some(end) = words
                    .last()
                    .and_then(|(word, _, _)| word.attribute("end"))
                    .and_then(timestamp)
                {
                    body.push_str(&format!("<{}>", stamp(end)));
                }
                synced.push(format!("[{}]{}", stamp(begin), body.trim()));
            } else {
                kind = kind.max(LyricsKind::Line);
                synced.push(format!("[{}]{}", stamp(begin), line.replace('\n', " ")));
            }
        }
    }
    if plain.is_empty() {
        return Err(Error::Lyrics("empty lyrics"));
    }
    // If some lines have no timing, do not silently drop them from the embedded lyrics.
    let synced = if synced.len() == plain.len() {
        Some(synced.join("\n") + "\n")
    } else {
        kind = LyricsKind::Plain;
        None
    };
    Ok(Lyrics {
        kind,
        plain: plain.join("\n") + "\n",
        synced,
        ttml: ttml.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_timing_and_spaces_survive() {
        let lyrics=parse_lyrics(r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div><p begin="1.250s"><span begin="1.250s" end="1.500s">Hello</span> <span begin="1.600s" end="2s">world</span></p></div></body></tt>"#).unwrap();
        assert_eq!(lyrics.kind, LyricsKind::Word);
        assert_eq!(lyrics.plain, "Hello world\n");
        assert_eq!(
            lyrics.embedded(),
            "[00:01.25]<00:01.25>Hello <00:01.60>world<00:02.00>\n"
        );
    }
    #[test]
    fn line_and_plain_fallback_do_not_invent_word_times() {
        let line =
            parse_lyrics("<tt><body><p begin=\"00:03.200\">One &amp; two</p></body></tt>").unwrap();
        assert_eq!(line.kind, LyricsKind::Line);
        assert_eq!(line.embedded(), "[00:03.20]One & two\n");
        let plain = parse_lyrics("<tt><body><p>No timing</p></body></tt>").unwrap();
        assert_eq!(plain.kind, LyricsKind::Plain);
        assert_eq!(plain.sidecar(LyricsFormat::Lrc), ("txt", "No timing\n"));
    }
    #[test]
    fn partial_timing_preserves_all_lines() {
        let lyrics =
            parse_lyrics("<tt><body><p begin=\"1s\">Timed</p><p>Untimed</p></body></tt>").unwrap();
        assert_eq!(lyrics.kind, LyricsKind::Plain);
        assert!(lyrics.embedded().contains("Untimed"));
        assert!(parse_lyrics("<tt><body/></tt>").is_err());
    }
}
