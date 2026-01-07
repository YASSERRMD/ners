//! ML Policy Bridge
//!
//! Bridges NERS autotuning with external ML models for learned policies.

use crate::features::{FeatureVector, MetricsWindow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;

/// ML Bridge errors
#[derive(Debug, Error)]
pub enum MlBridgeError {
    #[error("Model not available")]
    ModelUnavailable,
    #[error("Prediction failed: {0}")]
    PredictionFailed(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Timeout")]
    Timeout,
}

/// ML model prediction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlPrediction {
    /// Recommended action type
    pub action: PredictedAction,
    /// Confidence score [0.0, 1.0]
    pub confidence: f32,
    /// Predicted p99 latency after action (ns)
    pub predicted_p99_ns: u64,
    /// Model version that made the prediction
    pub model_version: String,
}

/// Predicted tuning action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictedAction {
    /// No action needed
    NoOp,
    /// Increase queue size for a stage
    IncreaseQueue { stage: String, delta: i32 },
    /// Decrease queue size for a stage
    DecreaseQueue { stage: String, delta: i32 },
    /// Enable batching for a stage
    EnableBatching { stage: String },
    /// Adjust backpressure threshold
    AdjustBackpressure { threshold: f32 },
}

impl PredictedAction {
    /// Get a description of the action
    pub fn description(&self) -> String {
        match self {
            PredictedAction::NoOp => "No action".to_string(),
            PredictedAction::IncreaseQueue { stage, delta } => {
                format!("Increase {} queue by {}", stage, delta)
            }
            PredictedAction::DecreaseQueue { stage, delta } => {
                format!("Decrease {} queue by {}", stage, delta)
            }
            PredictedAction::EnableBatching { stage } => {
                format!("Enable batching for {}", stage)
            }
            PredictedAction::AdjustBackpressure { threshold } => {
                format!("Set backpressure to {:.2}", threshold)
            }
        }
    }
}

/// ML Policy Bridge configuration
#[derive(Debug, Clone)]
pub struct MlBridgeConfig {
    /// Endpoint for ML service (if using external)
    pub endpoint: Option<String>,
    /// Timeout for predictions
    pub timeout: Duration,
    /// Cache duration for predictions
    pub cache_duration: Duration,
    /// Minimum confidence to apply prediction
    pub min_confidence: f32,
    /// Enable fallback to conservative policy
    pub fallback_enabled: bool,
}

impl Default for MlBridgeConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            timeout: Duration::from_millis(100),
            cache_duration: Duration::from_secs(5),
            min_confidence: 0.7,
            fallback_enabled: true,
        }
    }
}

/// Cached prediction
struct CachedPrediction {
    prediction: MlPrediction,
    timestamp: Instant,
}

/// ML Policy Bridge
/// 
/// Connects NERS autotuning to ML models for learned policies.
pub struct MlPolicyBridge {
    config: MlBridgeConfig,
    metrics_window: MetricsWindow,
    cache: HashMap<String, CachedPrediction>,
    prediction_count: u64,
    fallback_count: u64,
    enabled: bool,
}

impl MlPolicyBridge {
    /// Create a new ML policy bridge
    pub fn new(config: MlBridgeConfig) -> Self {
        Self {
            config,
            metrics_window: MetricsWindow::default(),
            cache: HashMap::new(),
            prediction_count: 0,
            fallback_count: 0,
            enabled: true,
        }
    }

    /// Add features from current metrics
    pub fn add_features(&mut self, features: FeatureVector) {
        self.metrics_window.push(features);
    }

    /// Get a prediction from the ML model
    pub fn predict(&mut self) -> Result<MlPrediction, MlBridgeError> {
        if !self.enabled {
            return Err(MlBridgeError::ModelUnavailable);
        }

        // Check cache first
        if let Some(cached) = self.get_cached_prediction() {
            return Ok(cached);
        }

        // Get latest features
        let features = self.metrics_window.latest()
            .ok_or_else(|| MlBridgeError::InvalidInput("No features available".to_string()))?;

        // Use embedded heuristic model (no external service needed)
        let prediction = self.heuristic_predict(features);
        
        self.prediction_count += 1;
        self.cache_prediction(&prediction);
        
        Ok(prediction)
    }

    /// Get prediction if confidence is sufficient, otherwise return None
    pub fn predict_if_confident(&mut self) -> Option<MlPrediction> {
        match self.predict() {
            Ok(pred) if pred.confidence >= self.config.min_confidence => Some(pred),
            Ok(_) => {
                self.fallback_count += 1;
                None
            }
            Err(_) => {
                self.fallback_count += 1;
                None
            }
        }
    }

    /// Simple heuristic-based prediction (embedded model)
    fn heuristic_predict(&self, features: &FeatureVector) -> MlPrediction {
        // Rule-based heuristics that simulate learned behavior
        let mut action = PredictedAction::NoOp;
        let mut confidence = 0.5f32;

        // High latency with low queue → bottleneck elsewhere
        if features.p99_latency > 0.5 && features.queue_depth < 0.3 {
            action = PredictedAction::EnableBatching {
                stage: "parse".to_string(),
            };
            confidence = 0.8;
        }
        // High latency with high queue → need more capacity
        else if features.p99_latency > 0.5 && features.queue_depth > 0.7 {
            action = PredictedAction::IncreaseQueue {
                stage: "app".to_string(),
                delta: (features.queue_depth * 100.0) as i32,
            };
            confidence = 0.85;
        }
        // High memory with low throughput → decrease queues
        else if features.memory_pressure > 0.8 && features.throughput < 0.3 {
            action = PredictedAction::DecreaseQueue {
                stage: "all".to_string(),
                delta: 50,
            };
            confidence = 0.75;
        }
        // High error rate → enable backpressure
        else if features.error_rate > 0.1 {
            action = PredictedAction::AdjustBackpressure { threshold: 0.8 };
            confidence = 0.9;
        }
        // Stable state
        else if features.p99_latency < 0.2 && features.queue_depth < 0.5 {
            action = PredictedAction::NoOp;
            confidence = 0.95;
        }

        // Estimate p99 after action
        let predicted_p99_ns = match &action {
            PredictedAction::NoOp => (features.p99_latency * 10_000_000.0) as u64,
            _ => ((features.p99_latency * 0.8) * 10_000_000.0) as u64,
        };

        MlPrediction {
            action,
            confidence,
            predicted_p99_ns,
            model_version: "heuristic-v1".to_string(),
        }
    }

    fn get_cached_prediction(&self) -> Option<MlPrediction> {
        let cache_key = "latest";
        if let Some(cached) = self.cache.get(cache_key) {
            if cached.timestamp.elapsed() < self.config.cache_duration {
                return Some(cached.prediction.clone());
            }
        }
        None
    }

    fn cache_prediction(&mut self, prediction: &MlPrediction) {
        self.cache.insert(
            "latest".to_string(),
            CachedPrediction {
                prediction: prediction.clone(),
                timestamp: Instant::now(),
            },
        );
    }

    /// Enable or disable the ML bridge
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if ML bridge is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get prediction statistics
    pub fn stats(&self) -> MlBridgeStats {
        MlBridgeStats {
            prediction_count: self.prediction_count,
            fallback_count: self.fallback_count,
            window_size: self.metrics_window.len(),
        }
    }
}

impl Default for MlPolicyBridge {
    fn default() -> Self {
        Self::new(MlBridgeConfig::default())
    }
}

/// ML Bridge statistics
#[derive(Debug, Clone)]
pub struct MlBridgeStats {
    pub prediction_count: u64,
    pub fallback_count: u64,
    pub window_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ml_bridge_prediction() {
        let mut bridge = MlPolicyBridge::default();
        
        // Add some features
        let mut features = FeatureVector::zeros();
        features.p99_latency = 0.6;
        features.queue_depth = 0.8;
        
        bridge.add_features(features);
        
        let prediction = bridge.predict().unwrap();
        assert!(prediction.confidence > 0.0);
    }

    #[test]
    fn test_predicted_action_description() {
        let action = PredictedAction::IncreaseQueue {
            stage: "parse".to_string(),
            delta: 100,
        };
        
        let desc = action.description();
        assert!(desc.contains("Increase"));
        assert!(desc.contains("parse"));
    }

    #[test]
    fn test_ml_bridge_stats() {
        let mut bridge = MlPolicyBridge::default();
        bridge.add_features(FeatureVector::zeros());
        let _ = bridge.predict();
        
        let stats = bridge.stats();
        assert_eq!(stats.prediction_count, 1);
    }
}
