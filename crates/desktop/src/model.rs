use app::{Event, QueueItem, State};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum DownloadStep {
    #[default]
    Input,
    Selection,
    Progress,
}

#[derive(Default)]
pub struct Selection {
    pub tracks: Vec<app::Track>,
    pub selected: std::collections::HashSet<String>,
}
impl Selection {
    pub fn replace(&mut self, tracks: Vec<app::Track>) {
        self.tracks = tracks;
        self.selected.clear();
    }
    pub fn toggle(&mut self, index: usize, checked: bool) {
        if let Some(track) = self.tracks.get(index) {
            if checked {
                self.selected.insert(track.id.clone());
            } else {
                self.selected.remove(&track.id);
            }
        }
    }
    pub fn select_all(&mut self, checked: bool) {
        self.selected = if checked {
            self.tracks.iter().map(|track| track.id.clone()).collect()
        } else {
            Default::default()
        };
    }
    pub fn urls(&self) -> Vec<String> {
        self.tracks
            .iter()
            .filter(|track| self.selected.contains(&track.id))
            .map(|track| {
                format!(
                    "https://music.apple.com/{}/song/{}",
                    track.storefront, track.id
                )
            })
            .collect()
    }
    pub fn filtered(&self, query: &str) -> Vec<usize> {
        let query = query.to_lowercase();
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                query.is_empty()
                    || track.title.to_lowercase().contains(&query)
                    || track.artist.to_lowercase().contains(&query)
                    || track.album.to_lowercase().contains(&query)
                    || track.id.contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Downloads,
    Settings,
    Activity,
}
impl Page {
    pub fn title(self) -> String {
        match self {
            Self::Downloads => tr!("page.downloads"),
            Self::Settings => tr!("page.settings"),
            Self::Activity => tr!("page.activity"),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Active,
    Done,
    Failed,
    Cancelled,
}
impl Filter {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Active,
        Self::Done,
        Self::Failed,
        Self::Cancelled,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Active => "active",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::All => tr!("filter.all"),
            Self::Active => tr!("filter.active"),
            Self::Done => tr!("state.complete"),
            Self::Failed => tr!("state.failed"),
            Self::Cancelled => tr!("state.cancelled"),
        }
    }
    pub fn matches(self, state: &State) -> bool {
        match self {
            Self::All => true,
            Self::Active => !terminal(state),
            Self::Done => matches!(state, State::Complete { .. } | State::Skipped { .. }),
            Self::Failed => matches!(state, State::Failed { .. }),
            Self::Cancelled => matches!(state, State::Cancelled),
        }
    }
}

pub struct Assets {
    pub lyrics: Option<String>,
    pub cover: bool,
}
impl Assets {
    pub fn label(&self) -> String {
        let lyrics = match self.lyrics.as_deref() {
            Some("逐字") => tr!("assets.word_lyrics"),
            Some("逐行") => tr!("assets.line_lyrics"),
            Some("纯文本") => tr!("assets.plain_lyrics"),
            Some(kind) => tr!("assets.lyrics", kind = kind),
            None => tr!("assets.no_lyrics"),
        };
        format!(
            "{} · {}",
            lyrics,
            if self.cover {
                tr!("assets.cover")
            } else {
                tr!("assets.no_cover")
            }
        )
    }
}

pub struct Row {
    pub item: QueueItem,
    pub state: State,
    pub completed: usize,
    pub total: usize,
    pub bytes: u64,
    pub assets: Option<Assets>,
}
impl Row {
    pub fn percent(&self) -> f32 {
        if matches!(self.state, State::Complete { .. } | State::Skipped { .. }) {
            100.
        } else if self.total > 0 {
            (self.completed as f32 / self.total as f32 * 100.).clamp(0., 100.)
        } else {
            0.
        }
    }
    pub fn path(&self) -> Option<&Path> {
        match &self.state {
            State::Complete { path } | State::Skipped { path } => Some(path),
            _ => None,
        }
    }
    pub fn error(&self) -> Option<&str> {
        match &self.state {
            State::Failed { message } => Some(message),
            _ => None,
        }
    }
}

pub fn terminal(state: &State) -> bool {
    matches!(
        state,
        State::Complete { .. } | State::Skipped { .. } | State::Failed { .. } | State::Cancelled
    )
}
pub fn state_label(state: &State) -> String {
    match state {
        State::Pending => tr!("state.pending"),
        State::Resolving => tr!("state.resolving"),
        State::Downloading => tr!("state.downloading"),
        State::Enriching => tr!("state.enriching"),
        State::Decrypting => tr!("state.decrypting"),
        State::Muxing => tr!("state.muxing"),
        State::Verifying => tr!("state.verifying"),
        State::Complete { .. } => tr!("state.complete"),
        State::Skipped { .. } => tr!("state.skipped"),
        State::Failed { .. } => tr!("state.failed"),
        State::Cancelled => tr!("state.cancelled"),
    }
}

#[derive(Default)]
pub struct Queue {
    pub rows: Vec<Row>,
    pub finished: Option<app::Summary>,
}
impl Queue {
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Queued { items } => {
                self.rows = items
                    .iter()
                    .cloned()
                    .map(|item| Row {
                        item,
                        state: State::Pending,
                        completed: 0,
                        total: 0,
                        bytes: 0,
                        assets: None,
                    })
                    .collect();
                self.finished = None;
            }
            Event::State { id, state } => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.item.id == *id) {
                    row.state = state.clone();
                }
            }
            Event::Progress {
                id,
                completed,
                total,
                bytes,
            } => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.item.id == *id) {
                    row.completed = *completed;
                    row.total = *total;
                    row.bytes = *bytes;
                }
            }
            Event::Finished { summary } => self.finished = Some(summary.clone()),
            Event::Assets { id, lyrics, cover } => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.item.id == *id) {
                    row.assets = Some(Assets {
                        lyrics: lyrics.clone(),
                        cover: *cover,
                    });
                }
            }
            Event::Warning { .. } | Event::Planning => (),
        }
    }
    pub fn filtered(&self, filter: Filter, query: &str) -> Vec<usize> {
        let query = query.to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                filter.matches(&row.state)
                    && (query.is_empty()
                        || row.item.title.to_lowercase().contains(&query)
                        || row.item.artist.to_lowercase().contains(&query)
                        || row.item.track_id.contains(&query))
            })
            .map(|(index, _)| index)
            .collect()
    }
    pub fn count(&self, filter: Filter) -> usize {
        self.rows
            .iter()
            .filter(|row| filter.matches(&row.state))
            .count()
    }
    pub fn retry_urls(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| matches!(row.state, State::Failed { .. } | State::Cancelled))
            .map(|row| row.item.url.clone())
            .collect()
    }
    pub fn percent(&self) -> f32 {
        if self.rows.is_empty() {
            0.
        } else {
            self.rows
                .iter()
                .map(|row| {
                    if terminal(&row.state) {
                        100.
                    } else {
                        row.percent() * 0.9
                    }
                })
                .sum::<f32>()
                / self.rows.len() as f32
        }
    }
    pub fn stopped(&mut self) {
        for row in &mut self.rows {
            if !terminal(&row.state) {
                row.state = State::Cancelled;
            }
        }
    }
}

pub fn urls(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    text.split_whitespace()
        .filter(|url| seen.insert(*url))
        .map(str::to_owned)
        .collect()
}

/// Keep transient feedback compact; the activity log retains the full message.
pub fn notice_preview(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 140 {
        compact
    } else {
        format!("{}…", compact.chars().take(140).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection() -> Selection {
        let mut selection = Selection::default();
        selection.replace(
            (1..=3)
                .map(|index| app::Track {
                    id: index.to_string(),
                    title: format!("Song {index}"),
                    artist: "Artist".into(),
                    album: "Album".into(),
                    track_number: None,
                    disc_number: None,
                    release_date: None,
                    storefront: "us".into(),
                    artwork_url: None,
                })
                .collect(),
        );
        selection
    }
    #[test]
    fn only_selected_tracks_are_submitted_in_catalog_order() {
        let mut selection = selection();
        assert!(selection.urls().is_empty());
        selection.toggle(2, true);
        selection.toggle(0, true);
        selection.toggle(2, true);
        assert_eq!(
            selection.urls(),
            vec![
                "https://music.apple.com/us/song/1",
                "https://music.apple.com/us/song/3"
            ]
        );
        selection.toggle(0, false);
        assert_eq!(selection.urls(), vec!["https://music.apple.com/us/song/3"]);
    }
    #[test]
    fn search_does_not_change_selection_and_reparse_resets_it() {
        let mut selection = selection();
        selection.select_all(true);
        assert_eq!(selection.filtered("song 2"), vec![1]);
        assert_eq!(selection.urls().len(), 3);
        selection.select_all(false);
        assert!(selection.urls().is_empty());
        selection.select_all(true);
        selection.replace(Vec::new());
        assert!(selection.selected.is_empty());
        assert!(selection.urls().is_empty());
    }
    #[test]
    fn terminal_rows_cannot_be_overwritten_by_late_progress() {
        let mut queue = Queue::default();
        queue.apply(&Event::Queued {
            items: vec![QueueItem {
                id: 4,
                track_id: "12".into(),
                url: "https://music.apple.com/cn/song/12".into(),
                title: "Song".into(),
                artist: "Artist".into(),
            }],
        });
        queue.apply(&Event::State {
            id: 4,
            state: State::Complete {
                path: "song.m4a".into(),
            },
        });
        queue.apply(&Event::Progress {
            id: 4,
            completed: 1,
            total: 4,
            bytes: 64,
        });
        assert_eq!(queue.rows[0].percent(), 100.);
        assert_eq!(queue.count(Filter::Done), 1);
        assert!(queue.filtered(Filter::Active, "").is_empty());
    }
    #[test]
    fn cancellation_and_retry_preserve_completed_files() {
        let mut queue = Queue::default();
        queue.apply(&Event::Queued {
            items: (0..3)
                .map(|id| QueueItem {
                    id,
                    track_id: (id + 10).to_string(),
                    url: format!("https://music.apple.com/cn/song/{}", id + 10),
                    title: "Title".into(),
                    artist: "Artist".into(),
                })
                .collect(),
        });
        queue.apply(&Event::State {
            id: 0,
            state: State::Complete {
                path: "file".into(),
            },
        });
        queue.apply(&Event::State {
            id: 1,
            state: State::Failed {
                message: "offline".into(),
            },
        });
        queue.stopped();
        assert_eq!(
            queue.retry_urls(),
            vec![
                "https://music.apple.com/cn/song/11",
                "https://music.apple.com/cn/song/12"
            ]
        );
        assert_eq!(queue.count(Filter::Done), 1);
        assert_eq!(queue.filtered(Filter::All, "artist").len(), 3);
    }
    #[test]
    fn multiline_urls_are_deduplicated_in_order() {
        assert_eq!(urls("a\nb\r\na  b"), vec!["a", "b"]);
    }
}
