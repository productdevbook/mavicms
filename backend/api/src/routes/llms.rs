//! What a site tells a program about itself.
//!
//! Wiring a front end to a CMS is a page of documentation and half a day of
//! finding out which half of it applies to this installation. This address
//! answers with the whole of it, written for whoever — or whatever — is doing
//! the wiring, and written by the site itself so that the addresses in it are
//! that site's rather than an example's.
//!
//! It answers to anybody. What it holds is how this API is shaped, which is
//! the same on every installation and is published anyway; the content behind
//! it still needs an account. A request that carries one gets the same
//! document with this site's own languages and forms appended, which is the
//! difference between an assistant guessing at locale codes and knowing them.

use axum::{
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use sea_orm::EntityTrait;
use tower_cookies::Cookies;

use crate::{entities::site_settings, error::AppResult, tenants::Site};

/// The document, with `{{…}}` where this site's own details go. Kept as a file
/// rather than as a string in this module: it is prose, it is long, and it is
/// meant to be read and edited as what it is.
const TEMPLATE: &str = include_str!("llms.txt");

/// How this site is reached, as the caller reached it.
///
/// The panel and the API are on the same address as the site, and which site
/// this is depends entirely on that address, so the answer has to be built
/// from the request rather than from anything the server holds.
fn origin(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("localhost");

    let forwarded = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim());

    format!("{}://{host}", scheme_for(forwarded, host))
}

/// Which scheme the addresses in this file should carry.
///
/// The addresses here are copied into builds verbatim, on purpose, so getting
/// this wrong does not produce a redirect — it produces a 404 that reads as
/// "the CMS has no posts", and every cover image on the generated site points
/// at one.
///
/// A forwarded `https` is taken at its word. A forwarded `http` is not, unless
/// the host looks like somebody's own machine: the header only carries the
/// last hop's scheme, so a proxy in front of a proxy reports plaintext for a
/// site that is only ever reached over TLS. Anything this image serves on a
/// real hostname sends HSTS, which settles what the transport was meant to be.
fn scheme_for(forwarded: Option<&str>, host: &str) -> &'static str {
    if forwarded.is_some_and(|scheme| scheme.eq_ignore_ascii_case("https")) {
        return "https";
    }
    if is_somebodys_own_machine(host) {
        "http"
    } else {
        "https"
    }
}

fn is_somebodys_own_machine(host: &str) -> bool {
    let name = host.split(':').next().unwrap_or(host);

    // A bare name with no dot in it is a container or a machine on a desk,
    // never a site somebody visits.
    if name == "localhost" || name.ends_with(".localhost") || !name.contains('.') {
        return true;
    }
    name.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| match ip {
            std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
            std::net::IpAddr::V6(ip) => ip.is_loopback(),
        })
}

/// How to build pages from this site.
#[utoipa::path(
    get,
    path = "/llms.txt",
    tag = "development",
    responses((status = 200, description = "The document", content_type = "text/plain"))
)]
pub async fn llms_txt(
    Site(state): Site,
    cookies: Cookies,
    headers: HeaderMap,
) -> AppResult<Response> {
    let db = state.db_or_unavailable()?;

    let title = site_settings::Entity::find()
        .one(db)
        .await?
        .map(|settings| settings.site_title)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "This site".to_string());

    let origin = origin(&headers);
    let base = format!("{origin}/api");

    let mut document = TEMPLATE
        .replace("{{title}}", &title)
        .replace("{{origin}}", &origin)
        .replace("{{base}}", &base);

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    // Signed in or not is the whole of the difference, not which account:
    // everything appended below is readable by any of them.
    if crate::auth::authenticate(&state, &cookies, bearer)
        .await
        .is_ok()
    {
        document.push_str(&this_site(db).await?);
    }

    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        document,
    )
        .into_response())
}

/// The part that is true of this site and of no other.
async fn this_site(db: &sea_orm::DatabaseConnection) -> AppResult<String> {
    use std::fmt::Write;

    use crate::entities::form;

    let mut section = String::from("\n## This site, as it stands\n\n");

    let languages = crate::languages::all(db).await?;
    if languages.is_empty() {
        section.push_str("No content languages are set up yet.\n");
    } else {
        section.push_str("Languages, as `locale` takes them:\n\n");
        for language in &languages {
            let _ = writeln!(
                section,
                "- `{}` — {}{}",
                language.code,
                language.name,
                if language.is_default {
                    ", the default"
                } else {
                    ""
                }
            );
        }
    }

    let forms = form::Entity::find().all(db).await?;
    let taking: Vec<_> = forms.iter().filter(|form| form.active).collect();
    if taking.is_empty() {
        section.push_str("\nNo form is taking answers.\n");
    } else {
        section.push_str(
            "\nForms taking answers, at `/forms/{slug}/schema` and `/forms/{slug}/submit`:\n\n",
        );
        for form in taking {
            let _ = writeln!(section, "- `{}` — {}", form.slug, form.name);
        }
    }

    let lists = crate::entities::mail_list::Entity::find().all(db).await?;
    let public: Vec<_> = lists.iter().filter(|list| list.public).collect();
    if !public.is_empty() {
        section.push_str("\nMailing lists taking sign-ups, as `lists` takes them:\n\n");
        for list in public {
            let _ = writeln!(section, "- `{}` — {}", list.slug, list.name);
        }
    }

    section.push_str(
        "\nThis part is only in the copy a signed-in request gets. Fetched \
         without an account, this file is the same document without it.\n",
    );

    Ok(section)
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, header};

    use super::{origin, scheme_for};

    #[test]
    fn the_address_is_the_one_the_caller_used() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "example.test".parse().unwrap());

        assert_eq!(origin(&headers), "https://example.test");
    }

    #[test]
    fn an_ingress_saying_http_about_a_real_hostname_is_not_believed() {
        // It used to be. The header carries the last hop's scheme, and behind
        // a proxy in front of a proxy that hop is plaintext for a site only
        // ever reached over TLS — so this said http:// about an address that
        // answers 404, in the one file builds are told to copy verbatim.
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "example.test".parse().unwrap());
        headers.insert("x-forwarded-proto", "http".parse().unwrap());

        assert_eq!(origin(&headers), "https://example.test");
    }

    /// Proxies chain the header rather than replacing it; the first hop is the
    /// browser's.
    #[test]
    fn a_chained_header_takes_the_first() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "example.test".parse().unwrap());
        headers.insert("x-forwarded-proto", "https,http".parse().unwrap());

        assert_eq!(origin(&headers), "https://example.test");
    }

    #[test]
    fn a_machine_developing_locally_is_not_sent_to_https() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "localhost:5173".parse().unwrap());

        assert_eq!(origin(&headers), "http://localhost:5173");
    }

    #[test]
    fn a_site_behind_a_proxy_in_front_of_a_proxy_is_still_https() {
        // What the shipped nginx used to report for a TLS-terminated site.
        assert_eq!(scheme_for(Some("http"), "example.com"), "https");
        assert_eq!(scheme_for(Some("https"), "example.com"), "https");
        assert_eq!(scheme_for(Some("HTTPS"), "example.com"), "https");
        assert_eq!(scheme_for(None, "example.com"), "https");
    }

    #[test]
    fn somebody_running_it_on_their_own_machine_keeps_http() {
        for host in [
            "localhost",
            "localhost:5173",
            "site.localhost",
            "127.0.0.1:8080",
            "[::1]:8080",
            "192.168.1.5:8080",
            "mavicms",
        ] {
            assert_eq!(scheme_for(Some("http"), host), "http", "{host}");
        }
    }
}
