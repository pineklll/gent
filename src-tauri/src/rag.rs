use chroma::ChromaHttpClient;
use serde::{Deserialize, Serialize};
use crate::llm::{embed_text, EmbeddingConfig, EmbeddingOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetrievalOutput {
    TextOnly,
    MetadataOnly,
    TextAndMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub text: String,
    pub metadata: serde_json::Value,
    pub score: f32,
}

pub async fn index_documents(
    collection: String,
    documents: Vec<String>,
    group_id: String,
    api_key: String,
) -> Result<u32, String> {
    if documents.is_empty() {
        return Ok(0);
    }

    let config = EmbeddingConfig {
        api_key,
        group_id,
    };

    let output: EmbeddingOutput = embed_text(config, documents.clone(), "db").await;
    if !output.error.is_empty() {
        return Err(format!("embedding error: {}", output.error));
    }

    let client = ChromaHttpClient::new(Default::default());

    let coll = client
        .create_collection(&collection, None, None)
        .await
        .map_err(|e| format!("Chroma create/get collection error: {}", e))?;

    let ids: Vec<String> = (0..documents.len())
        .map(|i| format!("{}-{}", collection, i))
        .collect();

    coll.add(
        ids,
        output.vectors,
        Some(documents.iter().map(|d| Some(d.clone())).collect()),
        None,
        None,
    )
    .await
    .map_err(|e| format!("Chroma add error: {}", e))?;

    Ok(documents.len() as u32)
}

pub async fn retrieve(
    collection: String,
    query: String,
    top_k: usize,
    group_id: String,
    api_key: String,
    output_type: String,
) -> Result<Vec<RetrievalResult>, String> {
    let config = EmbeddingConfig {
        api_key,
        group_id,
    };

    let output: EmbeddingOutput = embed_text(config, vec![query.clone()], "query").await;
    if !output.error.is_empty() {
        return Err(format!("embedding error: {}", output.error));
    }

    let query_vector = output
        .vectors
        .first()
        .ok_or("no embedding returned")?
        .clone();

    let client = ChromaHttpClient::new(Default::default());

    let coll = client
        .get_collection(&collection)
        .await
        .map_err(|e| format!("Chroma get collection error: {}", e))?;

    let results = coll
        .query(vec![query_vector], Some(top_k as u32), None, None, None)
        .await
        .map_err(|e| format!("Chroma query error: {}", e))?;

    let output_enum = match output_type.as_str() {
        "MetadataOnly" => RetrievalOutput::MetadataOnly,
        "TextAndMetadata" => RetrievalOutput::TextAndMetadata,
        _ => RetrievalOutput::TextOnly,
    };

    // Extract the first (and only) query batch from each field
    let ids_vec = results.ids.first();
    let docs_vec = results.documents.as_ref().and_then(|v| v.first());
    let metas_vec = results.metadatas.as_ref().and_then(|v| v.first());
    let dists_vec = results.distances.as_ref().and_then(|v| v.first());

    let ids_vec = match ids_vec {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };
    let docs_vec = match docs_vec {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };
    let metas_vec = match metas_vec {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };
    let dists_vec = match dists_vec {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };

    let n = ids_vec.len().min(docs_vec.len()).min(metas_vec.len()).min(dists_vec.len());

    let mut retrieval_results = Vec::with_capacity(n);
    for i in 0..n {
        let _id_str = match ids_vec.get(i) {
            Some(s) => s.as_str(),
            _ => continue,
        };
        let doc_str = match docs_vec.get(i) {
            Some(Some(s)) => s.as_str(),
            _ => continue,
        };
        let meta_val = match metas_vec.get(i) {
            Some(Some(m)) => serde_json::to_value(m).unwrap_or(serde_json::json!({})),
            _ => continue,
        };
        let dist_val = match dists_vec.get(i) {
            Some(Some(d)) => *d,
            _ => continue,
        };

        let score = 1.0 - (dist_val.clamp(0.0, 2.0) / 2.0);
        let text = match &output_enum {
            RetrievalOutput::TextOnly => doc_str.to_string(),
            RetrievalOutput::MetadataOnly => doc_str.to_string(),
            RetrievalOutput::TextAndMetadata => doc_str.to_string(),
        };
        retrieval_results.push(RetrievalResult {
            text,
            metadata: meta_val,
            score,
        });
    }

    Ok(retrieval_results)
}