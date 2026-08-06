//! Everything the server does, as a library, so that the API and the
//! builder are two front doors onto one codebase rather than two
//! codebases that have to agree.

pub mod auth;
pub mod backup;
pub mod config;
pub mod console;
pub mod crypto;
pub mod db;
pub mod dto;
pub mod email;
pub mod entities;
pub mod error;
pub mod etag;
pub mod fetch;
pub mod languages;
pub mod mailing;
pub mod markdown;
pub mod mcp;
pub mod middleware;
pub mod openapi;
pub mod plugins;
pub mod publish;
pub mod routes;
pub mod schedule;
pub mod slug;
pub mod state;
pub mod storage;
pub mod tenants;
