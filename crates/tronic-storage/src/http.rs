//! What the HTTP providers share: address rules and connection settings.

use std::io::Read;
use std::time::Duration;
use url::{Host, Url};

use crate::provider::StorageError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(1800);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// A project file is limited to 512 MB, see `tronic_project::store`.
pub(crate) const MAX_OBJECT_BYTES: u64 = 600 * 1024 * 1024;
const MAX_ERROR_CHARS: usize = 300;

/// Plain http only to this computer, as for AI providers. Passwords and project
/// data would otherwise travel unencrypted.
pub fn checked_base(base: &str) -> Result<(String, bool), StorageError> {
    let url = Url::parse(base.trim()).map_err(|_| StorageError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(StorageError::InvalidUrl);
    }
    let local = match url.host() {
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => return Err(StorageError::InvalidUrl),
    };
    if !local && url.scheme() != "https" {
        return Err(StorageError::InsecureUrl);
    }
    Ok((url.as_str().trim_end_matches('/').to_owned(), local))
}

/// No redirects, so that an answer cannot send credentials or data elsewhere.
pub fn agent(local: bool) -> ureq::Agent {
    let proxy = if local {
        None
    } else {
        ureq::Proxy::try_from_env()
    };
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .allow_non_standard_methods(true)
        .proxy(proxy)
        // The certificates of the operating system: a company's own certificate
        // authority works once it is installed on the computer.
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(TRANSFER_TIMEOUT))
        .timeout_send_body(Some(TRANSFER_TIMEOUT))
        .build()
        .into()
}

pub(crate) fn unreachable(error: &ureq::Error) -> StorageError {
    StorageError::Unreachable(error.to_string())
}

pub(crate) fn read_body(
    response: &mut ureq::http::Response<ureq::Body>,
) -> Result<Vec<u8>, StorageError> {
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_OBJECT_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| StorageError::Unreachable(error.to_string()))?;
    Ok(bytes)
}

/// Never includes request headers, so credentials cannot reach a message this way.
pub(crate) fn status_error(status: u16, body: &[u8]) -> StorageError {
    match status {
        401 => StorageError::Unauthorized,
        403 => StorageError::Forbidden,
        413 | 507 => StorageError::QuotaExceeded,
        _ => StorageError::Server {
            status,
            message: String::from_utf8_lossy(body)
                .chars()
                .take(MAX_ERROR_CHARS)
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_only_to_this_computer() {
        assert!(checked_base("http://127.0.0.1:8080/dav/").is_ok());
        assert_eq!(
            checked_base("https://s3.example.com").map(|b| b.1),
            Ok(false)
        );
        assert_eq!(
            checked_base("http://nas.local"),
            Err(StorageError::InsecureUrl)
        );
        assert_eq!(
            checked_base("http://localhost.example.com"),
            Err(StorageError::InsecureUrl)
        );
        assert_eq!(
            checked_base("https://a:b@example.com"),
            Err(StorageError::InvalidUrl)
        );
        assert_eq!(
            checked_base("ftp://localhost"),
            Err(StorageError::InvalidUrl)
        );
    }

    #[test]
    fn the_trailing_slash_does_not_matter() {
        assert_eq!(
            checked_base("http://localhost:1/x/").map(|b| b.0),
            Ok("http://localhost:1/x".to_owned())
        );
    }
}
