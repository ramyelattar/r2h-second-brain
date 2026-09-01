use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::ai_pack::{AiPackRole, R2hAiPackResolver};

const EMBEDDING_BASE_URL: &str = "http://127.0.0.1:42112";
pub(crate) const EMBEDDING_MODEL_ID: &str = "qwen3-embedding-0.6b-gguf";
pub(crate) const EMBEDDING_MODEL_REVISION: &str =
    "sha256:17c3e3f2eaabc6e321702b4a13680d042e72afc5d602f359f27a670c3e54718c";
const EMBEDDING_DIMENSIONS: usize = 1_024;

const FTS_WEIGHT: f64 = 0.35;
const VECTOR_WEIGHT: f64 = 0.65;

#[derive(Clone)]
pub struct EmbeddingClient {
    client: Client,
    endpoint: String,
    model_id: String,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
    encoding_format: &'static str,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f64>,
}

impl EmbeddingClient {
    pub fn local() -> Result<Self, String> {
        R2hAiPackResolver::from_environment()
            .resolve(AiPackRole::Embedding)
            .map_err(|error| format!("Embedding capability unavailable: {error}"))?;

        Self::with_endpoint(format!("{EMBEDDING_BASE_URL}/v1/embeddings"))
    }

    fn with_endpoint(endpoint: String) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .map_err(|error| format!("Unable to initialize local embedding client: {error}"))?;

        Ok(Self {
            client,
            endpoint,
            model_id: EMBEDDING_MODEL_ID.to_owned(),
        })
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f64>, String> {
        let input = text.trim();

        if input.is_empty() {
            return Err("Embedding input cannot be empty".to_owned());
        }

        let response = self
            .client
            .post(&self.endpoint)
            .json(&EmbeddingRequest {
                model: &self.model_id,
                input,
                encoding_format: "float",
            })
            .send()
            .await
            .map_err(|error| format!("Local embedding request failed: {error}"))?;

        let status = response.status();
        let payload = response
            .text()
            .await
            .map_err(|error| format!("Unable to read local embedding response: {error}"))?;

        if !status.is_success() {
            return Err(format!(
                "Local embedding runtime returned HTTP {status}: {payload}"
            ));
        }

        parse_embedding_response(&payload)
    }
}

fn parse_embedding_response(payload: &str) -> Result<Vec<f64>, String> {
    let response: EmbeddingResponse = serde_json::from_str(payload)
        .map_err(|error| format!("Invalid local embedding response: {error}"))?;

    if response.data.len() != 1 {
        return Err(format!(
            "Local embedding response contained {} vectors; expected exactly 1",
            response.data.len()
        ));
    }

    let embedding = response
        .data
        .into_iter()
        .next()
        .ok_or_else(|| "Local embedding response did not contain a vector".to_owned())?
        .embedding;

    validate_embedding(&embedding)?;

    Ok(embedding)
}

fn validate_embedding(embedding: &[f64]) -> Result<(), String> {
    if embedding.len() != EMBEDDING_DIMENSIONS {
        return Err(format!(
            "Local embedding dimension mismatch: expected \
             {EMBEDDING_DIMENSIONS}, received {}",
            embedding.len()
        ));
    }

    if embedding.iter().any(|value| !value.is_finite()) {
        return Err("Local embedding contains a non-finite value".to_owned());
    }

    let magnitude_squared = embedding.iter().map(|value| value * value).sum::<f64>();

    if !magnitude_squared.is_finite() || magnitude_squared <= f64::EPSILON {
        return Err("Local embedding vector has zero or invalid magnitude".to_owned());
    }

    Ok(())
}

pub fn cosine_similarity(left: &[f64], right: &[f64]) -> Result<f64, String> {
    if left.len() != right.len() {
        return Err(format!(
            "Embedding dimension mismatch: left={}, right={}",
            left.len(),
            right.len()
        ));
    }

    if left.is_empty() {
        return Err("Embedding vectors cannot be empty".to_owned());
    }

    if left
        .iter()
        .chain(right.iter())
        .any(|value| !value.is_finite())
    {
        return Err("Embedding vectors contain non-finite values".to_owned());
    }

    let dot = left
        .iter()
        .zip(right.iter())
        .map(|(left_value, right_value)| left_value * right_value)
        .sum::<f64>();

    let left_norm = left.iter().map(|value| value * value).sum::<f64>().sqrt();

    let right_norm = right.iter().map(|value| value * value).sum::<f64>().sqrt();

    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return Err("Embedding vectors must have non-zero magnitude".to_owned());
    }

    let similarity = dot / (left_norm * right_norm);

    if !similarity.is_finite() {
        return Err("Embedding cosine similarity is not finite".to_owned());
    }

    Ok(similarity.clamp(-1.0, 1.0))
}

#[must_use]
pub fn hybrid_score(fts_score: f64, cosine_score: f64) -> f64 {
    let normalized_fts = fts_score.clamp(0.0, 1.0);
    let normalized_vector = ((cosine_score.clamp(-1.0, 1.0)) + 1.0) / 2.0;

    (FTS_WEIGHT * normalized_fts + VECTOR_WEIGHT * normalized_vector).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_vector(first: f64, second: f64) -> Vec<f64> {
        let mut values = vec![0.0; EMBEDDING_DIMENSIONS];
        values[0] = first;
        values[1] = second;
        values
    }

    #[test]
    fn local_client_uses_loopback_embedding_endpoint() -> Result<(), String> {
        let client =
            EmbeddingClient::with_endpoint("http://127.0.0.1:42112/v1/embeddings".to_owned())?;

        assert_eq!(client.endpoint, "http://127.0.0.1:42112/v1/embeddings");
        assert!(!client.endpoint.contains("0.0.0.0"));

        Ok(())
    }

    #[test]
    fn test_client_can_override_endpoint() -> Result<(), String> {
        let client =
            EmbeddingClient::with_endpoint("http://127.0.0.1:49999/v1/embeddings".to_owned())?;

        assert_eq!(client.endpoint, "http://127.0.0.1:49999/v1/embeddings");

        Ok(())
    }

    #[test]
    fn parses_one_valid_embedding() -> Result<(), String> {
        let values = unit_vector(1.0, 0.0);
        let payload = serde_json::json!({
            "data": [
                {
                    "embedding": values,
                    "index": 0
                }
            ],
            "model": EMBEDDING_MODEL_ID
        })
        .to_string();

        let parsed = parse_embedding_response(&payload)?;

        assert_eq!(parsed.len(), EMBEDDING_DIMENSIONS);
        assert!((parsed[0] - 1.0).abs() <= f64::EPSILON);

        Ok(())
    }

    #[test]
    fn rejects_wrong_embedding_dimension() {
        let payload = serde_json::json!({
            "data": [
                {
                    "embedding": [1.0, 0.0],
                    "index": 0
                }
            ]
        })
        .to_string();

        let result = parse_embedding_response(&payload);

        assert!(matches!(
            result,
            Err(ref error) if error.contains("dimension mismatch")
        ));
    }

    #[test]
    fn rejects_multiple_embedding_vectors() {
        let payload = serde_json::json!({
            "data": [
                {
                    "embedding": unit_vector(1.0, 0.0)
                },
                {
                    "embedding": unit_vector(0.0, 1.0)
                }
            ]
        })
        .to_string();

        let result = parse_embedding_response(&payload);

        assert!(matches!(
            result,
            Err(ref error) if error.contains("expected exactly 1")
        ));
    }

    #[test]
    fn rejects_zero_magnitude_embedding() {
        let result = validate_embedding(&vec![0.0; EMBEDDING_DIMENSIONS]);

        assert!(matches!(
            result,
            Err(ref error) if error.contains("zero or invalid magnitude")
        ));
    }

    #[test]
    fn cosine_similarity_handles_identical_vectors() -> Result<(), String> {
        let vector = unit_vector(1.0, 0.0);
        let score = cosine_similarity(&vector, &vector)?;

        assert!((score - 1.0).abs() < 1e-12);

        Ok(())
    }

    #[test]
    fn cosine_similarity_handles_orthogonal_vectors() -> Result<(), String> {
        let left = unit_vector(1.0, 0.0);
        let right = unit_vector(0.0, 1.0);
        let score = cosine_similarity(&left, &right)?;

        assert!(score.abs() < 1e-12);

        Ok(())
    }

    #[test]
    fn cosine_similarity_rejects_dimension_mismatch() {
        let result = cosine_similarity(&[1.0], &[1.0, 0.0]);

        assert!(matches!(
            result,
            Err(ref error) if error.contains("dimension mismatch")
        ));
    }

    #[test]
    fn hybrid_score_is_deterministic_and_bounded() {
        let score = hybrid_score(0.8, 0.6);
        let repeated = hybrid_score(0.8, 0.6);

        assert!((score - repeated).abs() <= f64::EPSILON);
        assert!((0.0..=1.0).contains(&score));
        assert!((score - 0.8).abs() < 1e-12);
    }
}
