use serde::{Deserialize, Serialize};

const OPENVIKING_BASE_URL: &str = "http://127.0.0.1:1933";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub uri: String,
    pub text: String,
    pub metadata: serde_json::Value,
    pub score: f32,
}

#[derive(Debug, Serialize)]
struct FindRequest {
    query: String,
    #[serde(rename = "target_uri")]
    target_uri: String,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct OvFindResponse {
    status: String,
    result: OvFindResultWrapper,
}

#[derive(Debug, Deserialize)]
struct OvFindResultWrapper {
    #[serde(rename = "resources", default)]
    resources: Vec<OvFindResult>,
}

#[derive(Debug, Deserialize)]
struct OvFindResult {
    #[serde(rename = "uri", default)]
    uri: Option<String>,
    #[serde(rename = "abstract", default)]
    abstract_content: Option<String>,
    #[serde(rename = "score", default)]
    score: Option<f32>,
}

pub async fn retrieve(
    virtual_uri: String,
    query: String,
    top_k: usize,
) -> Result<Vec<RetrievalResult>, String> {
    let client = reqwest::Client::new();
    let request = FindRequest {
        query,
        target_uri: virtual_uri,
        limit: top_k,
    };

    let response = client
        .post(&format!("{}/api/v1/search/find", OPENVIKING_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("failed to send find request: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("find request failed: {} - {}", status, text));
    }

    let response: OvFindResponse = response
        .json()
        .await
        .map_err(|e| format!("failed to parse find response: {}", e))?;

    if response.status != "ok" {
        return Err(format!("find request returned error status: {}", response.status));
    }

    let results: Vec<RetrievalResult> = response
        .result
        .resources
        .into_iter()
        .take(top_k)
        .map(|r| {
            RetrievalResult {
                uri: r.uri.unwrap_or_default(),
                text: r.abstract_content.unwrap_or_default(),
                metadata: serde_json::json!({}),
                score: r.score.unwrap_or(0.0),
            }
        })
        .collect();

    Ok(results)
}