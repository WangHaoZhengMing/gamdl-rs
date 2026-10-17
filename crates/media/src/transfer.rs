use crate::{Error, Plan, Progress};
use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use reqwest::{
    Client, StatusCode,
    header::{CONTENT_RANGE, RANGE},
};
use std::{collections::HashMap, path::Path, time::Duration};
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use url::Url;

pub(crate) async fn fetch(
    client: &Client,
    url: &Url,
    range: Option<(u64, u64)>,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<(Vec<u8>, Url), Error> {
    for attempt in 0..4 {
        let operation = async {
            let mut request = client.get(url.clone());
            if let Some((start, end)) = range {
                request = request.header(RANGE, format!("bytes={start}-{end}"));
            }
            let mut response = request.send().await?;
            let status = response.status();
            if let Some((start, end)) = range
                && status.is_success()
                && (status != StatusCode::PARTIAL_CONTENT
                    || !response
                        .headers()
                        .get(CONTENT_RANGE)
                        .and_then(|h| h.to_str().ok())
                        .is_some_and(|s| s.starts_with(&format!("bytes {start}-{end}/"))))
            {
                return Err(Error::Playlist("server did not honor byte range"));
            }
            response = response.error_for_status()?;
            if response
                .content_length()
                .is_some_and(|length| length > limit as u64)
            {
                return Err(Error::Playlist("resource exceeds size limit"));
            }
            let actual = response.url().clone();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if bytes.len().saturating_add(chunk.len()) > limit {
                    return Err(Error::Playlist("resource exceeds size limit"));
                }
                bytes.extend_from_slice(&chunk);
            }
            if let Some((start, end)) = range
                && bytes.len() as u64 != end - start + 1
            {
                return Err(Error::Playlist("truncated byte range"));
            }
            Ok((bytes, actual))
        };
        let result = tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),result=operation=>result};
        match result {
            Ok(value) => return Ok(value),
            Err(Error::Http(ref error))
                if attempt < 3
                    && (error.is_timeout()
                        || error.is_connect()
                        || error.is_body()
                        || error.status().is_some_and(|s| {
                            s.is_server_error()
                                || s == StatusCode::TOO_MANY_REQUESTS
                                || s == StatusCode::REQUEST_TIMEOUT
                        })) =>
            {
                tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(250 << attempt))=>{}}
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("retry loop returns")
}

pub async fn download(
    client: &Client,
    plan: &Plan,
    path: &Path,
    cancel: &CancellationToken,
    progress: impl Fn(Progress),
) -> Result<(), Error> {
    let mut output = File::create(path).await?;
    let mut keys: HashMap<String, zeroize::Zeroizing<Vec<u8>>> = HashMap::new();
    let mut bytes = 0;
    for (index, piece) in plan.pieces.iter().enumerate() {
        let (mut data, _) =
            fetch(client, &piece.url, piece.range, 64 * 1024 * 1024, cancel).await?;
        if let Some(aes) = &piece.aes {
            if !keys.contains_key(aes.url.as_str()) {
                let (key, _) = fetch(client, &aes.url, None, 16, cancel).await?;
                if key.len() != 16 {
                    return Err(Error::InvalidKey);
                }
                keys.insert(aes.url.to_string(), zeroize::Zeroizing::new(key));
            }
            let key = &keys[aes.url.as_str()];
            data = cbc::Decryptor::<aes::Aes128>::new_from_slices(key, &aes.iv)
                .map_err(|_| Error::InvalidKey)?
                .decrypt_padded_vec_mut::<Pkcs7>(&data)
                .map_err(|_| Error::Aes)?;
        }
        tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled), result=output.write_all(&data)=>result?};
        bytes += data.len() as u64;
        progress(Progress::Download {
            completed: index + 1,
            total: plan.pieces.len(),
            bytes,
        });
    }
    output.flush().await?;
    Ok(())
}
