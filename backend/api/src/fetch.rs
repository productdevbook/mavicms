//! Fetching remote files on behalf of an authenticated admin — used by the
//! WordPress migration to pull media across from the old site.
//!
//! The URL comes from the client, so this is a server-side request forgery
//! surface: without checks it would let anyone who can log in read the cloud
//! metadata endpoint, probe the cluster's internal services, or reach a
//! database admin panel that is only bound to localhost. Every hostname is
//! therefore resolved up front and refused if it lands on a non-public
//! address, and redirects are followed manually so each hop is checked the
//! same way.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use reqwest::{StatusCode, Url, redirect::Policy};

const MAX_REDIRECTS: usize = 3;
// Anonymous requests are refused outright by many CDNs and WordPress security
// plugins, which is exactly the kind of host media is imported from.
const USER_AGENT: &str = concat!(
    "MaviCMS/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/productdevbook/mavicms)"
);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    Rejected(String),
    #[error("could not fetch {0}")]
    Unreachable(String),
    #[error("the file is larger than the {0} MB limit")]
    TooLarge(usize),
}

/// Addresses that must never be reachable through a user-supplied URL.
fn is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                // 100.64.0.0/10, carrier-grade NAT.
                || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
                // 192.0.0.0/24, IETF protocol assignments.
                || ip.octets()[..3] == [192, 0, 0]
                // 198.18.0.0/15, benchmarking.
                || (ip.octets()[0] == 198 && (ip.octets()[1] & 0xfe) == 18)
                // 240.0.0.0/4, reserved.
                || ip.octets()[0] >= 240
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            match segments {
                // ::ffff:a.b.c.d — an IPv4 address in a v6 costume, same host.
                [0, 0, 0, 0, 0, 0xffff, ..] => match to_ipv4(ip) {
                    Some(mapped) => is_forbidden(IpAddr::V4(mapped)),
                    None => true,
                },
                // The deprecated ::/96 compatible range, which also covers ::1
                // and ::. Nothing legitimate is reachable there.
                [0, 0, 0, 0, 0, 0, ..] => true,
                _ => {
                    ip.is_loopback()
                        || ip.is_unspecified()
                        || ip.is_multicast()
                        // fc00::/7 unique local, fe80::/10 link-local.
                        || (segments[0] & 0xfe00) == 0xfc00
                        || (segments[0] & 0xffc0) == 0xfe80
                }
            }
        }
    }
}

fn to_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    match segments {
        [0, 0, 0, 0, 0, 0xffff, a, b] => Some(Ipv4Addr::new(
            (a >> 8) as u8,
            a as u8,
            (b >> 8) as u8,
            b as u8,
        )),
        _ => None,
    }
}

/// Resolves the URL's host, refuses anything that is not a public address, and
/// returns the domain and address the connection must be pinned to.
///
/// Returns `None` for an address literal, where there is no name to rebind.
async fn resolve_public(url: &Url) -> Result<Option<(String, SocketAddr)>, FetchError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::Rejected(
            "only http and https addresses can be imported".to_string(),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| FetchError::Rejected("the address has no host".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(80);

    // An IPv6 literal arrives bracketed, which the resolver will not parse.
    let bare = host.trim_start_matches('[').trim_end_matches(']');

    let (addresses, is_literal) = match bare.parse::<IpAddr>() {
        Ok(ip) => (vec![ip], true),
        Err(_) => (
            tokio::net::lookup_host((bare, port))
                .await
                .map_err(|_| FetchError::Unreachable(host.to_string()))?
                .map(|addr| addr.ip())
                .collect(),
            false,
        ),
    };

    let first = *addresses
        .first()
        .ok_or_else(|| FetchError::Unreachable(host.to_string()))?;
    // Every answer has to be public: one private address among them is enough
    // for the connection to end up there.
    if addresses.iter().copied().any(is_forbidden) {
        return Err(FetchError::Rejected(format!(
            "{host} resolves to a private address, which cannot be imported"
        )));
    }

    Ok((!is_literal).then(|| (bare.to_string(), SocketAddr::new(first, port))))
}

/// Refuses a URL whose host is not on the public internet.
///
/// For an address this server is configured to talk to rather than one it was
/// handed in a request — a storage endpoint, say. Those are set by whoever
/// runs a site, who is not whoever runs the server, and an endpoint pointing
/// at the cluster would make the server fetch from inside itself and hand the
/// answer back in an error message.
///
/// The name is resolved every time rather than only when it is saved: a name
/// that answered publicly this morning can answer 127.0.0.1 this afternoon,
/// and that is somebody else's DNS to change.
pub async fn ensure_public_host(url: &str) -> Result<(), FetchError> {
    let parsed =
        Url::parse(url).map_err(|_| FetchError::Rejected(format!("{url} is not an address")))?;
    resolve_public(&parsed).await.map(|_| ())
}

/// Downloads a remote file, refusing private hosts and anything over `max_bytes`.
pub async fn fetch_remote_file(url: &str, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
    let mut current = Url::parse(url)
        .map_err(|_| FetchError::Rejected("that is not a valid address".to_string()))?;

    for _ in 0..=MAX_REDIRECTS {
        let pinned = resolve_public(&current).await?;

        let mut builder = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            // Followed by hand below so that every hop gets the same host check.
            .redirect(Policy::none());
        // Connect to the address that was just checked. Resolving again inside
        // the client would let a hostile DNS server answer with a public
        // address for the check and a private one for the connection; the
        // hostname is kept for SNI and Host, so TLS still has to match.
        if let Some((domain, address)) = &pinned {
            builder = builder.resolve(domain, *address);
        }
        let client = builder
            .build()
            .map_err(|_| FetchError::Unreachable(current.to_string()))?;

        let response = client
            .get(current.clone())
            .send()
            .await
            .map_err(|_| FetchError::Unreachable(current.to_string()))?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    FetchError::Unreachable(format!("{current} redirected without a target"))
                })?;
            current = current.join(location).map_err(|_| {
                FetchError::Rejected("the redirect target is not a valid address".to_string())
            })?;
            continue;
        }

        if response.status() != StatusCode::OK {
            return Err(FetchError::Unreachable(format!(
                "{current} returned {}",
                response.status()
            )));
        }

        // Content-Length is a hint, not a promise — the running total below is
        // what actually stops an oversized or lying response.
        if response
            .content_length()
            .is_some_and(|len| len > max_bytes as u64)
        {
            return Err(FetchError::TooLarge(max_bytes / (1024 * 1024)));
        }

        let mut response = response;
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| FetchError::Unreachable(current.to_string()))?
        {
            if bytes.len() + chunk.len() > max_bytes {
                return Err(FetchError::TooLarge(max_bytes / (1024 * 1024)));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }

    Err(FetchError::Rejected("too many redirects".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forbidden(literal: &str) -> bool {
        is_forbidden(literal.parse().unwrap())
    }

    #[test]
    fn refuses_loopback_and_private_ranges() {
        for address in [
            "127.0.0.1",
            "127.1.2.3",
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "0.0.0.0",
            "100.64.0.1",
            "192.0.0.1",
            "198.18.0.1",
            "255.255.255.255",
        ] {
            assert!(forbidden(address), "{address} should be refused");
        }
    }

    #[test]
    fn refuses_ipv6_private_and_mapped_addresses() {
        for address in [
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
        ] {
            assert!(forbidden(address), "{address} should be refused");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for address in ["1.1.1.1", "8.8.8.8", "93.184.216.34", "2606:4700::1111"] {
            assert!(!forbidden(address), "{address} should be allowed");
        }
    }

    #[tokio::test]
    async fn refuses_non_http_schemes() {
        let error = fetch_remote_file("file:///etc/passwd", 1024)
            .await
            .unwrap_err();
        assert!(matches!(error, FetchError::Rejected(_)));
    }

    #[tokio::test]
    async fn refuses_localhost_by_name() {
        let error = fetch_remote_file("http://localhost:8080/x.png", 1024)
            .await
            .unwrap_err();
        assert!(matches!(error, FetchError::Rejected(_)), "got {error:?}");
    }
}
