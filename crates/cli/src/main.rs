use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "A lightweight native Apple Music tool")]
struct Args {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    cookies: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a supported URL offline, without cookies or network access.
    Parse { url: String },
    /// Validate the cookie session with Apple Music.
    Auth,
    /// Fetch catalog metadata using the cookie session.
    Inspect { url: String },
    /// Check the local FFmpeg executable.
    Doctor,
    /// Download songs, albums or playlists. Progress events go to stderr as JSON.
    Download {
        #[arg(required = true)]
        urls: Vec<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(1..=16))]
        jobs: Option<u8>,
        #[arg(long)]
        wvd: Option<PathBuf>,
        /// Also save artwork beside the M4A file.
        #[arg(long)]
        save_cover: bool,
        /// Also save lyrics beside the M4A file.
        #[arg(long)]
        save_lyrics: bool,
        #[arg(long, value_parser = ["lrc", "ttml"])]
        lyrics_format: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Command::Parse { url } = &args.command {
        println!("{}", serde_json::to_string_pretty(&app::parse_url(url)?)?);
        return Ok(());
    }
    let mut config = app::load_config(args.config.as_deref()).await?;
    if let Some(path) = args.cookies {
        config.cookies_path = path;
    }
    if let Command::Download {
        urls,
        output,
        jobs,
        wvd,
        save_cover,
        save_lyrics,
        lyrics_format,
    } = &args.command
    {
        if let Some(output) = output {
            config.output_path = output.clone();
        }
        if let Some(jobs) = jobs {
            config.jobs = *jobs as usize;
        }
        if let Some(wvd) = wvd {
            config.wvd_path = Some(wvd.clone());
        }
        config.save_cover |= *save_cover;
        config.save_lyrics |= *save_lyrics;
        if let Some(format) = lyrics_format {
            config.lyrics_format = if format == "ttml" {
                app::LyricsFormat::Ttml
            } else {
                app::LyricsFormat::Lrc
            };
        }
        let cancel = app::CancellationToken::new();
        let signal_cancel = cancel.clone();
        let signal = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                signal_cancel.cancel();
            }
        });
        let (events, mut receiver) = tokio::sync::mpsc::channel(128);
        let reporter = tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                if let Ok(json) = serde_json::to_string(&event) {
                    eprintln!("{json}");
                }
            }
        });
        let result = app::download(config, urls.clone(), events, cancel).await;
        signal.abort();
        let _ = reporter.await;
        let summary = result?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        anyhow::ensure!(
            summary.failed == 0 && summary.cancelled == 0,
            "queue finished with {} failed and {} cancelled tracks",
            summary.failed,
            summary.cancelled
        );
        return Ok(());
    }
    let output = match args.command {
        Command::Parse { .. } | Command::Download { .. } => unreachable!(),
        Command::Auth => serde_json::to_string_pretty(&app::check_auth(&config).await?)?,
        Command::Inspect { url } => {
            serde_json::to_string_pretty(&app::inspect(&config, &url).await?)?
        }
        Command::Doctor => serde_json::to_string_pretty(&app::check_ffmpeg(&config).await?)?,
    };
    println!("{output}");
    Ok(())
}
