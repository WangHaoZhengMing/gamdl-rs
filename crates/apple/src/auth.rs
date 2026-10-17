use crate::Error;
use reqwest::{
    Client,
    header::{AUTHORIZATION, COOKIE, HeaderMap, HeaderValue, ORIGIN},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Intentionally no Debug/Serialize: tokens must not reach logs or JSON output.
#[derive(Clone)]
pub struct Session {
    pub(crate) client: Client,
}

#[derive(Deserialize)]
struct AccountResponse {
    meta: AccountMeta,
}
#[derive(Deserialize)]
struct AccountMeta {
    subscription: Subscription,
}
#[derive(Deserialize)]
struct Subscription {
    active: bool,
    storefront: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub active_subscription: bool,
    pub storefront: String,
}

pub(crate) fn user_token(cookies: &str, now: u64) -> Result<String, Error> {
    let mut token = None;
    for line in cookies.lines() {
        let line = line.trim_end_matches('\r');
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 7 {
            return Err(Error::InvalidCookies(
                "expected Netscape format with seven tab-separated fields",
            ));
        }
        if !matches!(fields[0], ".music.apple.com" | "music.apple.com")
            || fields[5] != "media-user-token"
        {
            continue;
        }
        let expires: u64 = fields[4]
            .parse()
            .map_err(|_| Error::InvalidCookies("invalid expiry"))?;
        if expires != 0 && expires <= now {
            return Err(Error::ExpiredCookies);
        }
        if fields[6].is_empty() || fields[6].bytes().any(|b| b <= 32 || b >= 127 || b == b';') {
            return Err(Error::InvalidCookies("invalid token"));
        }
        if token.is_some() {
            return Err(Error::InvalidCookies(
                "multiple media-user-token cookies; export a fresh file",
            ));
        }
        token = Some(fields[6].to_owned());
    }
    token.ok_or(Error::InvalidCookies(
        "media-user-token for music.apple.com is missing",
    ))
}

pub async fn check_session(cookies: &str) -> Result<(Session, Account), Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let user = user_token(cookies, now)?;
    let public_client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let home = public_client
        .get("https://music.apple.com")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let assets = regex::Regex::new(r#"/assets/index[~-][^/"\s]+\.js"#).expect("constant regex");
    let path = assets.find(&home).ok_or(Error::TokenUnavailable)?.as_str();
    let script = public_client
        .get(format!("https://music.apple.com{path}"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let jwt = regex::Regex::new(r#"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"#)
        .expect("constant regex");
    let developer = jwt.find(&script).ok_or(Error::TokenUnavailable)?.as_str();
    let mut headers = HeaderMap::new();
    for (name, value) in [
        (AUTHORIZATION, format!("Bearer {developer}")),
        (COOKIE, format!("media-user-token={user}")),
    ] {
        let mut header = HeaderValue::from_str(&value)
            .map_err(|_| Error::InvalidCookies("invalid authentication header"))?;
        header.set_sensitive(true);
        headers.insert(name, header);
    }
    headers.insert(ORIGIN, HeaderValue::from_static("https://music.apple.com"));
    // Never follow redirects with credential-bearing headers.
    let client = Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client
        .get("https://amp-api.music.apple.com/v1/me/account")
        .query(&[("meta", "subscription")])
        .send()
        .await?
        .error_for_status()?;
    let response: AccountResponse = response.json().await?;
    let subscription = response.meta.subscription;
    let account = Account {
        active_subscription: subscription.active,
        storefront: subscription.storefront,
    };
    Ok((Session { client }, account))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_http_only_session_cookie() {
        assert_eq!(user_token("# Netscape HTTP Cookie File\n#HttpOnly_.music.apple.com\tTRUE\t/\tTRUE\t0\tmedia-user-token\tsecret\r\n", 100).unwrap(), "secret");
    }
    #[test]
    fn rejects_expired_foreign_and_ambiguous_cookies() {
        let row = ".music.apple.com\tTRUE\t/\tTRUE\t99\tmedia-user-token\tsecret";
        assert!(matches!(user_token(row, 100), Err(Error::ExpiredCookies)));
        assert!(user_token(&row.replace(".music.apple.com", ".example.com"), 1).is_err());
        assert!(user_token(&format!("{row}\n{row}"), 1).is_err());
        assert!(user_token(&row.replace("secret", "bad;cookie"), 1).is_err());
    }
}
