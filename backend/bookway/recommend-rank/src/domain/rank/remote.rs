//! HTTP client for the MiniCPM model-serving `/score` endpoint.
//!
//! x-algorithm puts model inference behind a dedicated service; this client
//! is the recommend-rank side of that contract. The service reports
//! `ready: false` until a trained checkpoint is deployed — that response is
//! treated exactly like a timeout: heuristic ranking, degraded=false noise
//! avoided by logging at debug only.
//!
//! Served scores land on the candidates' explicit prediction fields, which
//! the fusion step already treats as authoritative (see predictor.rs: an
//! explicit value always wins). If the model service is down, nothing here
//! fails the request.

use std::time::Duration;

use bookway_recommend_recall_api::pb::Candidate;

const SCORE_TIMEOUT: Duration = Duration::from_millis(150);
const MAX_PROMPT_CHARS: usize = 512;

#[derive(Clone)]
pub(crate) struct RemoteScorer {
    endpoint: String,
    client: reqwest::Client,
}

impl RemoteScorer {
    pub(crate) fn new(endpoint: String, client: reqwest::Client) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// One batched call for the whole slate. Returns per-content-id
    /// (p_ctr, p_cvr, p_wegu).
    pub(crate) async fn score(
        &self,
        user_context: &str,
        candidates: &[bookway_recommend_recall_api::pb::Candidate],
    ) -> Result<(Vec<(String, (f64, f64, f64))>, String), String> {
        if candidates.is_empty() {
            return Ok((Vec::new(), String::new()));
        }
        let items: Vec<serde_json::Value> = candidates
            .iter()
            .map(|candidate| {
                serde_json::json!({
                    "content_id": candidate.content_id,
                    "user_context": truncate(user_context),
                    "candidate_text": candidate_text(candidate),
                })
            })
            .collect();
        let response = self
            .client
            .post(format!("{}/score", self.endpoint))
            .json(&serde_json::json!({ "items": items }))
            .timeout(SCORE_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("model serving unreachable: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("model serving returned {}", response.status()));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|error| format!("model serving payload invalid: {error}"))?;
        if !payload
            .get("ready")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return Err("scorer checkpoint not deployed yet".to_string());
        }
        let served_version = payload
            .get("model_version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("minicpm-unknown")
            .to_string();
        let rows = payload
            .get("scores")
            .and_then(serde_json::Value::as_array)
            .ok_or("model serving response missing scores")?;
        // Each row must name the content it scored. Matching by array position
        // would attach one candidate's pWEGU to another the moment the batch
        // comes back reordered or short, and a short response would be
        // truncated silently while still reporting a served model version.
        let requested = candidates
            .iter()
            .map(|candidate| candidate.content_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut returned = std::collections::HashSet::new();
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let objectives = row.as_object().ok_or("score row is not an object")?;
            let content_id = objectives
                .get("content_id")
                .and_then(serde_json::Value::as_str)
                .ok_or("score row missing content_id")?;
            validate_score_id(content_id, &requested, &mut returned)?;
            let get = |key: &str| {
                objectives
                    .get(key)
                    .and_then(serde_json::Value::as_f64)
                    .filter(|value| value.is_finite())
                    .map(|value| value.clamp(0.0, 1.0))
                    .ok_or_else(|| format!("score row missing {key}"))
            };
            out.push((
                content_id.to_string(),
                (get("p_ctr")?, get("p_cvr")?, get("p_wegu")?),
            ));
        }
        Ok((out, served_version))
    }
}

fn validate_score_id(
    content_id: &str,
    requested: &std::collections::HashSet<&str>,
    returned: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    if content_id.trim().is_empty() {
        return Err("score row has an empty content_id".to_string());
    }
    if !requested.contains(content_id) {
        return Err(format!(
            "score row names an unrequested candidate {content_id}"
        ));
    }
    if !returned.insert(content_id.to_string()) {
        return Err(format!(
            "score response contains duplicate candidate {content_id}"
        ));
    }
    Ok(())
}

fn truncate(value: &str) -> String {
    value.chars().take(MAX_PROMPT_CHARS).collect()
}

fn candidate_text(candidate: &Candidate) -> String {
    let Some(post) = candidate.post.as_ref() else {
        return String::new();
    };
    truncate(&format!("{}。{}", post.title, post.summary))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::validate_score_id;

    #[test]
    fn score_response_rejects_duplicate_candidate_rows() {
        let requested = HashSet::from(["candidate-1"]);
        let mut returned = HashSet::new();
        validate_score_id("candidate-1", &requested, &mut returned).expect("first row");
        let error = validate_score_id("candidate-1", &requested, &mut returned)
            .expect_err("duplicate rows must invalidate the response");
        assert!(error.contains("duplicate"));
    }

    #[test]
    fn score_response_rejects_empty_and_unknown_candidate_ids() {
        let requested = HashSet::from(["candidate-1"]);
        let mut returned = HashSet::new();
        assert!(validate_score_id("", &requested, &mut returned).is_err());
        assert!(validate_score_id("candidate-2", &requested, &mut returned).is_err());
    }
}
