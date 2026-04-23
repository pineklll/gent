use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub uri: String,
    pub text: String,
    pub metadata: serde_json::Value,
    pub score: f32,
}

/// Builds a viking:// path for a document within a collection.
/// Example: viking://resources/my_collection/doc_0
fn make_viking_path(collection: &str, doc_id: &str) -> String {
    format!("viking://resources/{}/{}", collection, doc_id)
}

pub async fn index_documents(
    collection: String,
    documents: Vec<String>,
) -> Result<u32, String> {
    if documents.is_empty() {
        return Ok(0);
    }

    for (i, doc) in documents.iter().enumerate() {
        let path = make_viking_path(&collection, &format!("doc_{}", i));
        let output = Command::new("ov")
            .args(["add-resource", &path, "--content", doc])
            .output()
            .await
            .map_err(|e| format!("failed to spawn ov add-resource: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ov add-resource failed: {}", stderr));
        }
    }

    Ok(documents.len() as u32)
}

/// Represents a single result from `ov find`.
/// The output format of `ov find --output json` is assumed to be JSON
/// with a structure like: { "results": [{ "content": "...", "score": 0.95, ... }] }
/// If the actual output differs, adjust the fields accordingly.
#[derive(Debug, Deserialize)]
struct OvFindResult {
    #[serde(rename = "uri", default)]
    uri: Option<String>,
    #[serde(rename = "content", default)]
    content: Option<String>,
    #[serde(rename = "text", default)]
    text: Option<String>,
    #[serde(rename = "score", default)]
    score: Option<f32>,
    #[serde(rename = "metadata", default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OvFindResponse {
    #[serde(rename = "results", default)]
    results: Vec<OvFindResult>,
}

pub async fn retrieve(
    virtual_uri: String,
    query: String,
    top_k: usize,
) -> Result<Vec<RetrievalResult>, String> {
    let output = Command::new("ov")
        .args(["find", &virtual_uri, &query, "--output", "json"])
        .output()
        .await
        .map_err(|e| format!("failed to spawn ov find: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ov find failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output. If ov find defaults to table format, stdout may contain
    // non-JSON preamble. Try to extract JSON from it.
    let response: OvFindResponse = serde_json::from_str(&stdout)
        .or_else(|_| {
            // Fallback: strip any non-JSON prefix/suffix
            let json_start = stdout.find('{').unwrap_or(0);
            let json_end = stdout.rfind('}').map(|i| i + 1).unwrap_or(stdout.len());
            let json_str = &stdout[json_start..json_end];
            serde_json::from_str(json_str)
        })
        .map_err(|e| format!("failed to parse ov find output: {} - stdout: {}", e, stdout))?;

    let results: Vec<RetrievalResult> = response
        .results
        .into_iter()
        .take(top_k)
        .map(|r| {
            RetrievalResult {
                uri: r.uri.unwrap_or_default(),
                text: r.content.or(r.text).unwrap_or_default(),
                metadata: r.metadata.unwrap_or(serde_json::json!({})),
                score: r.score.unwrap_or(0.0),
            }
        })
        .collect();

    Ok(results)
}