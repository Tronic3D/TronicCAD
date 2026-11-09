//! WebDAV (RFC 4918), as offered by Nextcloud, many NAS systems and others.

use std::collections::BTreeSet;
use std::sync::Mutex;

use crate::http::{agent, checked_base, read_body, status_error, unreachable};
use crate::provider::{check_key, StorageError, StorageProvider};

pub struct WebDavProvider {
    base: String,
    base_path: String,
    agent: ureq::Agent,
    user: String,
    password: String,
    /// Collections known to exist, to save a request per level on each write.
    made: Mutex<BTreeSet<String>>,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let group = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let sign = |shift: u32| BASE64[((group >> shift) & 63) as usize] as char;
        out.push(sign(18));
        out.push(sign(12));
        out.push(if chunk.len() > 1 { sign(6) } else { '=' });
        out.push(if chunk.len() > 2 { sign(0) } else { '=' });
    }
    out
}

/// The hrefs of a multistatus answer, whatever namespace prefix the server uses.
fn hrefs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("href>") {
        let after = &rest[open + 5..];
        let Some(close) = after.find('<') else {
            break;
        };
        // A closing tag also holds "href>", whatever the namespace prefix.
        let closing = rest[..open]
            .rfind('<')
            .is_some_and(|tag| rest[tag..].starts_with("</"));
        if !closing {
            out.push(after[..close].trim().to_owned());
        }
        rest = &after[close..];
    }
    out
}

impl WebDavProvider {
    pub fn new(base: &str, user: &str, password: &str) -> Result<Self, StorageError> {
        let (base, local) = checked_base(base)?;
        let base_path = url::Url::parse(&base)
            .map_err(|_| StorageError::InvalidUrl)?
            .path()
            .trim_end_matches('/')
            .to_owned();
        Ok(Self {
            base,
            base_path,
            agent: agent(local),
            user: user.to_owned(),
            password: password.to_owned(),
            made: Mutex::new(BTreeSet::new()),
        })
    }

    fn auth(&self) -> String {
        format!(
            "Basic {}",
            base64(format!("{}:{}", self.user, self.password).as_bytes())
        )
    }

    fn make_collections(&self, key: &str) -> Result<(), StorageError> {
        let parts: Vec<&str> = key.split('/').collect();
        let mut path = String::new();
        for part in &parts[..parts.len().saturating_sub(1)] {
            path.push_str(part);
            path.push('/');
            if self
                .made
                .lock()
                .map(|made| made.contains(&path))
                .unwrap_or(false)
            {
                continue;
            }
            let request = ureq::http::Request::builder()
                .method("MKCOL")
                .uri(format!("{}/{path}", self.base))
                .header("authorization", self.auth())
                .body(())
                .map_err(|_| StorageError::InvalidUrl)?;
            let mut response = self.agent.run(request).map_err(|e| unreachable(&e))?;
            let status = response.status().as_u16();
            // 405 and 409-free cases: the collection exists already.
            if !matches!(status, 200..=299 | 301 | 405) {
                let body = read_body(&mut response).unwrap_or_default();
                return Err(status_error(status, &body));
            }
            if let Ok(mut made) = self.made.lock() {
                made.insert(path.clone());
            }
        }
        Ok(())
    }

    fn walk(&self, relative: &str, out: &mut Vec<String>) -> Result<(), StorageError> {
        let request = ureq::http::Request::builder()
            .method("PROPFIND")
            .uri(format!("{}/{relative}", self.base))
            .header("authorization", self.auth())
            .header("depth", "1")
            .header("content-type", "application/xml")
            .body(
                r#"<?xml version="1.0"?><propfind xmlns="DAV:"><prop><resourcetype/></prop></propfind>"#
                    .to_owned(),
            )
            .map_err(|_| StorageError::InvalidUrl)?;
        let mut response = self.agent.run(request).map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        let body = read_body(&mut response)?;
        if status == 404 {
            return Ok(());
        }
        if status != 207 {
            return Err(status_error(status, &body));
        }
        let text = String::from_utf8_lossy(&body);
        let own = format!("{}/{relative}", self.base_path);
        for href in hrefs(&text) {
            // Some servers answer with a full address, others with a path.
            let path = url::Url::parse(&href)
                .map(|url| url.path().to_owned())
                .unwrap_or(href);
            if path.trim_end_matches('/') == own.trim_end_matches('/') {
                continue;
            }
            let Some(key) = path.strip_prefix(&format!("{}/", self.base_path)) else {
                continue;
            };
            if let Some(directory) = key.strip_suffix('/') {
                // A name a server cannot be trusted with is not followed.
                if check_key(directory).is_ok() {
                    self.walk(&format!("{directory}/"), out)?;
                }
            } else if check_key(key).is_ok() {
                out.push(key.to_owned());
            }
        }
        Ok(())
    }
}

impl StorageProvider for WebDavProvider {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        check_key(key)?;
        // Parents come first: on a missing parent, a server may refuse and close the
        // connection while a large body is still on its way, which looks like going
        // offline. Measured with a real server and 8 MB.
        for attempt in 0..2 {
            self.make_collections(key)?;
            let mut response = self
                .agent
                .put(format!("{}/{key}", self.base))
                .header("authorization", self.auth())
                .header("content-type", "application/octet-stream")
                .send(bytes)
                .map_err(|e| unreachable(&e))?;
            let status = response.status().as_u16();
            if (200..300).contains(&status) {
                return Ok(());
            }
            // Someone removed a collection since we made it.
            if attempt == 0 && matches!(status, 404 | 409) {
                if let Ok(mut made) = self.made.lock() {
                    made.clear();
                }
                continue;
            }
            let body = read_body(&mut response).unwrap_or_default();
            return Err(status_error(status, &body));
        }
        Err(StorageError::BadResponse)
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        check_key(key)?;
        let mut response = self
            .agent
            .get(format!("{}/{key}", self.base))
            .header("authorization", self.auth())
            .call()
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        let body = read_body(&mut response)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &body));
        }
        Ok(Some(body))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let prefix = prefix.trim_end_matches('/');
        check_key(prefix)?;
        let mut out = Vec::new();
        self.walk(&format!("{prefix}/"), &mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_basic_auth() {
        assert_eq!(
            base64(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn finds_hrefs_under_any_prefix() {
        let xml = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/a/</D:href></D:response><response><href> /dav/a/b.txt </href></response></D:multistatus>"#;
        assert_eq!(hrefs(xml), ["/dav/a/", "/dav/a/b.txt"]);
    }
}
