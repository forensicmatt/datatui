//! Embedding Service
//!
//! Provides a blocking interface to generate text embeddings via:
//! - OpenAI Embeddings API (`/v1/embeddings`)
//! - Azure OpenAI Embeddings API
//! - Ollama Embeddings API (`/api/embed`)
//!
//! All calls are synchronous (reqwest blocking). The caller is responsible for
//! running them off the main thread if needed.

use color_eyre::{eyre::eyre, Result};
use serde::{Deserialize, Serialize};

use crate::core::llm_config::{AzureOpenAiConfig, OllamaConfig, OpenAiConfig};

// ── Request / response types ──────────────────────────────────────────────────

/// OpenAI-compatible embeddings request (also used for Azure OpenAI).
#[derive(Debug, Serialize)]
struct OpenAiEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbedResponse {
    data: Vec<OpenAiEmbedData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbedData {
    embedding: Vec<f32>,
    index: usize,
}

/// Ollama embed request (uses `/api/embed` endpoint, batch capable).
#[derive(Debug, Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

// ── Progress callback ─────────────────────────────────────────────────────────

/// Called after each batch completes: `(rows_done, total_rows)`.
pub type ProgressCallback = Box<dyn Fn(usize, usize) + Send + 'static>;

// ── EmbeddingRequest ──────────────────────────────────────────────────────────

/// Parameters for a single embedding job.
#[derive(Debug, Clone)]
pub struct EmbeddingRequest {
    /// Text values to embed (one per row).
    pub texts: Vec<String>,
    /// Model name (e.g. `"text-embedding-3-small"`).
    pub model: String,
    /// Optional dimension override (OpenAI / Azure only).
    pub dimensions: Option<usize>,
    /// How many texts to send per HTTP request.
    pub batch_size: usize,
}

impl EmbeddingRequest {
    pub fn new(texts: Vec<String>, model: impl Into<String>) -> Self {
        Self {
            texts,
            model: model.into(),
            dimensions: None,
            batch_size: 256,
        }
    }

    pub fn with_dimensions(mut self, dims: usize) -> Self {
        self.dimensions = Some(dims);
        self
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }
}

// ── Provider-specific clients ─────────────────────────────────────────────────

/// Generate embeddings using the OpenAI API.
///
/// Returns one `Vec<f32>` per input text, in order.
pub fn embed_openai(
    config: &OpenAiConfig,
    request: &EmbeddingRequest,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<Vec<f32>>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let url = format!("{}/embeddings", config.base_url.trim_end_matches('/'));
    let total = request.texts.len();
    let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(total);

    for (batch_idx, batch) in request.texts.chunks(request.batch_size).enumerate() {
        let body = OpenAiEmbedRequest {
            model: &request.model,
            input: batch,
            dimensions: request.dimensions,
        };

        let response = client
            .post(&url)
            .bearer_auth(&config.api_key)
            .json(&body)
            .send()
            .map_err(|e| eyre!("OpenAI request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(eyre!("OpenAI API error {}: {}", status, text));
        }

        let mut parsed: OpenAiEmbedResponse = response
            .json()
            .map_err(|e| eyre!("Failed to parse OpenAI response: {}", e))?;

        // Sort by index to guarantee order
        parsed.data.sort_by_key(|d| d.index);

        let mut batch_embeddings: Vec<Vec<f32>> =
            parsed.data.into_iter().map(|d| d.embedding).collect();
        all_embeddings.append(&mut batch_embeddings);

        if let Some(cb) = progress {
            let done = (batch_idx + 1) * request.batch_size;
            cb(done.min(total), total);
        }
    }

    Ok(all_embeddings)
}

/// Generate embeddings using Azure OpenAI API.
///
/// Azure requires the model name as part of the URL path (deployment name).
pub fn embed_azure(
    config: &AzureOpenAiConfig,
    request: &EmbeddingRequest,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<Vec<f32>>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let base = config.base_url.trim_end_matches('/');
    let url = format!(
        "{}/openai/deployments/{}/embeddings?api-version={}",
        base, request.model, config.api_version
    );

    let total = request.texts.len();
    let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(total);

    for (batch_idx, batch) in request.texts.chunks(request.batch_size).enumerate() {
        // Azure uses the same request body format as OpenAI
        let body = OpenAiEmbedRequest {
            model: &request.model,
            input: batch,
            dimensions: request.dimensions,
        };

        let response = client
            .post(&url)
            .header("api-key", &config.api_key)
            .json(&body)
            .send()
            .map_err(|e| eyre!("Azure OpenAI request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(eyre!("Azure OpenAI API error {}: {}", status, text));
        }

        let mut parsed: OpenAiEmbedResponse = response
            .json()
            .map_err(|e| eyre!("Failed to parse Azure response: {}", e))?;

        parsed.data.sort_by_key(|d| d.index);

        let mut batch_embeddings: Vec<Vec<f32>> =
            parsed.data.into_iter().map(|d| d.embedding).collect();
        all_embeddings.append(&mut batch_embeddings);

        if let Some(cb) = progress {
            let done = (batch_idx + 1) * request.batch_size;
            cb(done.min(total), total);
        }
    }

    Ok(all_embeddings)
}

/// Generate embeddings using Ollama's `/api/embed` endpoint.
///
/// Ollama supports batch input natively (single round-trip per batch).
pub fn embed_ollama(
    config: &OllamaConfig,
    request: &EmbeddingRequest,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<Vec<f32>>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let url = format!("{}/api/embed", config.host.trim_end_matches('/'));
    let total = request.texts.len();
    let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(total);

    for (batch_idx, batch) in request.texts.chunks(request.batch_size).enumerate() {
        let body = OllamaEmbedRequest {
            model: &request.model,
            input: batch,
        };

        let response = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| eyre!("Ollama request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(eyre!("Ollama API error {}: {}", status, text));
        }

        let parsed: OllamaEmbedResponse = response
            .json()
            .map_err(|e| eyre!("Failed to parse Ollama response: {}", e))?;

        let mut batch_embeddings = parsed.embeddings;
        all_embeddings.append(&mut batch_embeddings);

        if let Some(cb) = progress {
            let done = (batch_idx + 1) * request.batch_size;
            cb(done.min(total), total);
        }
    }

    Ok(all_embeddings)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_request_defaults() {
        let req = EmbeddingRequest::new(vec!["hello world".to_string()], "text-embedding-3-small");
        assert_eq!(req.batch_size, 256);
        assert!(req.dimensions.is_none());
        assert_eq!(req.model, "text-embedding-3-small");
    }

    #[test]
    fn test_embedding_request_builder() {
        let req = EmbeddingRequest::new(vec!["test".to_string()], "model")
            .with_dimensions(512)
            .with_batch_size(50);
        assert_eq!(req.dimensions, Some(512));
        assert_eq!(req.batch_size, 50);
    }
}
