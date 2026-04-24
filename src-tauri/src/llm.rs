use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;

use crate::config;

fn merge_with_config(mut config: LlmConfig) -> LlmConfig {
    let app_config = config::get_config();

    if config.format.is_empty() {
        if let Some(ref fmt) = app_config.default_format {
            config.format = fmt.clone();
        }
    }

    if let Some(provider) = app_config.providers.get(&config.format) {
        if config.model_name.is_empty() {
            if let Some(ref model) = provider.model {
                config.model_name = model.clone();
            }
        }
        if config.api_key.is_empty() {
            if let Some(ref key) = provider.api_key {
                config.api_key = key.clone();
            }
        }
        if config.custom_url.is_empty() {
            if let Some(ref endpoint) = provider.endpoint {
                config.custom_url = endpoint.clone();
            }
        }
    }

    config
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub format: String,     // "openai" | "anthropic"
    pub model_name: String, // e.g., "gpt-4o-mini" — used directly
    pub api_key: String,
    pub custom_url: String, // custom endpoint base URL (optional, overrides default)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmInput {
    pub prompt: String,
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmOutput {
    pub text: String,
    pub tokens_used: u32,
    pub model: String,
    pub finish_reason: String,
    pub error: String,
}

fn resolve_api_key(config: &LlmConfig) -> Result<String, String> {
    if !config.api_key.is_empty() {
        return Ok(config.api_key.clone());
    }
    let env_key = match config.format.as_str() {
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        _ => return Err("unknown format".to_string()),
    };
    env::var(env_key).map_err(|_| "missing API key".to_string())
}

// OpenAI response shapes
#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    finish_reason: Option<String>, // can be null in edge cases
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>, // can be null when content_filter triggers
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    total_tokens: u32,
}

// Anthropic response shapes
#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String, // "text" or "thinking"
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    model: String,
    content: Vec<AnthropicContentBlock>,
    usage: AnthropicUsage,
    stop_reason: Option<String>, // can be null
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

pub async fn llm_complete(config: LlmConfig, input: LlmInput) -> LlmOutput {
    let config = merge_with_config(config);
    let model = config.model_name.clone();
    let api_key = match resolve_api_key(&config) {
        Ok(k) => k,
        Err(e) => {
            return LlmOutput {
                text: String::new(),
                tokens_used: 0,
                model: String::new(),
                finish_reason: String::new(),
                error: e,
            }
        }
    };

    let client = Client::new();

    match config.format.as_str() {
        "openai" => {
            let url = if !config.custom_url.is_empty() {
                format!(
                    "{}/chat/completions",
                    config.custom_url.trim_end_matches('/')
                )
            } else {
                "https://api.openai.com/v1/chat/completions".to_string()
            };

            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": input.prompt}],
                "temperature": input.temperature
            });

            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let text = response.text().await.unwrap_or_default();
                        return LlmOutput {
                            text: String::new(),
                            tokens_used: 0,
                            model: model.to_string(),
                            finish_reason: String::new(),
                            error: format!("HTTP {}: {}", status, text),
                        };
                    }
                    match response.json::<OpenAiResponse>().await {
                        Ok(data) => LlmOutput {
                            text: data
                                .choices
                                .first()
                                .and_then(|c| c.message.content.clone())
                                .unwrap_or_default(),
                            tokens_used: data.usage.total_tokens,
                            model: data.model,
                            finish_reason: data
                                .choices
                                .first()
                                .and_then(|c| c.finish_reason.clone())
                                .unwrap_or_default(),
                            error: String::new(),
                        },
                        Err(e) => LlmOutput {
                            text: String::new(),
                            tokens_used: 0,
                            model: model.to_string(),
                            finish_reason: String::new(),
                            error: format!("failed to parse response: {}", e),
                        },
                    }
                }
                Err(e) => LlmOutput {
                    text: String::new(),
                    tokens_used: 0,
                    model: model.to_string(),
                    finish_reason: String::new(),
                    error: format!("request failed: {}", e),
                },
            }
        }
        "anthropic" => {
            let url = if !config.custom_url.is_empty() {
                format!("{}/messages", config.custom_url.trim_end_matches('/'))
            } else {
                "https://api.anthropic.com/v1/messages".to_string()
            };
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": input.prompt}],
                "max_tokens": 1024,
                "temperature": input.temperature
            });

            let resp = client
                .post(url)
                // .header("x-api-key", &api_key)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let text = response.text().await.unwrap_or_default();
                        return LlmOutput {
                            text: String::new(),
                            tokens_used: 0,
                            model: model.to_string(),
                            finish_reason: String::new(),
                            error: format!("HTTP {}: {}", status, text),
                        };
                    }
                    match response.json::<AnthropicResponse>().await {
                        Ok(data) => {
                            // Filter to only text blocks, skip thinking blocks
                            let text = data
                                .content
                                .iter()
                                .filter(|b| b.block_type == "text")
                                .filter_map(|b| b.text.clone())
                                .collect::<Vec<_>>()
                                .join("\n");
                            LlmOutput {
                                text,
                                tokens_used: data
                                    .usage
                                    .input_tokens
                                    .saturating_add(data.usage.output_tokens),
                                model: data.model,
                                finish_reason: data.stop_reason.unwrap_or_default(),
                                error: String::new(),
                            }
                        }
                        Err(e) => LlmOutput {
                            text: String::new(),
                            tokens_used: 0,
                            model: model.to_string(),
                            finish_reason: String::new(),
                            error: format!("failed to parse response: {}", e),
                        },
                    }
                }
                Err(e) => LlmOutput {
                    text: String::new(),
                    tokens_used: 0,
                    model: model.to_string(),
                    finish_reason: String::new(),
                    error: format!("request failed: {}", e),
                },
            }
        }
        _ => LlmOutput {
            text: String::new(),
            tokens_used: 0,
            model: String::new(),
            finish_reason: String::new(),
            error: "unknown format".to_string(),
        },
    }
}
