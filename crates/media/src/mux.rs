use crate::{CancellationToken, ContentKey, Error, Plan, Progress, Protection};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

#[derive(Default, Clone)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub date: Option<String>,
    pub apple_music_id: Option<String>,
    pub apple_music_url: Option<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    Saved(PathBuf),
    Skipped(PathBuf),
}

async fn run(mut command: Command, cancel: &CancellationToken) -> Result<(), Error> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let status = tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),result=tokio::time::timeout(Duration::from_secs(300),command.status())=>result.map_err(|_|Error::Timeout)??};
    if status.success() {
        Ok(())
    } else {
        Err(Error::Failed)
    }
}

pub struct SaveOptions<'a> {
    pub output: &'a Path,
    pub ffmpeg: &'a Path,
    pub tags: &'a Tags,
    pub key: Option<&'a ContentKey>,
    pub extras: Option<&'a crate::Extras>,
}

pub async fn save(
    client: &reqwest::Client,
    plan: &Plan,
    options: SaveOptions<'_>,
    cancel: &CancellationToken,
    progress: impl Fn(Progress),
) -> Result<Output, Error> {
    let SaveOptions {
        output,
        ffmpeg,
        tags,
        key,
        extras,
    } = options;
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    match tokio::fs::metadata(output).await {
        Ok(_) => return Ok(Output::Skipped(output.to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    if matches!(plan.protection, Protection::Cenc { .. }) && key.is_none() {
        return Err(Error::CdmRequired);
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(parent).await?;
    let temp = tempfile::Builder::new()
        .prefix(".download-")
        .tempdir_in(parent)?;
    let raw = temp.path().join("source.mp4");
    let ready = temp.path().join("output.m4a");
    crate::download(client, plan, &raw, cancel, &progress).await?;
    progress(Progress::Muxing);
    let mut command = Command::new(ffmpeg);
    command.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
    if let Some(key) = key {
        command.arg("-decryption_key").arg(hex::encode(key.0));
    }
    command.arg("-i").arg(&raw).args([
        "-map",
        "0:a:0",
        "-vn",
        "-c:a",
        "copy",
        "-map_metadata",
        "-1",
        "-movflags",
        "+faststart",
    ]);
    for (name, value) in [
        ("title", Some(tags.title.clone())),
        ("artist", Some(tags.artist.clone())),
        ("album", Some(tags.album.clone())),
        ("track", tags.track.map(|v| v.to_string())),
        ("disc", tags.disc.map(|v| v.to_string())),
        ("date", tags.date.clone()),
    ] {
        if let Some(value) = value {
            command.arg("-metadata").arg(format!("{name}={value}"));
        }
    }
    command.arg(&ready);
    run(command, cancel).await?;
    {
        let extras = extras.cloned().unwrap_or_default();
        let tags = tags.clone();
        let ready = ready.clone();
        tokio::task::spawn_blocking(move || crate::extras::embed(&ready, &extras, &tags))
            .await
            .map_err(|_| Error::Tag)??;
    }
    progress(Progress::Verifying);
    let mut verify = Command::new(ffmpeg);
    verify
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-xerror",
            "-err_detect",
            "explode",
        ])
        .arg("-i")
        .arg(&ready)
        .args(["-map", "0:a:0", "-f", "null", "-"]);
    run(verify, cancel).await?;
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    // Sidecars are valid independent artifacts and never replace existing files.
    // Publish them before audio so a failed sidecar write cannot report a complete bundle.
    if let Some(extras) = extras {
        crate::extras::sidecars(extras, temp.path(), output).await?;
    }
    let result = tokio::fs::hard_link(&ready, output).await;
    match result {
        Ok(()) => Ok(Output::Saved(output.to_owned())),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(Output::Skipped(output.to_owned()))
        }
        Err(error) => Err(error.into()),
    }
}
