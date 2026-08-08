//! Talking to a site as a set of tools, rather than as an API.
//!
//! An agency looks after fifty sites and a person looks after one, and both
//! spend their day on questions — which site did not build, what came through
//! the contact form, put this correction in the third paragraph — that are
//! each a small errand through a panel. This is the same panel with a
//! different way in: an assistant holding a token can answer them.
//!
//! It is the Model Context Protocol, and this file is the transport. What it
//! answers is decided by two things that were already true of this system: the
//! address says which site is being asked about, and the credential says who
//! is asking. A build token, which can read a site and change nothing, is not
//! offered the tools that write — it is offered fewer tools rather than tools
//! that refuse, because a tool an assistant cannot use is better not described
//! to it.
//!
//! The protocol went stateless in its 2026-07-28 revision: no handshake, no
//! session, every request carrying its own version. That is why this is a
//! single POST handler and not a connection. Clients that have not caught up
//! still open with `initialize`, so that is answered too.

pub mod console;
pub mod site;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_cookies::Cookies;

use crate::{
    error::AppError,
    state::AppState,
    tenants::{Hosting, Resolved, Site},
};

/// The revision this speaks. Stateless, one POST per message.
pub const MODERN: &str = "2026-07-28";

/// Revisions that open with a handshake. Answered because clients ship on
/// their own schedule, and a panel that only works with next month's client
/// is a panel nobody can point at anything.
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

/// How long a client may hold on to a tool list.
///
/// Private, not public: which tools come back depends on the credential, and a
/// shared cache that answered an agency's question with a build token's tools
/// would be wrong in the direction that matters.
const TOOLS_TTL_MS: u64 = 5 * 60 * 1000;

/// The largest message this reads. A tool call carrying a post is the biggest
/// of them, and a post is prose.
pub const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// The headers do not agree with the body.
const HEADER_MISMATCH: i64 = -32020;
/// This server does not speak the version that was asked for.
const UNSUPPORTED_VERSION: i64 = -32022;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const PARSE_ERROR: i64 = -32700;

#[derive(Deserialize)]
struct Call {
    /// Absent on a notification, which wants no answer.
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: String,
    #[serde(default)]
    params: Value,
}

impl Call {
    fn meta(&self, key: &str) -> Option<&Value> {
        self.params.get("_meta")?.get(key)
    }

    fn arguments(&self) -> &Value {
        self.params.get("arguments").unwrap_or(&Value::Null)
    }

    fn tool(&self) -> Option<&str> {
        self.params.get("name")?.as_str()
    }
}

/// Which era of the protocol a request is written in.
enum Era {
    /// 2026-07-28 and later: version in `_meta`, headers mirroring the body.
    Modern,
    /// A handshake-era client. Header rules do not apply to it.
    Legacy,
}

/// Everything this endpoint answers.
///
/// One address, on every site and on the server's own. Which of the two tool
/// sets it offers is settled by [`caller`].
pub async fn endpoint(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    Site(state): Site,
    cookies: Cookies,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(refusal) = wrong_origin(&headers) {
        return refusal;
    }

    let call: Call = match serde_json::from_slice(&body) {
        Ok(call) => call,
        Err(err) => {
            return error(
                StatusCode::BAD_REQUEST,
                None,
                PARSE_ERROR,
                &format!("that is not a JSON-RPC message: {err}"),
                None,
            );
        }
    };

    // A notification wants no answer, and this revision defines none that a
    // client sends. Accepting it is the whole of the handling.
    let Some(id) = call.id.clone() else {
        return StatusCode::ACCEPTED.into_response();
    };

    let era = match era(&headers, &call, &id) {
        Ok(era) => era,
        Err(refusal) => return *refusal,
    };

    if let Era::Modern = era
        && let Some(refusal) = headers_disagree(&headers, &call, &id)
    {
        return refusal;
    }

    match call.method.as_str() {
        "server/discover" => result(&id, discover()),
        // A handshake-era client opens with this. Answering it in its own
        // version is what "dual-era" means; the tools below are the same.
        "initialize" => result(&id, initialize(&call)),
        "ping" => result(&id, json!({})),
        "tools/list" | "tools/call" => {
            let caller = match caller(&hosting, &resolved, &state, &cookies, &headers).await {
                Ok(caller) => caller,
                Err(refusal) => return refusal,
            };

            if call.method == "tools/list" {
                return result(&id, listing(&caller));
            }

            let Some(name) = call.tool().map(str::to_string) else {
                return error(
                    StatusCode::BAD_REQUEST,
                    Some(&id),
                    INVALID_PARAMS,
                    "the call did not say which tool",
                    None,
                );
            };

            match invoke(&hosting, &resolved, &caller, &name, call.arguments()).await {
                Some(Ok(value)) => result(&id, produced(value)),
                // The tool ran and could not do what was asked. Handed back as
                // a result rather than a protocol error, because it is the
                // kind of failure an assistant can read and try again from.
                Some(Err(err)) => result(&id, refused(&err.to_string())),
                None => error(
                    StatusCode::BAD_REQUEST,
                    Some(&id),
                    INVALID_PARAMS,
                    &format!("there is no tool called {name} here"),
                    None,
                ),
            }
        }
        other => error(
            StatusCode::NOT_FOUND,
            Some(&id),
            METHOD_NOT_FOUND,
            &format!("this server does not answer {other}"),
            None,
        ),
    }
}

/// A browser on somebody else's page must not be able to reach this.
///
/// The panel and the API are cross-origin by design — a build calls them from
/// anywhere — but those need a token in a header, which a form on another site
/// cannot add. This endpoint accepts a cookie, so the same request can be made
/// by a page the reader did not write, and the `Origin` header is what tells
/// the two apart. Anything that names an origin at all must name ours.
fn wrong_origin(headers: &HeaderMap) -> Option<Response> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())?;

    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    // Compared without the scheme: an ingress terminates TLS, so the server
    // never sees whether the browser said https, and the host is the part that
    // decides whose site this is.
    let named = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);

    (named != host).then(|| {
        (
            StatusCode::FORBIDDEN,
            axum::Json(json!({
                "jsonrpc": "2.0",
                "error": { "code": -32600, "message": "this address does not answer that origin" }
            })),
        )
            .into_response()
    })
}

/// Which era a request is written in, or the refusal that says why neither.
fn era(headers: &HeaderMap, call: &Call, id: &Value) -> Result<Era, Box<Response>> {
    let header = headers
        .get("mcp-protocol-version")
        .and_then(|value| value.to_str().ok());

    let stated = call
        .meta("io.modelcontextprotocol/protocolVersion")
        .and_then(Value::as_str);

    // A version in two places that disagree is the case the header rules exist
    // for: something between here and the client is routing on one of them.
    if let (Some(header), Some(stated)) = (header, stated)
        && header != stated
    {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            Some(id),
            HEADER_MISMATCH,
            &format!("MCP-Protocol-Version says {header} and the message says {stated}"),
            None,
        )));
    }

    match header.or(stated) {
        Some(MODERN) => Ok(Era::Modern),
        Some(version) if LEGACY.contains(&version) => Ok(Era::Legacy),
        Some(version) => Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            Some(id),
            UNSUPPORTED_VERSION,
            "unsupported protocol version",
            Some(json!({ "supported": supported(), "requested": version })),
        ))),
        // Older than the header itself. Read as a handshake-era client, which
        // is what it is.
        None => Ok(Era::Legacy),
    }
}

fn supported() -> Vec<&'static str> {
    std::iter::once(MODERN)
        .chain(LEGACY.iter().copied())
        .collect()
}

/// Whether the mirrored headers say what the body says.
///
/// They exist so that a load balancer can route on the operation without
/// reading the body, which only holds if the two cannot differ — so a server
/// that reads the body has to be the one that checks.
fn headers_disagree(headers: &HeaderMap, call: &Call, id: &Value) -> Option<Response> {
    let mismatch = |message: String| {
        Some(error(
            StatusCode::BAD_REQUEST,
            Some(id),
            HEADER_MISMATCH,
            &message,
            None,
        ))
    };

    let method = headers
        .get("mcp-method")
        .and_then(|value| value.to_str().ok());
    match method {
        None => return mismatch("Mcp-Method is required".to_string()),
        Some(method) if method != call.method => {
            return mismatch(format!(
                "Mcp-Method says {method} and the message says {}",
                call.method
            ));
        }
        Some(_) => {}
    }

    if call.method != "tools/call" {
        return None;
    }

    let named = headers
        .get("mcp-name")
        .and_then(|value| value.to_str().ok())
        .map(decode_name);
    match (named, call.tool()) {
        (None, _) => mismatch("Mcp-Name is required on tools/call".to_string()),
        (Some(Some(named)), Some(tool)) if named == tool => None,
        (Some(Some(named)), _) => {
            mismatch(format!("Mcp-Name says {named} and the message does not"))
        }
        (Some(None), _) => mismatch("Mcp-Name is not encoded the way it says it is".to_string()),
    }
}

/// A header value that could not be written as plain ASCII arrives wrapped.
/// `None` means it said it was wrapped and was not.
fn decode_name(value: &str) -> Option<String> {
    let Some(encoded) = value
        .strip_prefix("=?base64?")
        .and_then(|rest| rest.strip_suffix("?="))
    else {
        return Some(value.to_string());
    };

    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn discover() -> Value {
    json!({
        "supportedVersions": supported(),
        "capabilities": { "tools": {} },
        "instructions": INSTRUCTIONS,
        "_meta": { "io.modelcontextprotocol/serverInfo": server_info() },
        "ttlMs": TOOLS_TTL_MS,
        "cacheScope": "private",
    })
}

/// The answer a handshake-era client opens with.
fn initialize(call: &Call) -> Value {
    let asked = call
        .params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(LEGACY[0]);

    // Its own version back if it is one this answers, and otherwise the newest
    // handshake-era one — a legacy client has no way to ask again.
    let version = if LEGACY.contains(&asked) {
        asked
    } else {
        LEGACY[0]
    };

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    })
}

fn server_info() -> Value {
    json!({ "name": "mavicms", "version": env!("CARGO_PKG_VERSION") })
}

const INSTRUCTIONS: &str = "This is a Mavi CMS site. Which site depends on the address you \
    reached it on. \
    \
    If none of the tools you were offered can write, this connection was made with a build \
    token, which is the read-only kind. Two things fix it: connect again using the address on \
    its own, with no Authorization header, and the site will ask whoever is connecting to sign \
    in; or ask whoever the site belongs to for an assistant key, made in the panel under API, \
    and send that in the header instead. \
    \
    What this site holds is posts and pages, the categories and tags on them, uploaded files, \
    forms and what has come in through them, and the languages it writes in. A page is one that \
    is not in the feed — an About, a Contact — and posts_create takes kind: \"page\" for it. \
    \
    A site may also publish kinds of its own, each with fields of its own — a course with a \
    price and a level, a property with rooms. content_types_list says which; \
    content_types_create adds one. Reach for that when what somebody wants to publish has \
    facts a page should lay out, rather than writing a post with a heading called \"Price\". \
    \
    It does not hold a footer, a menu, or anything about how the site looks. Those are in the \
    project the site's pages are built from, which is somewhere else and not reachable from \
    here. If you are asked to change one, say where it actually lives rather than looking for a \
    tool for it. \
    \
    There are also things this deliberately cannot do, and no tool for them will appear: making \
    or changing accounts, reading or setting any credential, running or restoring a backup, \
    rewiring how the site is built, and sending a mailing to anybody. Those are administration \
    rather than writing, and a person does them in the panel. If you are asked for one, say so \
    plainly instead of looking for a way round it. \
    \
    Read GET /api/llms.txt on the same address for the shape of the content and how a front \
    end is built from it.";

/// Who is asking, and therefore which set of tools they are offered.
pub enum Caller {
    /// Somebody working on one site, or a build reading it.
    Site {
        state: AppState,
        user: crate::entities::user::Model,
    },
    /// An agency, on the server's own address, asking about its sites.
    Console(crate::console::Operator),
}

async fn caller(
    hosting: &Hosting,
    resolved: &Resolved,
    state: &AppState,
    cookies: &Cookies,
    headers: &HeaderMap,
) -> Result<Caller, Response> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);

    // The server's own address serves the console as well as the installation
    // that lives there, so the credential is what says which of the two this
    // is. A hosted site has no console to be asking about.
    if resolved.is_host()
        && let Ok(registry) = hosting.registry()
        && let Some(operator) =
            console_session(registry.control(), cookies, bearer.as_deref()).await
    {
        return Ok(Caller::Console(operator));
    }

    match crate::auth::authenticate(state, cookies, bearer.as_deref()).await {
        Ok(user) => Ok(Caller::Site {
            state: state.clone(),
            user,
        }),
        // The header is the whole of how a client finds the sign-in flow: it
        // reads the address out of it, fetches the metadata there, registers
        // itself and sends somebody to authorize. Without it a connector can
        // only tell its user that the server said no.
        Err(_) => Err((
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                format!(
                    "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
                    crate::routes::llms::origin(headers)
                ),
            )],
            axum::Json(json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32600,
                    "message": "not signed in: connect through this site, or send a token made in the panel"
                }
            })),
        )
            .into_response()),
    }
}

async fn console_session(
    control: &sea_orm::DatabaseConnection,
    cookies: &Cookies,
    bearer: Option<&str>,
) -> Option<crate::console::Operator> {
    // A token for a program, or the cookie of somebody signed in to the
    // console in a browser. Deliberately different lookups: a session id is
    // not a token, so pasting one into a client's settings does not work and
    // does not have to be reasoned about.
    if let Some(token) = bearer.and_then(|token| uuid::Uuid::parse_str(token).ok())
        && let Ok(found) = crate::console::token_operator(control, token).await
    {
        return found;
    }

    let session = cookies
        .get(crate::console::CONSOLE_COOKIE)
        .and_then(|cookie| uuid::Uuid::parse_str(cookie.value()).ok())?;

    crate::console::session_operator(control, session)
        .await
        .ok()?
}

/// What a tool is, from the outside.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Whether using it changes anything. A credential that may not write is
    /// not shown these at all.
    pub writes: bool,
    /// Whether it destroys something there is no way back from. One tool does.
    /// Clients show this differently, and so does the panel.
    pub destroys: bool,
    pub schema: fn() -> Value,
}

fn listing(caller: &Caller) -> Value {
    let (tools, may_write) = match caller {
        Caller::Site { user, .. } => (site::TOOLS, user.role != crate::auth::BUILDER),
        Caller::Console(_) => (console::TOOLS, true),
    };

    let described: Vec<Value> = tools
        .iter()
        .filter(|tool| may_write || !tool.writes)
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                // The hints a client uses to decide what to confirm with
                // somebody before running. They are hints and not permissions
                // — what a caller may actually do is settled above, by which
                // tools it was offered at all.
                "annotations": {
                    "title": tool.title,
                    "readOnlyHint": !tool.writes,
                    "destructiveHint": tool.destroys,
                    "idempotentHint": !tool.writes || tool.destroys,
                },
            })
        })
        .collect();

    json!({
        "tools": described,
        "ttlMs": TOOLS_TTL_MS,
        "cacheScope": "private",
    })
}

/// `None` when no tool of that name is on offer to this caller.
async fn invoke(
    hosting: &Hosting,
    resolved: &Resolved,
    caller: &Caller,
    name: &str,
    arguments: &Value,
) -> Option<Result<Value, AppError>> {
    match caller {
        Caller::Site { state, user } => {
            let tool = site::TOOLS.iter().find(|tool| tool.name == name)?;
            if tool.writes && user.role == crate::auth::BUILDER {
                // Naming the credential and stopping there is what this used
                // to do, and somebody who had just gone and made one was told
                // what sounded like "you need a token". The way out comes
                // first, and the fact that a token is the cause rather than
                // the cure is said outright.
                return Some(Err(AppError::Forbidden(
                    "this connection can only read, because it was made with a build token, \
                     which is the read-only kind. There are two ways to one that writes: \
                     connect again using the address on its own, with no Authorization \
                     header, and the site will ask you to sign in; or ask whoever the site \
                     belongs to for an assistant key, made in the panel under API, and send \
                     that in the header instead."
                        .to_string(),
                )));
            }
            // Whose hand it was, and that it arrived through an assistant
            // rather than through the panel. Somebody reading the bin wants to
            // know which of the two deleted their post.
            let who = format!("{} (assistant)", user.username);
            Some(site::call(hosting, resolved, state, &who, name, arguments).await)
        }
        Caller::Console(operator) => {
            console::TOOLS.iter().find(|tool| tool.name == name)?;
            Some(console::call(hosting, operator, name, arguments).await)
        }
    }
}

/// A tool that did what was asked.
///
/// The same answer twice: as JSON for a client that reads `structuredContent`,
/// and as text for one that does not.
fn produced(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": false,
    })
}

/// A tool that could not.
fn refused(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

fn result(id: &Value, mut result: Value) -> Response {
    if let Some(object) = result.as_object_mut() {
        object.insert("resultType".to_string(), json!("complete"));
        object
            .entry("_meta")
            .or_insert_with(|| json!({ "io.modelcontextprotocol/serverInfo": server_info() }));
    }

    axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn error(
    status: StatusCode,
    id: Option<&Value>,
    code: i64,
    message: &str,
    data: Option<Value>,
) -> Response {
    let mut body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    if let Some(data) = data {
        body["error"]["data"] = data;
    }

    (status, axum::Json(body)).into_response()
}

/// The GET and DELETE the earlier revisions used. Answered so that a client
/// that tries them is told plainly rather than left to guess from a 404.
pub async fn gone() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST")],
        "this endpoint takes POST; sessions and standalone streams are not part of this protocol revision\n",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, header};
    use serde_json::json;

    use super::{Call, decode_name, headers_disagree, wrong_origin};

    fn call(method: &str, name: Option<&str>) -> Call {
        let params = match name {
            Some(name) => json!({ "name": name }),
            None => json!({}),
        };
        serde_json::from_value(json!({ "id": 1, "method": method, "params": params }))
            .expect("the shape is the one the type describes")
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn the_mirrored_method_has_to_match() {
        let id = json!(1);
        assert!(
            headers_disagree(
                &headers(&[("mcp-method", "tools/list")]),
                &call("tools/list", None),
                &id
            )
            .is_none()
        );
        assert!(
            headers_disagree(
                &headers(&[("mcp-method", "tools/call")]),
                &call("tools/list", None),
                &id
            )
            .is_some()
        );
        assert!(headers_disagree(&headers(&[]), &call("tools/list", None), &id).is_some());
    }

    #[test]
    fn a_call_has_to_mirror_the_tool_name() {
        let id = json!(1);
        let named = call("tools/call", Some("posts_search"));

        assert!(
            headers_disagree(
                &headers(&[("mcp-method", "tools/call"), ("mcp-name", "posts_search")]),
                &named,
                &id
            )
            .is_none()
        );
        assert!(
            headers_disagree(
                &headers(&[("mcp-method", "tools/call"), ("mcp-name", "posts_create")]),
                &named,
                &id
            )
            .is_some()
        );
    }

    #[test]
    fn a_wrapped_name_is_unwrapped_before_it_is_compared() {
        assert_eq!(decode_name("posts_search").as_deref(), Some("posts_search"));
        assert_eq!(
            decode_name("=?base64?cG9zdHNfc2VhcmNo?=").as_deref(),
            Some("posts_search")
        );
        assert_eq!(decode_name("=?base64?not base64?="), None);
    }

    #[test]
    fn a_page_on_another_site_is_refused() {
        assert!(
            wrong_origin(&headers(&[
                (header::ORIGIN.as_str(), "https://example.test"),
                (header::HOST.as_str(), "example.test"),
            ]))
            .is_none()
        );
        assert!(
            wrong_origin(&headers(&[
                (header::ORIGIN.as_str(), "https://somewhere.else"),
                (header::HOST.as_str(), "example.test"),
            ]))
            .is_some()
        );
        // Nothing to check against: a token in a header is what guards this,
        // and a browser always sends an origin.
        assert!(wrong_origin(&headers(&[(header::HOST.as_str(), "example.test")])).is_none());
    }
}
