//! Browser authentication runs in a child copy of this executable. Credentials
//! travel only over its private stdout pipe, never command-line args or files.
mod browser;

use serde::{Deserialize, Serialize};
use std::{
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

pub const HELPER_ARG: &str = "--apple-music-login";
pub(super) const PROFILE_ENV: &str = "GAMDL_LOGIN_PROFILE";
const SERVICE: &str = "com.gamdl.rust.apple-music";
const USER: &str = "browser-session";
const MAX_REPLY: u64 = 32 * 1024;

// Deliberately no Debug. Serialization is restricted to the private helper pipe
// and the operating system's credential store.
#[derive(Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    token: String,
    expires: Option<u64>,
}

impl BrowserSession {
    pub fn new(token: String, expires: Option<u64>) -> Result<Self, String> {
        app::UserToken::new(token.clone()).map_err(|_| tr!("login.invalid_token"))?;
        let session = Self { token, expires };
        session.credentials()?;
        Ok(session)
    }

    pub fn credentials(&self) -> Result<app::Credentials, String> {
        if self.expires.is_some_and(|expires| expires <= now()) {
            return Err(tr!("login.expired"));
        }
        let token = app::UserToken::new(self.token.clone())
            .map_err(|_| tr!("login.invalid_saved_token"))?;
        Ok(app::Credentials::Browser(token))
    }
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Reply {
    Session { session: BrowserSession },
    Cancelled,
    Failed { message: String },
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, USER).map_err(|_| tr!("login.store_unavailable"))
}

pub fn load() -> Result<Option<BrowserSession>, String> {
    load_from(&entry()?)
}

fn load_from(entry: &keyring::Entry) -> Result<Option<BrowserSession>, String> {
    let text = match entry.get_secret() {
        Ok(text) => text,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(_) => return Err(tr!("login.store_read_failed")),
    };
    let session: BrowserSession =
        serde_json::from_slice(&text).map_err(|_| tr!("login.store_corrupt"))?;
    session.credentials()?;
    Ok(Some(session))
}

pub fn save(session: &BrowserSession) -> Result<(), String> {
    save_to(&entry()?, session)
}

fn save_to(entry: &keyring::Entry, session: &BrowserSession) -> Result<(), String> {
    session.credentials()?;
    let text = serde_json::to_vec(session).map_err(|_| tr!("login.store_write_failed"))?;
    entry
        .set_secret(&text)
        .map_err(|_| tr!("login.session_only"))
}

pub fn forget() -> Result<(), String> {
    forget_from(&entry()?)
}

fn forget_from(entry: &keyring::Entry) -> Result<(), String> {
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(tr!("login.store_delete_failed")),
    }
}

pub async fn sign_in(cancel: app::CancellationToken) -> Result<Option<BrowserSession>, String> {
    // The parent owns the profile so cancellation also cleans it up.
    let profile = tempfile::Builder::new()
        .prefix("gamdl-login-")
        .tempdir()
        .map_err(|_| tr!("login.profile_failed"))?;
    let executable = std::env::current_exe().map_err(|_| tr!("login.executable_missing"))?;
    let mut command = tokio::process::Command::new(executable);
    command
        .arg(HELPER_ARG)
        .env("GAMDL_UI_LOCALE", rust_i18n::locale().to_string())
        .env(PROFILE_ENV, profile.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Hide the helper's console, not its interactive login window.
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| tr!("login.window_failed"))?;
    let output = child.stdout.take().ok_or(tr!("login.pipe_failed"))?;
    let mut reader = BufReader::new(output.take(MAX_REPLY + 1));
    let mut reply = String::new();
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Ok(None),
        _ = tokio::time::sleep(Duration::from_secs(15 * 60)) => Err(tr!("login.timeout")),
        read = reader.read_line(&mut reply) => {
            match read {
                Ok(size) if size > 0 && size as u64 <= MAX_REPLY => parse_reply(&reply),
                _ => Err(tr!("login.window_closed")),
            }
        }
    };
    // Also handles successful authentication: no orphaned browser processes.
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

fn parse_reply(reply: &str) -> Result<Option<BrowserSession>, String> {
    match serde_json::from_str::<Reply>(reply) {
        Ok(Reply::Session { session }) => {
            session.credentials()?;
            Ok(Some(session))
        }
        Ok(Reply::Cancelled) => Ok(None),
        Ok(Reply::Failed { message }) => Err(message),
        Err(_) => Err(tr!("login.invalid_reply")),
    }
}

pub fn run_helper() {
    use std::io::Write;
    let reply = match browser::run() {
        Ok(Some(session)) => Reply::Session { session },
        Ok(None) => Reply::Cancelled,
        Err(message) => Reply::Failed { message },
    };
    let mut output = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut output, &reply);
    let _ = output.write_all(b"\n");
    let _ = output.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_expiry_and_header_injection_are_rejected() {
        assert!(BrowserSession::new("valid".into(), Some(now() - 1)).is_err());
        for token in ["", "bad;cookie", "bad\r\nHeader: value", "令牌"] {
            assert!(BrowserSession::new(token.into(), None).is_err());
        }
        assert!(BrowserSession::new("valid-token".into(), None).is_ok());
    }

    #[test]
    fn helper_protocol_handles_cancel_invalid_and_expired_results() {
        assert!(parse_reply(r#"{"status":"cancelled"}"#).unwrap().is_none());
        assert!(parse_reply("not JSON").is_err());
        let reply = r#"{"status":"session","session":{"token":"secret","expires":1}}"#;
        assert!(parse_reply(reply).is_err());
        let reply = r#"{"status":"session","session":{"token":"secret","expires":null}}"#;
        assert!(parse_reply(reply).unwrap().is_some());
    }

    fn mock_store() -> keyring::Entry {
        keyring::Entry::new_with_credential(Box::new(keyring::mock::MockCredential::default()))
    }

    #[test]
    fn saved_session_roundtrips_and_logout_is_idempotent() {
        let entry = mock_store();
        assert!(load_from(&entry).unwrap().is_none());
        let session = BrowserSession::new("test-session".into(), None).unwrap();
        save_to(&entry, &session).unwrap();
        let loaded = load_from(&entry).unwrap().unwrap();
        assert_eq!(loaded.token, session.token);
        assert!(loaded.credentials().is_ok());
        forget_from(&entry).unwrap();
        forget_from(&entry).unwrap();
        assert!(load_from(&entry).unwrap().is_none());
    }

    #[test]
    fn corrupt_or_unavailable_store_never_exposes_its_secret_in_errors() {
        let entry = mock_store();
        entry.set_secret(b"private-corrupt-secret").unwrap();
        let error = load_from(&entry).err().unwrap();
        assert!(!error.contains("private-corrupt-secret"));
        let mock: &keyring::mock::MockCredential = entry.get_credential().downcast_ref().unwrap();
        mock.set_error(keyring::Error::BadEncoding(
            b"private-corrupt-secret".to_vec(),
        ));
        let error = load_from(&entry).err().unwrap();
        assert!(!error.contains("private-corrupt-secret"));
    }
}
