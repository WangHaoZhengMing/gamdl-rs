use crate::{Config, Error};
use apple::{Session, Track};
use media::{CancellationToken, Protection};
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
};
use tokio::{sync::mpsc, task::JoinSet};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum State {
    Pending,
    Resolving,
    Downloading,
    Enriching,
    Decrypting,
    Muxing,
    Verifying,
    Complete { path: PathBuf },
    Skipped { path: PathBuf },
    Failed { message: String },
    Cancelled,
}
#[derive(Debug, Clone, Serialize)]
pub struct QueueItem {
    pub id: usize,
    pub track_id: String,
    pub url: String,
    pub title: String,
    pub artist: String,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub total: usize,
    pub saved: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: usize,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Assets {
        id: usize,
        lyrics: Option<String>,
        cover: bool,
    },
    Warning {
        id: usize,
        message: String,
    },
    Planning,
    Queued {
        items: Vec<QueueItem>,
    },
    State {
        id: usize,
        state: State,
    },
    Progress {
        id: usize,
        completed: usize,
        total: usize,
        bytes: u64,
    },
    Finished {
        summary: Summary,
    },
}

async fn emit(events: &mpsc::Sender<Event>, event: Event) {
    let _ = events.send(event).await;
}

pub async fn download(
    config: Config,
    urls: Vec<String>,
    events: mpsc::Sender<Event>,
    cancel: CancellationToken,
) -> Result<Summary, Error> {
    download_with_credentials(config, urls, events, cancel, crate::Credentials::CookieFile).await
}

pub async fn download_with_credentials(
    config: Config,
    urls: Vec<String>,
    events: mpsc::Sender<Event>,
    cancel: CancellationToken,
    credentials: crate::Credentials,
) -> Result<Summary, Error> {
    if !(1..=16).contains(&config.jobs) {
        return Err(Error::InvalidJobs);
    }
    // Validate all URLs before opening credentials or doing network work.
    let resources = urls
        .iter()
        .map(|url| crate::parse_url(url))
        .collect::<Result<Vec<_>, _>>()?;
    if resources.is_empty() {
        return Err(Error::EmptyQueue);
    }
    emit(&events, Event::Planning).await;
    let planning = async {
        crate::check_ffmpeg(&config).await?;
        let (session, account) = crate::session(&config, &credentials).await?;
        if !account.active_subscription {
            return Err(Error::SubscriptionRequired);
        }
        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        let mut sources = std::collections::HashMap::new();
        for resource in resources {
            for track in apple::tracks(&session, &resource, &config.language).await? {
                if seen.insert(track.id.clone()) {
                    sources.insert(
                        track.id.clone(),
                        format!(
                            "https://music.apple.com/{}/song/{}",
                            resource.storefront, track.id
                        ),
                    );
                    tracks.push(track);
                }
            }
        }
        Ok::<_, Error>((session, tracks, sources))
    };
    let (session, tracks, sources) = tokio::select! {biased; _=cancel.cancelled()=>return Err(media::Error::Cancelled.into()),result=planning=>result?};
    if tracks.is_empty() {
        return Err(Error::EmptyQueue);
    }
    let items = tracks
        .iter()
        .enumerate()
        .map(|(id, track)| QueueItem {
            id,
            track_id: track.id.clone(),
            url: sources[&track.id].clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
        })
        .collect();
    emit(&events, Event::Queued { items }).await;
    let client = media::client()?;
    let worker_events = events.clone();
    let worker_cancel = cancel.clone();
    let jobs = config.jobs;
    schedule(tracks, jobs, &events, &cancel, move |id, track| {
        let config = config.clone();
        let session = session.clone();
        let client = client.clone();
        let events = worker_events.clone();
        let cancel = worker_cancel.clone();
        async move {
            match download_one(&config, &session, &client, &track, id, &events, &cancel).await {
                Ok(media::Output::Saved(path)) => State::Complete { path },
                Ok(media::Output::Skipped(path)) => State::Skipped { path },
                Err(Error::Media(media::Error::Cancelled)) => State::Cancelled,
                Err(error) => State::Failed {
                    message: error.to_string(),
                },
            }
        }
    })
    .await
}

async fn schedule<T, F, Fut>(
    items: Vec<T>,
    jobs: usize,
    events: &mpsc::Sender<Event>,
    cancel: &CancellationToken,
    worker: F,
) -> Result<Summary, Error>
where
    T: Send + 'static,
    F: Fn(usize, T) -> Fut,
    Fut: std::future::Future<Output = State> + Send + 'static,
{
    let mut pending: VecDeque<_> = items.into_iter().enumerate().collect();
    let mut summary = Summary {
        total: pending.len(),
        ..Default::default()
    };
    let mut workers = JoinSet::new();
    let mut task_ids = std::collections::HashMap::new();
    loop {
        while workers.len() < jobs && !cancel.is_cancelled() {
            let Some((id, item)) = pending.pop_front() else {
                break;
            };
            let future = worker(id, item);
            let task = workers.spawn(async move { (id, future.await) });
            task_ids.insert(task.id(), id);
        }
        if cancel.is_cancelled() {
            for (id, _) in pending.drain(..) {
                summary.cancelled += 1;
                emit(
                    events,
                    Event::State {
                        id,
                        state: State::Cancelled,
                    },
                )
                .await;
            }
        }
        let Some(result) = workers.join_next_with_id().await else {
            break;
        };
        let (id, state) = match result {
            Ok((task, result)) => {
                task_ids.remove(&task);
                result
            }
            Err(error) => {
                let id = task_ids.remove(&error.id()).ok_or(Error::Worker)?;
                (
                    id,
                    State::Failed {
                        message: "download worker stopped unexpectedly".into(),
                    },
                )
            }
        };
        match &state {
            State::Complete { .. } => summary.saved += 1,
            State::Skipped { .. } => summary.skipped += 1,
            State::Cancelled => summary.cancelled += 1,
            State::Failed { .. } => summary.failed += 1,
            _ => return Err(Error::Worker),
        }
        emit(events, Event::State { id, state }).await;
    }
    emit(
        events,
        Event::Finished {
            summary: summary.clone(),
        },
    )
    .await;
    Ok(summary)
}

fn component(input: &str) -> String {
    let value: String = input
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .scan(0usize, |bytes, character| {
            *bytes += character.len_utf8();
            (*bytes <= 160).then_some(character)
        })
        .collect();
    let value = value.trim().trim_matches('.').trim_end();
    if value.is_empty() {
        return "Unknown".into();
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        format!("_{value}")
    } else {
        value.to_owned()
    }
}
fn output_path(root: &Path, track: &Track) -> PathBuf {
    root.join(component(&track.artist))
        .join(component(&track.album))
        .join(format!(
            "{:02}-{:02} {} [{}].m4a",
            track.disc_number.unwrap_or(1),
            track.track_number.unwrap_or(0),
            component(&track.title),
            track.id
        ))
}

async fn download_one(
    config: &Config,
    session: &Session,
    client: &reqwest::Client,
    track: &Track,
    id: usize,
    events: &mpsc::Sender<Event>,
    cancel: &CancellationToken,
) -> Result<media::Output, Error> {
    let output = output_path(&config.output_path, track);
    if tokio::fs::try_exists(&output).await? {
        return Ok(media::Output::Skipped(output));
    }
    emit(
        events,
        Event::State {
            id,
            state: State::Resolving,
        },
    )
    .await;
    let preparation = async {
        let playback = apple::playback(session, &track.id, &config.language).await?;
        let plan = media::prepare(client, playback.url, cancel).await?;
        let key = match &plan.protection {
            Protection::Cenc { pssh } => {
                emit(
                    events,
                    Event::State {
                        id,
                        state: State::Decrypting,
                    },
                )
                .await;
                let wvd = config.wvd_path.clone();
                let pssh = pssh.clone();
                let request = tokio::task::spawn_blocking(move || {
                    media::license_request(wvd.as_deref(), &pssh)
                })
                .await
                .map_err(|_| Error::Worker)??;
                let challenge = request.challenge()?;
                let license =
                    apple::exchange_license(session, &track.id, pssh_uri(&plan)?, &challenge)
                        .await?;
                Some(
                    tokio::task::spawn_blocking(move || request.finish(&license))
                        .await
                        .map_err(|_| Error::Worker)??,
                )
            }
            _ => None,
        };
        Ok::<_, Error>((plan, key))
    };
    let (plan, key) = tokio::select! {biased; _=cancel.cancelled()=>return Err(media::Error::Cancelled.into()),result=preparation=>result?};
    emit(
        events,
        Event::State {
            id,
            state: State::Enriching,
        },
    )
    .await;
    let (extras, warnings) =
        crate::enrichment::fetch(config, session, client, track, cancel).await?;
    for message in warnings {
        emit(events, Event::Warning { id, message }).await;
    }
    emit(
        events,
        Event::Assets {
            id,
            lyrics: extras
                .lyrics
                .as_ref()
                .map(|lyrics| lyrics.kind.label().into()),
            cover: extras.cover.is_some(),
        },
    )
    .await;
    emit(
        events,
        Event::State {
            id,
            state: State::Downloading,
        },
    )
    .await;
    let tags = media::Tags {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        track: track.track_number,
        disc: track.disc_number,
        date: track.release_date.clone(),
        apple_music_id: Some(track.id.clone()),
        apple_music_url: Some(format!(
            "https://music.apple.com/{}/song/{}",
            track.storefront, track.id
        )),
    };
    let options = media::SaveOptions {
        output: &output,
        ffmpeg: &config.ffmpeg_path,
        tags: &tags,
        key: key.as_ref(),
        extras: Some(&extras),
    };
    // High-frequency updates are lossy; terminal events are reliable.
    Ok(media::save(client, &plan, options, cancel, |progress| {
        let event = match progress {
            media::Progress::Download {
                completed,
                total,
                bytes,
            } => Event::Progress {
                id,
                completed,
                total,
                bytes,
            },
            media::Progress::Muxing => Event::State {
                id,
                state: State::Muxing,
            },
            media::Progress::Verifying => Event::State {
                id,
                state: State::Verifying,
            },
        };
        let _ = events.try_send(event);
    })
    .await?)
}
fn pssh_uri(plan: &media::Plan) -> Result<&str, Error> {
    match &plan.protection {
        Protection::Cenc { pssh } => Ok(pssh),
        _ => Err(media::Error::InvalidKey.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_cannot_escape_output_and_are_portable() {
        for (input, expected) in [
            ("../a/b", "_a_b"),
            ("CON", "_CON"),
            ("..", "Unknown"),
            ("x:y?", "x_y_"),
        ] {
            assert_eq!(component(input), expected);
        }
    }
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    #[tokio::test]
    async fn concurrency_is_bounded_and_one_failure_does_not_stop_the_queue() {
        let (events, mut receiver) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = active.clone();
        let count = active.clone();
        let maximum = Arc::new(AtomicUsize::new(0));
        let observed = maximum.clone();
        let summary = schedule((0..7).collect(), 2, &events, &cancel, move |_, item| {
            let active = count.clone();
            let maximum = observed.clone();
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                if item == 2 {
                    State::Failed {
                        message: "fixture".into(),
                    }
                } else {
                    State::Complete {
                        path: PathBuf::from(item.to_string()),
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(summary.saved, 6);
        assert_eq!(summary.failed, 1);
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
        assert_eq!(peak.load(Ordering::SeqCst), 0);
        let mut terminals = 0;
        while let Ok(event) = receiver.try_recv() {
            if matches!(event, Event::State { .. }) {
                terminals += 1;
            }
        }
        assert_eq!(terminals, 7);
    }
    #[tokio::test]
    async fn cancellation_covers_running_and_pending_tracks() {
        let (events, mut receiver) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let summary = schedule((0..5).collect(), 1, &events, &cancel, move |_, _| {
            let cancel = worker_cancel.clone();
            async move {
                cancel.cancel();
                State::Cancelled
            }
        })
        .await
        .unwrap();
        assert_eq!(summary.cancelled, 5);
        assert_eq!(summary.saved, 0);
        let mut cancelled = 0;
        while let Ok(event) = receiver.try_recv() {
            if matches!(
                event,
                Event::State {
                    state: State::Cancelled,
                    ..
                }
            ) {
                cancelled += 1;
            }
        }
        assert_eq!(cancelled, 5);
    }
}
