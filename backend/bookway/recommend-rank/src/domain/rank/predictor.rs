use std::sync::Arc;

/// The three conversion objectives 万卷行 ranks for. WEGU (行动转化率) dominates
/// because a verified completed action is the product's north star; clicks and
/// purchases are modeled separately so commerce never silently hijacks ranking.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Prediction {
    pub p_ctr: f64,
    pub p_cvr: f64,
    pub p_wegu: f64,
}

/// Raw per-candidate evidence available to any predictor, extracted once by the
/// ranking stage (which owns request-level feature lookup) so predictor
/// implementations stay free of pb layout details.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ObjectiveEvidence {
    /// Explicit upstream prediction; 0 means absent.
    pub explicit_ctr: f64,
    pub explicit_cvr: f64,
    pub explicit_wegu: f64,
    /// Server-verified behavior rates computed by feature-main.
    pub observed_ctr: f64,
    pub observed_cvr: f64,
    pub observed_wegu: f64,
}

fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

pub(crate) trait MultiObjectivePredictor: Send + Sync {
    /// Predicts the three objectives for one candidate. Implementations must be
    /// pure and allocation-light: this runs per candidate inside the P99 budget.
    fn predict(&self, evidence: &ObjectiveEvidence) -> Prediction;

    /// True when this predictor is a fallback (e.g. the remote model endpoint
    /// was unreachable and heuristics served instead).
    fn degraded(&self) -> bool {
        false
    }
}

/// Deterministic heuristic used before a trained model exists and whenever the
/// remote endpoint is unavailable. Explicit predictions win; otherwise the
/// observed server-verified rate stands in, shrunk toward an objective-specific
/// prior so a tiny observation window cannot spike the whole slate.
#[derive(Debug, Default)]
pub(crate) struct HeuristicPredictor;

const SMOOTHING_STRENGTH: f64 = 20.0;

impl MultiObjectivePredictor for HeuristicPredictor {
    fn predict(&self, evidence: &ObjectiveEvidence) -> Prediction {
        Prediction {
            p_ctr: predicted(evidence.explicit_ctr, evidence.observed_ctr, 0.03),
            p_cvr: predicted(evidence.explicit_cvr, evidence.observed_cvr, 0.01),
            p_wegu: predicted(evidence.explicit_wegu, evidence.observed_wegu, 0.08),
        }
    }
}

fn predicted(explicit: f64, observed_rate: f64, prior_mean: f64) -> f64 {
    let explicit = finite(explicit);
    if explicit > 0.0 {
        return explicit.clamp(0.0001, 1.0);
    }
    // Without per-candidate observation counts on the wire we shrink toward the
    // objective prior with fixed strength; counts plumbing upgrades this to an
    // exact Beta posterior without changing any caller.
    let weight = SMOOTHING_STRENGTH / (SMOOTHING_STRENGTH + 1.0);
    (weight * clamp01(observed_rate) + (1.0 - weight) * prior_mean).clamp(0.0001, 1.0)
}

fn clamp01(value: f64) -> f64 {
    finite(value).clamp(0.0, 1.0)
}

/// Reserved wiring for the standalone model-serving deployment. Until that
/// contract ships, construction succeeds but every call defers to the
/// heuristic and reports degradation — no fake RPC traffic is emitted.
#[derive(Clone, Debug)]
pub(crate) struct RemoteModelPredictor {
    endpoint: String,
    heuristic: Arc<HeuristicPredictor>,
}

impl RemoteModelPredictor {
    pub(crate) fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            heuristic: Arc::new(HeuristicPredictor),
        }
    }

    /// `Some(prediction)` once the remote contract goes live; currently always
    /// `None`, documented as the single switch point for that launch.
    fn try_remote(&self, _evidence: &ObjectiveEvidence) -> Option<Prediction> {
        tracing::debug!(endpoint = %self.endpoint, "model-serving contract not deployed; using heuristic");
        None
    }
}

impl MultiObjectivePredictor for RemoteModelPredictor {
    fn predict(&self, evidence: &ObjectiveEvidence) -> Prediction {
        match self.try_remote(evidence) {
            Some(prediction) => prediction,
            None => self.heuristic.predict(evidence),
        }
    }

    fn degraded(&self) -> bool {
        true
    }
}

pub(crate) fn choose_predictor(model_endpoint: Option<&str>) -> Box<dyn MultiObjectivePredictor> {
    match model_endpoint.filter(|endpoint| !endpoint.trim().is_empty()) {
        Some(endpoint) => Box::new(RemoteModelPredictor::new(endpoint.to_string())),
        None => Box::new(HeuristicPredictor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_prediction_overrides_observed_rates() {
        let predictor = HeuristicPredictor;
        let evidence = ObjectiveEvidence {
            explicit_ctr: 0.9,
            explicit_cvr: 0.0,
            explicit_wegu: 0.0,
            observed_ctr: 0.1,
            observed_cvr: 0.6,
            observed_wegu: 0.1,
        };
        let prediction = predictor.predict(&evidence);
        assert_eq!(prediction.p_ctr, 0.9);
        assert!(prediction.p_cvr < 0.6); // shrunk toward the 0.01 purchase prior
        // Shrunk toward the 0.08 WEGU prior yet still pulled up by the 0.1 rate.
        assert!(prediction.p_wegu > 0.08 && prediction.p_wegu < 0.1);
    }

    #[test]
    fn non_finite_inputs_never_poison_predictions() {
        let predictor = HeuristicPredictor;
        let evidence = ObjectiveEvidence {
            explicit_ctr: f64::NAN,
            explicit_cvr: f64::INFINITY,
            explicit_wegu: f64::NEG_INFINITY,
            observed_ctr: f64::NAN,
            observed_cvr: 0.5,
            observed_wegu: -3.0,
        };
        let prediction = predictor.predict(&evidence);
        assert!(prediction.p_ctr.is_finite() && prediction.p_ctr > 0.0);
        assert!(prediction.p_cvr.is_finite());
        assert!(prediction.p_wegu.is_finite());
    }

    #[test]
    fn empty_endpoint_selects_heuristic_without_degradation() {
        assert!(!choose_predictor(None).degraded());
        assert!(!choose_predictor(Some("")).degraded());
        assert!(choose_predictor(Some("http://127.0.0.1:9099")).degraded());
    }
}
