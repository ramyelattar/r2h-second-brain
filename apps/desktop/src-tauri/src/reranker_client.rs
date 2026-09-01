use serde_json::{Value, json};

use crate::ai_pack::{AiPackRole, R2hAiPackResolver};

const RERANKER_ENDPOINT: &str = "http://127.0.0.1:42113";
const COMPLETION_PATH: &str = "/completion";

const YES_TOKEN_ID: i64 = 9693;
const NO_TOKEN_ID: i64 = 2152;

// The approved Qwen3 checkpoint runs on the bundled CPU Python runtime. Keep
// the HTTP request bound above a cold local inference without permitting an
// unbounded request.
const REQUEST_TIMEOUT_SECONDS: u64 = 180;
const N_PROBS: u32 = 32_000;

#[derive(Clone)]
pub struct RerankerClient {
    client: reqwest::Client,
    endpoint: String,
}

impl RerankerClient {
    pub fn local() -> Result<Self, String> {
        R2hAiPackResolver::from_environment()
            .resolve(AiPackRole::Reranker)
            .map_err(|error| format!("Reranker capability unavailable: {error}"))?;
        Self::new(RERANKER_ENDPOINT)
    }

    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint = endpoint.trim().trim_end_matches('/');

        if endpoint.is_empty() {
            return Err("Reranker endpoint is blank".to_owned());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
            .map_err(|error| format!("Unable to construct reranker client: {error}"))?;

        Ok(Self {
            client,
            endpoint: endpoint.to_owned(),
        })
    }

    pub async fn score(&self, query: &str, document: &str) -> Result<f64, String> {
        let query = query.trim();
        let document = document.trim();

        if query.is_empty() {
            return Err("Reranker query is blank".to_owned());
        }

        if document.is_empty() {
            return Err("Reranker document is blank".to_owned());
        }

        let request = json!({
            "prompt": build_prompt(query, document),
            "n_predict": 1,
            "temperature": 0.0,
            "top_k": 0,
            "top_p": 1.0,
            "min_p": 0.0,
            "n_probs": N_PROBS,
            "cache_prompt": false,
            "stream": false
        });

        let response = self
            .client
            .post(format!("{}{}", self.endpoint, COMPLETION_PATH))
            .json(&request)
            .send()
            .await
            .map_err(|error| format!("Reranker request failed: {error}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("Unable to read reranker response: {error}"))?;

        if !status.is_success() {
            return Err(format!(
                "Reranker returned HTTP {}: {}",
                status.as_u16(),
                truncate_error(&body)
            ));
        }

        let value: Value = serde_json::from_str(&body)
            .map_err(|error| format!("Reranker returned invalid JSON: {error}"))?;

        parse_relevance_score(&value)
    }
}

fn build_prompt(query: &str, document: &str) -> String {
    format!(
        "<|im_start|>system\n\
Judge whether the Document is relevant to the Query. \
Respond with exactly Yes or No.<|im_end|>\n\
<|im_start|>user\n\
Query: {query}\n\
Document: {document}<|im_end|>\n\
<|im_start|>assistant\n\
<think>\n\n</think>\n"
    )
}

fn parse_relevance_score(value: &Value) -> Result<f64, String> {
    let mut yes_logit = None;
    let mut no_logit = None;

    collect_token_scores(value, &mut yes_logit, &mut no_logit);

    let yes_logit = yes_logit
        .ok_or_else(|| "Reranker response omitted the Yes token probability".to_owned())?;

    let no_logit =
        no_logit.ok_or_else(|| "Reranker response omitted the No token probability".to_owned())?;

    softmax_yes_probability(yes_logit, no_logit)
}

fn collect_token_scores(value: &Value, yes_logit: &mut Option<f64>, no_logit: &mut Option<f64>) {
    match value {
        Value::Array(values) => {
            for child in values {
                collect_token_scores(child, yes_logit, no_logit);
            }
        }
        Value::Object(object) => {
            let token_id = numeric_field(object, &["id", "token_id", "token"]);
            let token_text = text_field(object, &["tok_str", "token_str", "content", "text"]);
            let score = token_score(object);

            if let Some(score) = score {
                let is_yes = token_id == Some(YES_TOKEN_ID)
                    || token_text
                        .as_deref()
                        .is_some_and(|token| normalized_token(token) == "yes");

                let is_no = token_id == Some(NO_TOKEN_ID)
                    || token_text
                        .as_deref()
                        .is_some_and(|token| normalized_token(token) == "no");

                if is_yes {
                    retain_greater(yes_logit, score);
                }

                if is_no {
                    retain_greater(no_logit, score);
                }
            }

            for child in object.values() {
                collect_token_scores(child, yes_logit, no_logit);
            }
        }
        _ => {}
    }
}

fn numeric_field(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| {
        object.get(*name).and_then(|value| {
            value.as_i64().or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.trim().parse::<i64>().ok())
            })
        })
    })
}

fn text_field(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        object
            .get(*name)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn token_score(object: &serde_json::Map<String, Value>) -> Option<f64> {
    if let Some(logprob) = object.get("logprob").and_then(Value::as_f64) {
        return logprob.is_finite().then_some(logprob);
    }

    if let Some(probability) = object
        .get("prob")
        .or_else(|| object.get("probability"))
        .and_then(Value::as_f64)
        && probability.is_finite()
        && probability > 0.0
    {
        return Some(probability.ln());
    }

    None
}

fn normalized_token(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '"' | '\'' | '.' | ',' | ':' | ';' | '!' | '?' | '\n' | '\r'
                )
        })
        .to_ascii_lowercase()
}

fn retain_greater(target: &mut Option<f64>, candidate: f64) {
    if !candidate.is_finite() {
        return;
    }

    match target {
        Some(current) if *current >= candidate => {}
        _ => *target = Some(candidate),
    }
}

fn softmax_yes_probability(yes_logit: f64, no_logit: f64) -> Result<f64, String> {
    if !yes_logit.is_finite() || !no_logit.is_finite() {
        return Err("Reranker token scores are non-finite".to_owned());
    }

    let maximum = yes_logit.max(no_logit);
    let yes = (yes_logit - maximum).exp();
    let no = (no_logit - maximum).exp();
    let denominator = yes + no;

    if !denominator.is_finite() || denominator <= f64::EPSILON {
        return Err("Reranker token scores cannot be normalized".to_owned());
    }

    Ok((yes / denominator).clamp(0.0, 1.0))
}

fn truncate_error(value: &str) -> String {
    const LIMIT: usize = 300;

    value.chars().take(LIMIT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_ids_with_logprobs() -> Result<(), String> {
        let response = json!({
            "completion_probabilities": [{
                "probs": [
                    {
                        "id": YES_TOKEN_ID,
                        "tok_str": "Yes",
                        "logprob": -0.1
                    },
                    {
                        "id": NO_TOKEN_ID,
                        "tok_str": "No",
                        "logprob": -2.0
                    }
                ]
            }]
        });

        let score = parse_relevance_score(&response)?;

        assert!(score > 0.85);
        assert!(score <= 1.0);

        Ok(())
    }

    #[test]
    fn parses_token_strings_with_probabilities() -> Result<(), String> {
        let response = json!({
            "completion_probabilities": [{
                "probs": [
                    {
                        "tok_str": " Yes",
                        "prob": 0.8
                    },
                    {
                        "tok_str": " No",
                        "prob": 0.2
                    }
                ]
            }]
        });

        let score = parse_relevance_score(&response)?;

        assert!((score - 0.8).abs() < 1e-12);

        Ok(())
    }

    #[test]
    fn rejects_missing_binary_token_pair() {
        let response = json!({
            "completion_probabilities": [{
                "probs": [{
                    "tok_str": "Maybe",
                    "prob": 1.0
                }]
            }]
        });

        assert!(parse_relevance_score(&response).is_err());
    }

    #[test]
    fn score_is_deterministic_and_bounded() -> Result<(), String> {
        let first = softmax_yes_probability(-0.25, -1.25)?;
        let second = softmax_yes_probability(-0.25, -1.25)?;

        assert!((first - second).abs() <= f64::EPSILON);
        assert!((0.0..=1.0).contains(&first));

        Ok(())
    }

    #[test]
    fn prompt_preserves_query_and_document() {
        let prompt = build_prompt("PURPLE-CIRCUIT-9274", "verification phrase");

        assert!(prompt.contains("PURPLE-CIRCUIT-9274"));
        assert!(prompt.contains("verification phrase"));
        assert!(prompt.ends_with("<think>\n\n</think>\n"));
    }
}
