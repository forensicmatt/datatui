use std::env;
use serde::{Deserialize, Serialize};
use reqwest::blocking::Client as HttpClient;

#[derive(Debug, Clone)]
pub struct Client {
    pub api_key: String,
}

impl Client {
    pub fn new<S: Into<String>>(api_key: S) -> Self {
        Self { api_key: api_key.into() }
    }

    pub fn from_env() -> Option<Self> {
        match env::var("OPENAI_API_KEY") {
            Ok(key) if !key.trim().is_empty() => Some(Self::new(key)),
            _ => None,
        }
    }

    /// Generate embeddings using OpenAI's embeddings API (text-embedding-3-small by default)
    /// Optionally pass `dimensions` to request a reduced embedding size (model-dependent).
    pub fn generate_embeddings(&self, inputs: &[String], model: Option<&str>, dimensions: Option<usize>) -> anyhow::Result<Vec<Vec<f32>>> {
        if inputs.is_empty() { return Ok(vec![]); }
        let model_name = model.unwrap_or("text-embedding-3-small");

        #[derive(Serialize)]
        struct EmbeddingsRequest<'a> {
            model: &'a str,
            input: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            dimensions: Option<usize>,
        }

        #[derive(Deserialize)]
        struct EmbeddingData { embedding: Vec<f32> }
        #[derive(Deserialize)]
        struct EmbeddingsResponse { data: Vec<EmbeddingData> }

        let http = HttpClient::builder()
            .user_agent("datatui/0.1")
            .build()?;
        let req = EmbeddingsRequest { model: model_name, input: inputs, dimensions };
        let resp: EmbeddingsResponse = http
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()?
            .error_for_status()?
            .json()?;
        if resp.data.len() != inputs.len() {
            anyhow::bail!("OpenAI returned {} embeddings for {} inputs", resp.data.len(), inputs.len());
        }
        Ok(resp.data.into_iter().map(|d| d.embedding).collect())
    }
}


