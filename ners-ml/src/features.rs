//! Feature Extraction for ML Models
//!
//! Converts raw metrics to normalized feature vectors for ML inference.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Normalized feature vector for ML model input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    /// p50 latency normalized [0.0, 1.0]
    pub p50_latency: f32,
    /// p99 latency normalized [0.0, 1.0]
    pub p99_latency: f32,
    /// p100 latency normalized [0.0, 1.0]
    pub p100_latency: f32,
    /// Latency trend: -1.0 (decreasing) to 1.0 (increasing)
    pub latency_trend: f32,
    
    /// Queue depth normalized [0.0, 1.0]
    pub queue_depth: f32,
    /// Queue overflow rate [0.0, 1.0]
    pub queue_overflow_rate: f32,
    /// Queue trend: -1.0 to 1.0
    pub queue_trend: f32,
    
    /// Throughput normalized [0.0, 1.0]
    pub throughput: f32,
    /// Throughput trend: -1.0 to 1.0
    pub throughput_trend: f32,
    
    /// Memory pressure [0.0, 1.0]
    pub memory_pressure: f32,
    
    /// Error rate [0.0, 1.0]
    pub error_rate: f32,
    
    /// Load category: 0=low, 0.33=medium, 0.67=high, 1.0=peak
    pub load_category: f32,
}

impl FeatureVector {
    /// Create a feature vector with all zeros
    pub fn zeros() -> Self {
        Self {
            p50_latency: 0.0,
            p99_latency: 0.0,
            p100_latency: 0.0,
            latency_trend: 0.0,
            queue_depth: 0.0,
            queue_overflow_rate: 0.0,
            queue_trend: 0.0,
            throughput: 0.0,
            throughput_trend: 0.0,
            memory_pressure: 0.0,
            error_rate: 0.0,
            load_category: 0.0,
        }
    }

    /// Convert to flat array for model input
    pub fn as_array(&self) -> [f32; 12] {
        [
            self.p50_latency,
            self.p99_latency,
            self.p100_latency,
            self.latency_trend,
            self.queue_depth,
            self.queue_overflow_rate,
            self.queue_trend,
            self.throughput,
            self.throughput_trend,
            self.memory_pressure,
            self.error_rate,
            self.load_category,
        ]
    }

    /// Number of features
    pub const FEATURE_COUNT: usize = 12;
}

impl Default for FeatureVector {
    fn default() -> Self {
        Self::zeros()
    }
}

/// Feature normalizer with configurable max values
#[derive(Debug, Clone)]
pub struct FeatureNormalizer {
    /// Maximum p99 latency in nanoseconds (10ms)
    pub p99_max_ns: u64,
    /// Maximum queue size
    pub queue_max_size: usize,
    /// Maximum throughput (req/sec)
    pub throughput_max: f64,
    /// Maximum memory in bytes
    pub memory_max_bytes: u64,
}

impl FeatureNormalizer {
    /// Create with default max values
    pub fn new() -> Self {
        Self {
            p99_max_ns: 10_000_000, // 10ms
            queue_max_size: 100_000,
            throughput_max: 200_000.0,
            memory_max_bytes: 8 * 1024 * 1024 * 1024, // 8GB
        }
    }

    /// Normalize a value to [0.0, 1.0]
    pub fn normalize(&self, raw_value: f64, max_value: f64) -> f32 {
        ((raw_value / max_value).clamp(0.0, 1.0)) as f32
    }

    /// Extract features from raw metrics
    pub fn extract(&self, metrics: &RawMetrics) -> FeatureVector {
        let p99_latency = self.normalize(metrics.p99_latency_ns as f64, self.p99_max_ns as f64);
        let queue_depth = self.normalize(metrics.queue_len as f64, self.queue_max_size as f64);
        let throughput = self.normalize(metrics.requests_per_sec, self.throughput_max);
        let memory_pressure = self.normalize(metrics.memory_used as f64, self.memory_max_bytes as f64);
        let error_rate = self.normalize(metrics.error_count as f64, (metrics.total_requests.max(1)) as f64);
        
        // Classify load
        let load_category = if throughput < 0.25 {
            0.0
        } else if throughput < 0.5 {
            0.33
        } else if throughput < 0.75 {
            0.67
        } else {
            1.0
        };

        FeatureVector {
            p50_latency: self.normalize(metrics.p50_latency_ns as f64, self.p99_max_ns as f64),
            p99_latency,
            p100_latency: self.normalize(metrics.p100_latency_ns as f64, self.p99_max_ns as f64),
            latency_trend: metrics.latency_trend,
            queue_depth,
            queue_overflow_rate: self.normalize(metrics.queue_overflows as f64, 1000.0),
            queue_trend: metrics.queue_trend,
            throughput,
            throughput_trend: metrics.throughput_trend,
            memory_pressure,
            error_rate,
            load_category,
        }
    }
}

impl Default for FeatureNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw metrics input
#[derive(Debug, Clone, Default)]
pub struct RawMetrics {
    pub p50_latency_ns: u64,
    pub p99_latency_ns: u64,
    pub p100_latency_ns: u64,
    pub latency_trend: f32,
    pub queue_len: usize,
    pub queue_overflows: usize,
    pub queue_trend: f32,
    pub requests_per_sec: f64,
    pub throughput_trend: f32,
    pub memory_used: u64,
    pub total_requests: u64,
    pub error_count: u64,
}

/// Time-series window of feature vectors
#[derive(Debug, Clone)]
pub struct MetricsWindow {
    features: VecDeque<FeatureVector>,
    window_size: usize,
}

impl MetricsWindow {
    /// Create a new metrics window
    pub fn new(window_size: usize) -> Self {
        Self {
            features: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    /// Add a feature vector to the window
    pub fn push(&mut self, features: FeatureVector) {
        if self.features.len() >= self.window_size {
            self.features.pop_front();
        }
        self.features.push_back(features);
    }

    /// Get the window as a flat tensor
    pub fn as_tensor(&self) -> Vec<f32> {
        self.features
            .iter()
            .flat_map(|f| f.as_array().into_iter())
            .collect()
    }

    /// Get the latest feature vector
    pub fn latest(&self) -> Option<&FeatureVector> {
        self.features.back()
    }

    /// Check if window is full
    pub fn is_full(&self) -> bool {
        self.features.len() >= self.window_size
    }

    /// Get current window length
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Check if window is empty
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

impl Default for MetricsWindow {
    fn default() -> Self {
        Self::new(60) // 1 minute at 1 sample/sec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_normalization() {
        let normalizer = FeatureNormalizer::new();
        
        // Test normal range
        assert_eq!(normalizer.normalize(0.0, 100.0), 0.0);
        assert_eq!(normalizer.normalize(50.0, 100.0), 0.5);
        assert_eq!(normalizer.normalize(100.0, 100.0), 1.0);
        
        // Test clamping
        assert_eq!(normalizer.normalize(200.0, 100.0), 1.0);
    }

    #[test]
    fn test_metrics_window() {
        let mut window = MetricsWindow::new(3);
        
        assert!(window.is_empty());
        
        window.push(FeatureVector::zeros());
        window.push(FeatureVector::zeros());
        
        assert_eq!(window.len(), 2);
        assert!(!window.is_full());
        
        window.push(FeatureVector::zeros());
        assert!(window.is_full());
        
        // Push one more, oldest should be evicted
        window.push(FeatureVector::zeros());
        assert_eq!(window.len(), 3);
    }
}
