mod algorithm;
pub(crate) mod predictor;
pub(crate) mod remote;

use crate::api::pb;
use crate::domain::Domain;
use crate::domain::rank::predictor::model_version_label;

impl Domain {
    pub(crate) async fn rank(&self, mut request: pb::RankRequest) -> pb::RankResponse {
        let bucket = algorithm::stable_bucket(&request.user_id);
        // LLM scorer stage (the heavy-ranker slot in the x-algorithm shape):
        // one batched call replaces the candidates' explicit predictions; the
        // per-candidate fusion below keeps its documented precedence. Any
        // trouble — timeout, not-ready, bad payload — falls back to the
        // local heuristic without surfacing an error.
        let mut served_model_version: Option<String> = None;
        if let Some(scorer) = self.scorer.as_ref() {
            match scorer
                .score(&request.user_context, &request.candidates)
                .await
            {
                Ok((predictions, scorer_version)) => {
                    if !scorer_version.is_empty() {
                        // Empty-candidate batches answer Ok with no version;
                        // that is not a served model.
                        served_model_version = Some(scorer_version);
                    }
                    let by_id: std::collections::HashMap<&str, &(f64, f64, f64)> = predictions
                        .iter()
                        .map(|(id, objectives)| (id.as_str(), objectives))
                        .collect();
                    for candidate in &mut request.candidates {
                        if let Some((p_ctr, p_cvr, p_wegu)) = by_id.get(candidate.content_id.as_str())
                        {
                            candidate.p_ctr = *p_ctr;
                            candidate.p_cvr = *p_cvr;
                            candidate.p_wegu = *p_wegu;
                        }
                    }
                }
                Err(error) => {
                    tracing::debug!(%error, "LLM scorer unavailable; heuristic ranking retained");
                }
            }
        }
        // Degradation is an observed fact of THIS response, not a config
        // property: a configured scorer that did not serve it (down,
        // untrained, malformed) means the heuristic shaped the slate.
        let degraded = self.scorer.is_some() && served_model_version.is_none();
        // A served artifact labels itself so every exposure row records which
        // weight file produced the estimates; the heuristic keeps the config
        // version string.
        // Which model actually shaped this response: LLM scorer > artifact >
        // config version. The exposure ledger persists it for regression.
        let serving_version = served_model_version
            .or_else(|| model_version_label(self.predictor.as_ref()))
            .unwrap_or_else(|| self.model.model_version().to_string());
        pb::RankResponse {
            candidates: algorithm::rank(
                request.candidates,
                request.features.as_ref(),
                bucket,
                &serving_version,
                self.predictor.as_ref(),
            ),
            model_version: serving_version.clone(),
            experiment_bucket: format!("{}-{bucket}", serving_version),
            degraded,
        }
    }
}
