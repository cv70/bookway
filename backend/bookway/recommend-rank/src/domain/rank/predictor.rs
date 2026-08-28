use std::collections::HashMap;

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
    /// Window/population facts from feature-main that a learned model may
    /// weigh; the heuristic ignores them.
    pub route_completion: f64,
    pub domain_affinity: f64,
    pub author_affinity: f64,
    pub impression_fatigue: f64,
    pub direct_negative_feedback: f64,
}

fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

pub(crate) trait MultiObjectivePredictor: Send + Sync {
    /// Predicts the three objectives for one candidate. Implementations must be
    /// pure and allocation-light: this runs per candidate inside the P99 budget.
    fn predict(&self, evidence: &ObjectiveEvidence) -> Prediction;

    /// Downcast hook so the rank stage can label its responses with the
    /// served artifact version.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Deterministic heuristic used before a trained model exists and whenever the
/// remote endpoint is unavailable. Explicit predictions win; otherwise the
/// observed server-verified rate stands in, shrunk toward an objective-specific
/// prior so a tiny observation window cannot spike the whole slate.
#[derive(Debug, Default)]
pub(crate) struct HeuristicPredictor;

const SMOOTHING_STRENGTH: f64 = 20.0;

impl MultiObjectivePredictor for HeuristicPredictor {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

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

/// Trained-artifact contract: a versioned logistic head over a fixed,
/// explicitly named feature set. Weights ship with the feature snapshot they
/// were fit on; unknown feature names are rejected at load so a typo can
/// never silently zero a weight.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ModelArtifact {
    pub version: String,
    pub bias: ModelBias,
    pub weights: ModelHead,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ModelBias {
    pub ctr: f64,
    pub cvr: f64,
    pub wegu: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ModelHead {
    pub ctr: HashMap<String, f64>,
    pub cvr: HashMap<String, f64>,
    pub wegu: HashMap<String, f64>,
}

/// Serving container for an offline-trained logistic model over the ranking
/// features. Construction is fail-fast (bad artifacts refuse to boot); the
/// per-candidate pass is allocation-light and bounded to (0, 1) by the
/// sigmoid. Explicit upstream predictions still win, matching the heuristic
/// contract.
#[derive(Debug, Clone)]
pub(crate) struct LinearPredictor {
    artifact: ModelArtifact,
}

impl LinearPredictor {
    pub(crate) fn load(path: &std::path::Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|error| format!("model artifact {}: {error}", path.display()))?;
        let artifact: ModelArtifact = serde_json::from_str(&raw)
            .map_err(|error| format!("model artifact {} is invalid: {error}", path.display()))?;
        if artifact.version.trim().is_empty() {
            return Err(format!("model artifact {} has no version", path.display()));
        }
        let evidence_keys = [
            "explicit_ctr",
            "observed_ctr",
            "observed_cvr",
            "observed_wegu",
            "route_completion",
            "domain_affinity",
            "author_affinity",
            "impression_fatigue",
            "direct_negative_feedback",
        ];
        for (head_name, head) in [
            ("ctr", &artifact.weights.ctr),
            ("cvr", &artifact.weights.cvr),
            ("wegu", &artifact.weights.wegu),
        ] {
            for name in head.keys() {
                if !evidence_keys.contains(&name.as_str()) {
                    return Err(format!(
                        "model artifact {} references unknown feature '{name}' in {head_name} head",
                        path.display()
                    ));
                }
            }
        }
        Ok(Self { artifact })
    }

    fn head(&self, bias: f64, head: &HashMap<String, f64>, evidence: &ObjectiveEvidence) -> f64 {
        let values = [
            ("explicit_ctr", evidence.explicit_ctr),
            ("observed_ctr", evidence.observed_ctr),
            ("observed_cvr", evidence.observed_cvr),
            ("observed_wegu", evidence.observed_wegu),
            ("route_completion", evidence.route_completion),
            ("domain_affinity", evidence.domain_affinity),
            ("author_affinity", evidence.author_affinity),
            ("impression_fatigue", evidence.impression_fatigue),
            ("direct_negative_feedback", evidence.direct_negative_feedback),
        ];
        let mut z = bias;
        for (name, coefficient) in head {
            let value = values
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| finite(*value))
                .unwrap_or_default();
            z += coefficient * value;
        }
        sigmoid(z)
    }

    fn version(&self) -> &str {
        &self.artifact.version
    }
}

fn sigmoid(z: f64) -> f64 {
    // 1/(1+e^-z) written overflow-safe for large |z|.
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

impl MultiObjectivePredictor for LinearPredictor {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn predict(&self, evidence: &ObjectiveEvidence) -> Prediction {
        let explicit = Prediction {
            p_ctr: evidence.explicit_ctr,
            p_cvr: evidence.explicit_cvr,
            p_wegu: evidence.explicit_wegu,
        };
        let modeled = Prediction {
            p_ctr: self.head(
                self.artifact.bias.ctr,
                &self.artifact.weights.ctr,
                evidence,
            ),
            p_cvr: self.head(
                self.artifact.bias.cvr,
                &self.artifact.weights.cvr,
                evidence,
            ),
            p_wegu: self.head(
                self.artifact.bias.wegu,
                &self.artifact.weights.wegu,
                evidence,
            ),
        };
        // Same precedence as the heuristic: a paid claim always stands.
        Prediction {
            p_ctr: predicted(explicit.p_ctr, modeled.p_ctr, modeled.p_ctr),
            p_cvr: predicted(explicit.p_cvr, modeled.p_cvr, modeled.p_cvr),
            p_wegu: predicted(explicit.p_wegu, modeled.p_wegu, modeled.p_wegu),
        }
    }
}

/// The local per-candidate predictor is EITHER a trained artifact OR the
/// heuristic. The model-serving LLM stage is a separate concern owned by
/// `RemoteScorer` (rank/mod.rs wires it as the heavy-ranker slot); routing
/// an endpoint through here used to construct a predictor that could never
/// serve — the stub that always reported degraded.
pub(crate) fn choose_predictor(
    model_artifact: Option<&std::path::Path>,
) -> Result<Box<dyn MultiObjectivePredictor>, String> {
    if let Some(path) = model_artifact.filter(|path| !path.as_os_str().is_empty()) {
        tracing::info!(artifact = %path.display(), "serving trained rank model artifact");
        return Ok(Box::new(LinearPredictor::load(path)?));
    }
    Ok(Box::new(HeuristicPredictor))
}

pub(crate) fn model_version_label(predictor: &dyn MultiObjectivePredictor) -> Option<String> {
    predictor
        .as_any()
        .downcast_ref::<LinearPredictor>()
        .map(|model| format!("linear-{}", model.version()))
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
            route_completion: 0.0,
            domain_affinity: 0.0,
            author_affinity: 0.0,
            impression_fatigue: 0.0,
            direct_negative_feedback: 0.0,
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
            route_completion: f64::NAN,
            domain_affinity: f64::NAN,
            author_affinity: f64::INFINITY,
            impression_fatigue: 0.0,
            direct_negative_feedback: f64::NAN,
        };
        let prediction = predictor.predict(&evidence);
        assert!(prediction.p_ctr.is_finite() && prediction.p_ctr > 0.0);
        assert!(prediction.p_cvr.is_finite());
        assert!(prediction.p_wegu.is_finite());
    }

    #[test]
    fn artifact_absent_selects_the_heuristic() {
        assert!(choose_predictor(None)
            .expect("heuristic")
            .as_any()
            .is::<HeuristicPredictor>());
        assert!(choose_predictor(Some(std::path::Path::new("")))
            .expect("heuristic")
            .as_any()
            .is::<HeuristicPredictor>());
    }

    fn evidence_with(values: &[(&str, f64)]) -> ObjectiveEvidence {
        let mut evidence = ObjectiveEvidence {
            explicit_ctr: 0.0,
            explicit_cvr: 0.0,
            explicit_wegu: 0.0,
            observed_ctr: 0.0,
            observed_cvr: 0.0,
            observed_wegu: 0.0,
            route_completion: 0.0,
            domain_affinity: 0.0,
            author_affinity: 0.0,
            impression_fatigue: 0.0,
            direct_negative_feedback: 0.0,
        };
        for (name, value) in values {
            let value = *value;
            match *name {
                "observed_ctr" => evidence.observed_ctr = value,
                "observed_cvr" => evidence.observed_cvr = value,
                "observed_wegu" => evidence.observed_wegu = value,
                "domain_affinity" => evidence.domain_affinity = value,
                _ => panic!("test fixture only covers a few features"),
            }
        }
        evidence
    }

    fn artifact_json(weights: &str) -> String {
        format!(
            r#"{{
                "version": "lr-test-v1",
                "bias": {{"ctr": -2.0, "cvr": -2.0, "wegu": -2.0}},
                "weights": {weights}
            }}"#
        )
    }

    #[test]
    fn linear_predictor_serves_bounded_sigmoid_predictions() {
        let path = std::env::temp_dir().join("bookway-rank-artifact-test.json");
        std::fs::write(
            &path,
            artifact_json(
                r#"{"ctr": {"observed_ctr": 4.0, "domain_affinity": 1.0}, "cvr": {"observed_cvr": 3.0}, "wegu": {"observed_wegu": 3.0, "domain_affinity": 0.5}}"#,
            ),
        )
        .expect("write artifact");
        let model = LinearPredictor::load(&path).expect("valid artifact");
        let strong = model.predict(&evidence_with(&[
            ("observed_ctr", 0.9),
            ("observed_cvr", 0.5),
            ("observed_wegu", 0.8),
            ("domain_affinity", 1.0),
        ]));
        let weak = model.predict(&evidence_with(&[]));
        assert!(strong.p_ctr > weak.p_ctr);
        assert!(strong.p_wegu > weak.p_wegu);
        for value in [strong.p_ctr, strong.p_cvr, strong.p_wegu] {
            assert!(value > 0.0 && value < 1.0);
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn linear_predictor_rejects_unknown_feature_names() {
        let path = std::env::temp_dir().join("bookway-rank-artifact-bad.json");
        std::fs::write(
            &path,
            artifact_json(r#"{"ctr": {"observed_ctr_typo": 4.0}, "cvr": {}, "wegu": {}}"#),
        )
        .expect("write artifact");
        let error = LinearPredictor::load(&path).expect_err("unknown feature must fail load");
        assert!(error.contains("unknown feature"));
        std::fs::remove_file(&path).ok();
    }
}
