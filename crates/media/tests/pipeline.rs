use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use media::{CancellationToken, ContentKey, Output, Progress, SaveOptions, Tags};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use url::Url;

struct Server {
    base: Url,
    task: tokio::task::JoinHandle<()>,
    requests: Arc<AtomicUsize>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn new(routes: HashMap<String, Vec<u8>>, retry: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let routes = Arc::new(routes);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let routes = routes.clone();
                let count = count.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut buf = [0; 1024];
                    loop {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let request = String::from_utf8_lossy(&bytes);
                    let path = request.split_whitespace().nth(1).unwrap();
                    let number = count.fetch_add(1, Ordering::SeqCst);
                    let (mut status, mut body) = if retry && number == 0 {
                        ("503 Service Unavailable", b"retry".as_slice())
                    } else if let Some(body) = routes.get(path) {
                        ("200 OK", body.as_slice())
                    } else {
                        ("404 Not Found", b"missing".as_slice())
                    };
                    let mut range_header = String::new();
                    if status == "200 OK"
                        && let Some(range) = request
                            .lines()
                            .find_map(|line| line.strip_prefix("range: bytes="))
                    {
                        let (start, end) = range.split_once('-').unwrap();
                        let start: usize = start.parse().unwrap();
                        let end: usize = end.parse().unwrap();
                        range_header =
                            format!("Content-Range: bytes {start}-{end}/{}\r\n", body.len());
                        body = &body[start..=end];
                        status = "206 Partial Content";
                    }
                    let header = format!(
                        "HTTP/1.1 {status}\r\n{range_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                });
            }
        });
        Self {
            base,
            task,
            requests,
        }
    }
}
async fn fixture(path: &Path, encrypted: bool) {
    let mut command = tokio::process::Command::new("ffmpeg");
    command.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=0.25",
        "-c:a",
        "aac",
        "-b:a",
        "64k",
    ]);
    if encrypted {
        command.args([
            "-encryption_scheme",
            "cenc-aes-ctr",
            "-encryption_key",
            "01010101010101010101010101010101",
            "-encryption_kid",
            "02020202020202020202020202020202",
        ]);
    }
    if path.extension().is_some_and(|ext| ext == "ts") {
        command.args(["-f", "mpegts"]);
    }
    assert!(command.arg(path).status().await.unwrap().success());
}
fn playlist(key: &str) -> Vec<u8> {
    format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n{key}#EXTINF:0.25,\nsegment\n#EXT-X-ENDLIST\n")
        .into_bytes()
}

#[tokio::test]
async fn aes_hls_retries_decrypts_tags_and_never_overwrites() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.ts");
    fixture(&source, false).await;
    let plain = tokio::fs::read(&source).await.unwrap();
    let key = [7; 16];
    let iv = [0; 16];
    let encrypted = cbc::Encryptor::<aes::Aes128>::new(&key.into(), &iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(&plain);
    let server = Server::new(
        HashMap::from([
            (
                "/list".into(),
                playlist("#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n"),
            ),
            ("/key".into(), key.to_vec()),
            ("/segment".into(), encrypted),
        ]),
        true,
    )
    .await;
    let client = media::client().unwrap();
    let cancel = CancellationToken::new();
    let plan = media::prepare(&client, server.base.join("list").unwrap(), &cancel)
        .await
        .unwrap();
    let output = temp.path().join("out/song.m4a");
    let tags = Tags {
        title: "Fixture title".into(),
        artist: "Fixture artist".into(),
        ..Default::default()
    };
    let options = || SaveOptions {
        output: &output,
        ffmpeg: Path::new("ffmpeg"),
        tags: &tags,
        key: None,
    };
    assert!(matches!(
        media::save(&client, &plan, options(), &cancel, |_| {})
            .await
            .unwrap(),
        Output::Saved(_)
    ));
    let original = tokio::fs::read(&output).await.unwrap();
    assert!(matches!(
        media::save(&client, &plan, options(), &cancel, |_| {})
            .await
            .unwrap(),
        Output::Skipped(_)
    ));
    assert_eq!(tokio::fs::read(&output).await.unwrap(), original);
    let probe = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=title,artist",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .await
        .unwrap();
    let tags: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(tags["format"]["tags"]["title"], "Fixture title");
    assert!(server.requests.load(Ordering::SeqCst) >= 4);
    assert_eq!(
        std::fs::read_dir(output.parent().unwrap()).unwrap().count(),
        1
    );
}

#[tokio::test]
async fn cenc_key_decrypts_and_wrong_key_cannot_publish_corrupt_audio() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("encrypted.m4a");
    fixture(&source, true).await;
    let bytes = tokio::fs::read(&source).await.unwrap();
    let server=Server::new(HashMap::from([("/list".into(),playlist("#EXT-X-KEY:METHOD=SAMPLE-AES-CTR,URI=\"data:text/plain;base64,AgICAgICAgICAgICAgICAg==\",KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"\n")),("/segment".into(),bytes)]),false).await;
    let client = media::client().unwrap();
    let cancel = CancellationToken::new();
    let plan = media::prepare(&client, server.base.join("list").unwrap(), &cancel)
        .await
        .unwrap();
    let output = temp.path().join("decrypted.m4a");
    let tags = Tags::default();
    let key = ContentKey::from_bytes([1; 16]);
    assert!(
        media::save(
            &client,
            &plan,
            SaveOptions {
                output: &output,
                ffmpeg: Path::new("ffmpeg"),
                tags: &tags,
                key: Some(&key)
            },
            &cancel,
            |_| {}
        )
        .await
        .is_ok()
    );
    let bad = temp.path().join("wrong.m4a");
    let wrong = ContentKey::from_bytes([3; 16]);
    assert!(
        media::save(
            &client,
            &plan,
            SaveOptions {
                output: &bad,
                ffmpeg: Path::new("ffmpeg"),
                tags: &tags,
                key: Some(&wrong)
            },
            &cancel,
            |_| {}
        )
        .await
        .is_err()
    );
    assert!(!bad.exists());
    assert!(!std::fs::read_dir(temp.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".download-")
    }));
}

#[tokio::test]
async fn cancellation_cleans_partial_output() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.ts");
    fixture(&source, false).await;
    let server = Server::new(
        HashMap::from([
            ("/list".into(), playlist("")),
            ("/segment".into(), tokio::fs::read(source).await.unwrap()),
        ]),
        false,
    )
    .await;
    let client = media::client().unwrap();
    let cancel = CancellationToken::new();
    let plan = media::prepare(&client, server.base.join("list").unwrap(), &cancel)
        .await
        .unwrap();
    let output = temp.path().join("cancelled.m4a");
    let tags = Tags::default();
    let result = media::save(
        &client,
        &plan,
        SaveOptions {
            output: &output,
            ffmpeg: Path::new("ffmpeg"),
            tags: &tags,
            key: None,
        },
        &cancel,
        |progress| {
            if matches!(progress, Progress::Download { .. }) {
                cancel.cancel();
            }
        },
    )
    .await;
    assert!(matches!(result, Err(media::Error::Cancelled)));
    assert!(!output.exists());
    assert!(!std::fs::read_dir(temp.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".download-")
    }));
}

#[tokio::test]
async fn byte_ranges_reassemble_the_original_payload() {
    let manifest=b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\n#EXT-X-BYTERANGE:4@0\nsegment\n#EXTINF:1,\n#EXT-X-BYTERANGE:4\nsegment\n#EXT-X-ENDLIST\n".to_vec();
    let server = Server::new(
        HashMap::from([
            ("/list".into(), manifest),
            ("/segment".into(), b"abcdefgh".to_vec()),
        ]),
        false,
    )
    .await;
    let client = media::client().unwrap();
    let cancel = CancellationToken::new();
    let plan = media::prepare(&client, server.base.join("list").unwrap(), &cancel)
        .await
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("reassembled");
    media::download(&client, &plan, &path, &cancel, |_| {})
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(path).await.unwrap(), b"abcdefgh");
}
