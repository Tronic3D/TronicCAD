//! S3-compatible object storage, signed with AWS Signature Version 4.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::OffsetDateTime;

use crate::http::{agent, checked_base, read_body, status_error, unreachable};
use crate::provider::{check_key, StorageError, StorageProvider};
use crate::{hex, sha256_hex};

const STAMP: &[FormatItem<'static>] =
    format_description!("[year][month][day]T[hour][minute][second]Z");
const DATE: &[FormatItem<'static>] = format_description!("[year][month][day]");

pub struct S3Provider {
    endpoint: String,
    host: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
    agent: ureq::Agent,
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    // HMAC takes keys of any length, so this cannot fail.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap_or_else(|_| unreachable!());
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// RFC 3986 encoding, as signature version 4 asks for it.
fn encode(text: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b'/' if keep_slash => out.push('/'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The content of every `<tag>` in a listing. Enough for the fixed shape of an S3
/// answer, and object names outside the allowed characters are dropped later.
fn tag_values<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        out.push(&after[..end]);
        rest = &after[end + close.len()..];
    }
    out
}

struct Signed {
    url: String,
    authorization: String,
    stamp: String,
    payload_hash: String,
}

impl S3Provider {
    pub fn new(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, StorageError> {
        let (endpoint, local) = checked_base(endpoint)?;
        check_key(bucket)?;
        let url = url::Url::parse(&endpoint).map_err(|_| StorageError::InvalidUrl)?;
        let host = match url.port() {
            Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
            None => url.host_str().unwrap_or_default().to_owned(),
        };
        Ok(Self {
            endpoint,
            host,
            bucket: bucket.to_owned(),
            region: if region.trim().is_empty() {
                "us-east-1".to_owned()
            } else {
                region.trim().to_owned()
            },
            access_key: access_key.to_owned(),
            secret_key: secret_key.to_owned(),
            agent: agent(local),
        })
    }

    fn sign(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, &str)],
        payload: &[u8],
    ) -> Result<Signed, StorageError> {
        let now = OffsetDateTime::now_utc();
        let stamp = now.format(STAMP).map_err(|_| StorageError::BadResponse)?;
        let date = now.format(DATE).map_err(|_| StorageError::BadResponse)?;
        let payload_hash = sha256_hex(payload);
        let path = if key.is_empty() {
            format!("/{}", self.bucket)
        } else {
            format!("/{}/{}", self.bucket, encode(key, true))
        };
        let mut pairs: Vec<(String, String)> = query
            .iter()
            .map(|(name, value)| (encode(name, false), encode(value, false)))
            .collect();
        pairs.sort();
        let query_text = pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let canonical = format!(
            "{method}\n{path}\n{query_text}\nhost:{}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{stamp}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload_hash}",
            self.host
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let to_sign = format!(
            "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
            sha256_hex(canonical.as_bytes())
        );
        let k_date = hmac(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        );
        let k_region = hmac(&k_date, self.region.as_bytes());
        let k_service = hmac(&k_region, b"s3");
        let k_signing = hmac(&k_service, b"aws4_request");
        let signature = hex(&hmac(&k_signing, to_sign.as_bytes()));
        let url = if query_text.is_empty() {
            format!("{}{path}", self.endpoint)
        } else {
            format!("{}{path}?{query_text}", self.endpoint)
        };
        Ok(Signed {
            url,
            authorization: format!(
                "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}",
                self.access_key
            ),
            stamp,
            payload_hash,
        })
    }
}

impl S3Provider {
    /// Removes an object. Not part of the provider interface: only the cloud
    /// service uses this, when an account is deleted.
    pub fn remove(&self, key: &str) -> Result<(), StorageError> {
        check_key(key)?;
        let signed = self.sign("DELETE", key, &[], b"")?;
        let mut response = self
            .agent
            .delete(&signed.url)
            .header("authorization", &signed.authorization)
            .header("x-amz-date", &signed.stamp)
            .header("x-amz-content-sha256", &signed.payload_hash)
            .call()
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) || status == 404 {
            return Ok(());
        }
        let body = read_body(&mut response).unwrap_or_default();
        Err(status_error(status, &body))
    }
}

impl StorageProvider for S3Provider {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        check_key(key)?;
        let signed = self.sign("PUT", key, &[], bytes)?;
        let mut response = self
            .agent
            .put(&signed.url)
            .header("authorization", &signed.authorization)
            .header("x-amz-date", &signed.stamp)
            .header("x-amz-content-sha256", &signed.payload_hash)
            .send(bytes)
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body = read_body(&mut response).unwrap_or_default();
        Err(status_error(status, &body))
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        check_key(key)?;
        let signed = self.sign("GET", key, &[], b"")?;
        let mut response = self
            .agent
            .get(&signed.url)
            .header("authorization", &signed.authorization)
            .header("x-amz-date", &signed.stamp)
            .header("x-amz-content-sha256", &signed.payload_hash)
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
        let prefix = format!("{}/", prefix.trim_end_matches('/'));
        check_key(prefix.trim_end_matches('/'))?;
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![("list-type", "2"), ("prefix", prefix.as_str())];
            if let Some(token) = &token {
                query.push(("continuation-token", token));
            }
            let signed = self.sign("GET", "", &query, b"")?;
            let mut response = self
                .agent
                .get(&signed.url)
                .header("authorization", &signed.authorization)
                .header("x-amz-date", &signed.stamp)
                .header("x-amz-content-sha256", &signed.payload_hash)
                .call()
                .map_err(|e| unreachable(&e))?;
            let status = response.status().as_u16();
            let body = read_body(&mut response)?;
            if !(200..300).contains(&status) {
                return Err(status_error(status, &body));
            }
            let text = String::from_utf8_lossy(&body);
            out.extend(
                tag_values(&text, "Key")
                    .into_iter()
                    .filter(|key| check_key(key).is_ok())
                    .map(str::to_owned),
            );
            let truncated = tag_values(&text, "IsTruncated").first() == Some(&"true");
            token = tag_values(&text, "NextContinuationToken")
                .first()
                .map(|value| value.replace("&amp;", "&"));
            if !truncated || token.is_none() {
                return Ok(out);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The derivation example from the AWS documentation for signature version 4.
    #[test]
    fn derives_the_documented_signing_key() {
        let k_date = hmac(b"AWS4wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", b"20150830");
        let k_region = hmac(&k_date, b"us-east-1");
        let k_service = hmac(&k_region, b"iam");
        let k_signing = hmac(&k_service, b"aws4_request");
        assert_eq!(
            hex(&k_signing),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[test]
    fn encodes_like_rfc_3986() {
        assert_eq!(encode("a b/c~d", true), "a%20b/c~d");
        assert_eq!(encode("a/b+c", false), "a%2Fb%2Bc");
    }

    #[test]
    fn reads_listings() {
        let xml = "<ListBucketResult><IsTruncated>true</IsTruncated><Contents><Key>a/b</Key></Contents><Contents><Key>a/c</Key></Contents><NextContinuationToken>x</NextContinuationToken></ListBucketResult>";
        assert_eq!(tag_values(xml, "Key"), ["a/b", "a/c"]);
        assert_eq!(tag_values(xml, "IsTruncated"), ["true"]);
    }

    #[test]
    fn refuses_plain_http_to_other_computers() {
        assert!(matches!(
            S3Provider::new("http://s3.example.com", "b", "", "a", "s"),
            Err(StorageError::InsecureUrl)
        ));
    }
}
