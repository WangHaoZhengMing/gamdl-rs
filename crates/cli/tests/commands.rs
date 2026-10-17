use std::process::Command;

#[test]
fn parse_is_offline_and_does_not_require_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args([
            "--config",
            "/nonexistent/config.toml",
            "parse",
            "https://music.apple.com/us/album/title/123?i=456",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["kind"], "song");
    assert_eq!(value["id"], "456");
}

#[test]
fn unsupported_url_exits_unsuccessfully_without_json_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["parse", "https://example.com/us/song/123"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid Apple Music URL"));
}
