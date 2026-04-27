use crate::llm::{self, LlmConfig, LlmInput};
use serde::{Deserialize, Serialize};
use crate::rag::RetrievalResult;

const OPENVIKING_BASE_URL: &str = "http://127.0.0.1:1933";

// =============================================================================
// Request/Response Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiQueryRequest {
    pub query: String,
    pub num_queries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiQueryResponse {
    pub queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBackRequest {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBackResponse {
    pub abstract_query: String,
    pub original_query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagFusionRequest {
    pub query: String,
    pub virtual_uri: String,
    pub num_queries: usize,
    pub limit_per_query: usize,
    pub k: usize,  // RRF parameter, typically 60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagFusionResponse {
    pub fused_results: Vec<FusedResult>,
    pub individual_results: Vec<PerQueryResults>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerQueryResults {
    pub query: String,
    pub results: Vec<RetrievalResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedResult {
    pub uri: String,
    pub text: String,
    pub score: f32,
    pub source_queries: Vec<String>,
}

// =============================================================================
// Multi-Query: Generate n query variations
// =============================================================================

pub async fn multi_query(
    llm_config: LlmConfig,
    query: String,
    num_queries: usize,
) -> Result<MultiQueryResponse, String> {
    let prompt = format!(
        r#"You are a query augmentation assistant. Given the original query, generate {} different search queries that represent different perspectives or approaches to the same information need.

Original query: {}

Generate {} diverse queries (one per line, no numbering):
"#,
        num_queries, query, num_queries
    );

    let input = LlmInput {
        prompt,
        temperature: 0.7,
    };

    let output = llm::llm_complete(llm_config, input).await;

    if !output.error.is_empty() {
        return Err(output.error);
    }

    let queries: Vec<String> = output
        .text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(num_queries)
        .map(|s| s.trim().to_string())
        .collect();

    Ok(MultiQueryResponse { queries })
}

// =============================================================================
// Step-Back: Generate abstract high-level query
// =============================================================================

pub async fn step_back(
    llm_config: LlmConfig,
    query: String,
) -> Result<StepBackResponse, String> {
    let prompt = format!(
        r#"You are a query abstraction assistant. Given the original query, generate a more abstract high-level query that captures the fundamental concepts and principles, while remaining relevant to the original intent.

Original query: {}

Generate a step-back query that is more general and conceptual:
"#,
        query
    );

    let input = LlmInput {
        prompt,
        temperature: 0.7,
    };

    let output = llm::llm_complete(llm_config, input).await;

    if !output.error.is_empty() {
        return Err(output.error);
    }

    Ok(StepBackResponse {
        abstract_query: output.text.trim().to_string(),
        original_query: query,
    })
}

// =============================================================================
// RAG-Fusion: Reciprocal Rank Fusion combining multiple query results
// =============================================================================

/// Reciprocal Rank Fusion algorithm
/// RRF(score) = 1 / (k + rank), where k is typically 60
fn reciprocal_rank_fusion<'a>(
    results_per_query: &[(&'a str, Vec<RetrievalResult>)],
    k: usize,
) -> Vec<FusedResult> {
    use std::collections::HashMap;

    let mut uri_scores: HashMap<String, (f32, Vec<String>)> = HashMap::new();

    for (query_str, results) in results_per_query {
        for (rank, result) in results.iter().enumerate() {
            let rrf_score = 1.0 / (k as f32 + rank as f32);
            let entry = uri_scores.entry(result.uri.clone()).or_insert((0.0, Vec::new()));
            entry.0 += rrf_score;
            if !entry.1.contains(&query_str.to_string()) {
                entry.1.push(query_str.to_string());
            }
        }
    }

    let mut fused: Vec<FusedResult> = uri_scores
        .into_iter()
        .map(|(uri, (score, source_queries))| {
            // Find text from first occurrence
            let text = results_per_query
                .iter()
                .find_map(|(_, results)| {
                    results.iter().find(|r| r.uri == uri).map(|r| r.text.clone())
                })
                .unwrap_or_default();

            FusedResult {
                uri,
                text,
                score,
                source_queries,
            }
        })
        .collect();

    fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

pub async fn rag_fusion(
    llm_config: LlmConfig,
    query: String,
    virtual_uri: String,
    num_queries: usize,
    limit_per_query: usize,
    k: usize,
) -> Result<RagFusionResponse, String> {
    // Step 1: Generate multiple query variations
    let multi_resp = multi_query(llm_config.clone(), query.clone(), num_queries).await?;
    let queries = multi_resp.queries;

    // Step 2: Execute each query against retrieval
    let mut individual_results: Vec<PerQueryResults> = Vec::new();
    let mut retrieval_inputs: Vec<(&str, Vec<RetrievalResult>)> = Vec::new();

    for q in &queries {
        let results = crate::rag::retrieve(virtual_uri.clone(), q.clone(), limit_per_query).await
            .map_err(|e| format!("retrieval failed for query '{}': {}", q, e))?;
        retrieval_inputs.push((q.as_str(), results.clone()));
        individual_results.push(PerQueryResults {
            query: q.clone(),
            results,
        });
    }

    // Step 3: Apply RRF to fuse results
    let fused_results = reciprocal_rank_fusion(&retrieval_inputs, k);

    Ok(RagFusionResponse {
        fused_results,
        individual_results,
    })
}