//! Answering a caller that already has the answer.
//!
//! A site generator asks for the whole archive on every build and every reload
//! of the development server, and most of those builds follow a change to one
//! post — or to none at all. The rows still have to be read to know that
//! nothing moved, but the megabytes do not have to be sent again.

use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::Serialize;

use crate::error::{AppError, AppResult};

/// A JSON body with an `ETag`, answered with 304 when the caller sent that tag
/// back in `If-None-Match`.
///
/// The tag is a hash of the body, so it is exactly as accurate as the answer
/// is: anything that would change a byte of what is sent changes the tag. It
/// is not a strong tag in the HTTP sense — two responses with the same tag are
/// byte-identical here, but nothing promises that of a future field ordering,
/// so it is marked weak.
pub fn conditional<T: Serialize>(headers: &HeaderMap, body: &T) -> AppResult<Response> {
    let bytes = serde_json::to_vec(body)
        .map_err(|err| AppError::Internal(format!("could not write the answer: {err}")))?;

    let tag = {
        use sha2::{Digest, Sha256};
        format!("W/\"{}\"", hex::encode(&Sha256::digest(&bytes)[..16]))
    };

    if matches(headers, &tag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &tag)
            .body(Body::empty())
            .map_err(|err| AppError::Internal(err.to_string()));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ETAG, &tag)
        .body(Body::from(bytes))
        .map_err(|err| AppError::Internal(err.to_string()))
}

/// Whether `If-None-Match` names this tag. The header is a comma-separated
/// list, and `*` means "whatever you have".
fn matches(headers: &HeaderMap, tag: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    value.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            // A tag sent back weak is the same tag: some clients drop the
            // prefix, and refusing them a 304 only costs the transfer this
            // exists to save.
            || candidate.trim_start_matches("W/") == tag.trim_start_matches("W/")
    })
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, StatusCode, header};

    use super::conditional;

    fn tag_of(response: &axum::response::Response) -> String {
        response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .expect("every answer carries one")
            .to_string()
    }

    #[test]
    fn the_same_body_gets_the_same_tag() {
        let empty = HeaderMap::new();
        let first = conditional(&empty, &serde_json::json!({"a": 1})).unwrap();
        let second = conditional(&empty, &serde_json::json!({"a": 1})).unwrap();

        assert_eq!(tag_of(&first), tag_of(&second));
    }

    #[test]
    fn a_different_body_gets_a_different_one() {
        let empty = HeaderMap::new();
        let first = conditional(&empty, &serde_json::json!({"a": 1})).unwrap();
        let second = conditional(&empty, &serde_json::json!({"a": 2})).unwrap();

        assert_ne!(tag_of(&first), tag_of(&second));
    }

    #[test]
    fn sending_the_tag_back_gets_nothing_back() {
        let body = serde_json::json!({"a": 1});
        let tag = tag_of(&conditional(&HeaderMap::new(), &body).unwrap());

        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, tag.parse().unwrap());

        let response = conditional(&headers, &body).unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[test]
    fn a_stale_tag_gets_the_answer() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, "W/\"0000\"".parse().unwrap());

        let response = conditional(&headers, &serde_json::json!({"a": 1})).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn one_of_several_tags_is_enough() {
        let body = serde_json::json!({"a": 1});
        let tag = tag_of(&conditional(&HeaderMap::new(), &body).unwrap());

        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            format!("W/\"0000\", {tag}").parse().unwrap(),
        );

        assert_eq!(
            conditional(&headers, &body).unwrap().status(),
            StatusCode::NOT_MODIFIED
        );
    }
}
