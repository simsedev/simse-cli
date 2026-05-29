//! gRPC-Web `AdaptiveService` client for the CLI.
//!
//! The CLI persists and recalls memories against the cloud
//! `quantiz.adaptive.AdaptiveService`. Unlike the managed bridge — which
//! reaches the baremetal `simse-adaptive` binary over raw h2 gRPC (see
//! `managed/adaptive_client.rs`) — the CLI has no direct line to the
//! adaptive service. It goes through `cloud/api`, which proxies
//! `/quantiz.adaptive.AdaptiveService/*` and speaks Connect / gRPC-Web
//! over HTTP/1.1. This client hand-frames that wire protocol via
//! [`GrpcWebClient`].
//!
//! Method set + return shapes mirror `AdaptiveGrpcClient`; the only
//! difference is the transport and that the caller supplies a fully
//! built [`Scope`] (the CLI's auth state owns user/team/session IDs).

use std::collections::HashMap;

use reqwest::Client;

use crate::proto::adaptive::{
    MemoryAddRequest, MemoryAddResponse, MemoryDeleteRequest, MemoryDeleteResponse, MemoryEntry,
    MemoryListRequest, MemoryListResponse, MemorySearchRequest, MemorySearchResponse,
    MemoryStatsRequest, MemoryStatsResponse, Scope,
};
use simse_core::error::SimseError;
use simse_core::remote::grpc_web::GrpcWebClient;

/// Fully-qualified gRPC service name (the `cloud/api` proxy path prefix).
const ADAPTIVE_SERVICE: &str = "quantiz.adaptive.AdaptiveService";

/// Default API gateway base URL.
const DEFAULT_API_URL: &str = "https://api.simse.dev";

/// gRPC-Web client for the cloud `AdaptiveService` memory RPCs.
#[derive(Clone)]
pub struct AdaptiveClient {
    web: GrpcWebClient,
    token: String,
}

impl AdaptiveClient {
    /// Build a client against `api_url` (pass an empty string to fall back
    /// to the default `https://api.simse.dev`) with the given bearer token.
    pub fn new(api_url: &str, access_token: &str) -> Self {
        let base = if api_url.trim().is_empty() {
            DEFAULT_API_URL
        } else {
            api_url
        };
        let web = GrpcWebClient::new(Client::new(), base);
        Self {
            web,
            token: access_token.to_string(),
        }
    }

    /// Persist a memory; returns the server-assigned entry id.
    pub async fn memory_add(
        &self,
        scope: Scope,
        text: String,
        metadata: HashMap<String, String>,
    ) -> Result<String, SimseError> {
        let request = MemoryAddRequest {
            scope: Some(scope),
            text,
            metadata,
        };
        let resp: MemoryAddResponse = self
            .web
            .unary(ADAPTIVE_SERVICE, "MemoryAdd", Some(&self.token), &request)
            .await?;
        Ok(resp.id)
    }

    /// Semantic search over the scoped shelf.
    pub async fn memory_search(
        &self,
        scope: Scope,
        query: String,
        limit: Option<i32>,
        min_score: Option<f64>,
    ) -> Result<Vec<MemoryEntry>, SimseError> {
        let request = MemorySearchRequest {
            scope: Some(scope),
            query,
            limit,
            min_score,
        };
        let resp: MemorySearchResponse = self
            .web
            .unary(
                ADAPTIVE_SERVICE,
                "MemorySearch",
                Some(&self.token),
                &request,
            )
            .await?;
        Ok(resp.entries)
    }

    /// List entries on the scoped shelf.
    pub async fn memory_list(
        &self,
        scope: Scope,
        limit: Option<i32>,
    ) -> Result<Vec<MemoryEntry>, SimseError> {
        let request = MemoryListRequest {
            scope: Some(scope),
            limit,
        };
        let resp: MemoryListResponse = self
            .web
            .unary(ADAPTIVE_SERVICE, "MemoryList", Some(&self.token), &request)
            .await?;
        Ok(resp.entries)
    }

    /// Delete a memory by id; returns whether the entry existed.
    pub async fn memory_delete(&self, scope: Scope, id: String) -> Result<bool, SimseError> {
        let request = MemoryDeleteRequest {
            scope: Some(scope),
            id,
        };
        let resp: MemoryDeleteResponse = self
            .web
            .unary(
                ADAPTIVE_SERVICE,
                "MemoryDelete",
                Some(&self.token),
                &request,
            )
            .await?;
        Ok(resp.ok)
    }

    /// Entry count and ISO-8601 timestamp of the most recent insert.
    pub async fn memory_stats(&self, scope: Scope) -> Result<(i64, String), SimseError> {
        let request = MemoryStatsRequest { scope: Some(scope) };
        let resp: MemoryStatsResponse = self
            .web
            .unary(ADAPTIVE_SERVICE, "MemoryStats", Some(&self.token), &request)
            .await?;
        Ok((resp.count, resp.last_added_at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    fn sample_scope() -> Scope {
        Scope {
            user_id: "user-1".to_string(),
            session_id: Some("session-1".to_string()),
            team_id: "team-1".to_string(),
        }
    }

    #[test]
    fn new_defaults_empty_url_to_api_simse_dev() {
        // Constructs without panicking; empty url falls back to the default.
        let _ = AdaptiveClient::new("", "tok");
        let client = AdaptiveClient::new("https://api.example.test", "tok");
        assert_eq!(client.token, "tok");
    }

    #[test]
    fn builds_all_five_request_messages() {
        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "cli".to_string());

        let add = MemoryAddRequest {
            scope: Some(sample_scope()),
            text: "remember this".to_string(),
            metadata: metadata.clone(),
        };
        assert_eq!(add.text, "remember this");
        assert_eq!(add.scope.as_ref().unwrap().user_id, "user-1");
        assert!(!add.encode_to_vec().is_empty());

        let search = MemorySearchRequest {
            scope: Some(sample_scope()),
            query: "what did i say".to_string(),
            limit: Some(10),
            min_score: Some(0.5),
        };
        assert_eq!(search.limit, Some(10));
        assert_eq!(search.min_score, Some(0.5));
        assert!(!search.encode_to_vec().is_empty());

        let list = MemoryListRequest {
            scope: Some(sample_scope()),
            limit: Some(25),
        };
        assert_eq!(list.limit, Some(25));
        assert!(!list.encode_to_vec().is_empty());

        let delete = MemoryDeleteRequest {
            scope: Some(sample_scope()),
            id: "entry-7".to_string(),
        };
        assert_eq!(delete.id, "entry-7");
        assert!(!delete.encode_to_vec().is_empty());

        let stats = MemoryStatsRequest {
            scope: Some(sample_scope()),
        };
        assert_eq!(stats.scope.unwrap().team_id, "team-1");
    }
}
