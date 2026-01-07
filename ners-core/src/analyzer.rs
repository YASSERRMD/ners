//! Metrics Analyzer for NERS Phase 3
//!
//! Converts raw metrics into actionable insights for autotuning.

use ners_metrics::{MetricsSnapshot, StageMetricsSnapshot};
use std::collections::{HashMap, VecDeque};


/// Direction of a metric trend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Increasing,
    Decreasing,
    Stable,
}

/// Trend information for a stage
#[derive(Debug, Clone)]
pub struct StageTrend {
    pub stage_id: &'static str,
    pub latency_trend: Direction,
    pub queue_trend: Direction,
    pub throughput_trend: Direction,
}

/// Analysis rules configuration
#[derive(Debug, Clone)]
pub struct AnalysisRules {
    /// Target p99 latency in nanoseconds (default: 5ms)
    pub p99_target_ns: u64,
    /// Queue length alert threshold
    pub queue_threshold: u64,
    /// Number of snapshots to consider for trend (3-5)
    pub trend_window: usize,
    /// Memory pressure threshold (0.0-1.0)
    pub memory_pressure_threshold: f64,
}

impl Default for AnalysisRules {
    fn default() -> Self {
        Self {
            p99_target_ns: 5_000_000, // 5ms
            queue_threshold: 500,
            trend_window: 3,
            memory_pressure_threshold: 0.8,
        }
    }
}

/// Metrics analyzer for autotuning decisions
pub struct MetricsAnalyzer {
    /// Historical snapshots (last 60 for 1 minute window at 1/sec)
    history: VecDeque<MetricsSnapshot>,
    /// Current trends per stage
    trends: HashMap<&'static str, StageTrend>,
    /// Analysis rules
    rules: AnalysisRules,
    /// Maximum history size
    max_history: usize,
}

impl MetricsAnalyzer {
    /// Create a new metrics analyzer
    pub fn new(rules: AnalysisRules) -> Self {
        Self {
            history: VecDeque::with_capacity(60),
            trends: HashMap::new(),
            rules,
            max_history: 60,
        }
    }

    /// Add a new snapshot and update analysis
    pub fn add_snapshot(&mut self, snapshot: MetricsSnapshot) {
        // Maintain history size
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        
        // Compute trends before adding
        self.compute_trends(&snapshot);
        
        self.history.push_back(snapshot);
    }

    /// Identify the bottleneck stage (highest average latency)
    pub fn identify_bottleneck(&self) -> Option<&'static str> {
        let latest = self.history.back()?;
        
        let mut max_latency = 0u64;
        let mut bottleneck = None;
        
        for (stage_id, metrics) in &latest.stages {
            let avg_latency = metrics.avg_latency_ns();
            if avg_latency > max_latency && avg_latency > self.rules.p99_target_ns {
                max_latency = avg_latency;
                bottleneck = Some(*stage_id);
            }
        }
        
        bottleneck
    }

    /// Detect stages with queue overflow
    pub fn detect_queue_overflow(&self) -> Vec<&'static str> {
        let latest = match self.history.back() {
            Some(s) => s,
            None => return Vec::new(),
        };
        
        latest.stages.iter()
            .filter(|(_, m)| m.current_queue_len > self.rules.queue_threshold)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get memory pressure (0.0 = plenty, 1.0 = critical)
    pub fn get_memory_pressure(&self) -> f64 {
        // Simple heuristic based on queue lengths
        let latest = match self.history.back() {
            Some(s) => s,
            None => return 0.0,
        };
        
        let total_queue: u64 = latest.stages.values()
            .map(|m| m.current_queue_len)
            .sum();
        
        // Assume max 10k total queue capacity
        (total_queue as f64 / 10_000.0).min(1.0)
    }

    /// Get trend for a stage
    pub fn get_trend(&self, stage_id: &'static str) -> Option<&StageTrend> {
        self.trends.get(stage_id)
    }

    /// Compute trends based on recent history
    fn compute_trends(&mut self, current: &MetricsSnapshot) {
        if self.history.len() < self.rules.trend_window {
            return;
        }
        
        let window_start = self.history.len().saturating_sub(self.rules.trend_window);
        
        for (stage_id, current_metrics) in &current.stages {
            let historical: Vec<&StageMetricsSnapshot> = self.history
                .iter()
                .skip(window_start)
                .filter_map(|s| s.stages.get(stage_id))
                .collect();
            
            if historical.is_empty() {
                continue;
            }
            
            // Calculate trend direction for latency
            let latency_trend = self.calculate_direction(
                historical.iter().map(|m| m.avg_latency_ns()).collect(),
                current_metrics.avg_latency_ns(),
            );
            
            // Calculate trend direction for queue
            let queue_trend = self.calculate_direction(
                historical.iter().map(|m| m.current_queue_len).collect(),
                current_metrics.current_queue_len,
            );
            
            // Calculate trend direction for throughput
            let throughput_trend = self.calculate_direction(
                historical.iter().map(|m| m.processed_count).collect(),
                current_metrics.processed_count,
            );
            
            self.trends.insert(*stage_id, StageTrend {
                stage_id,
                latency_trend,
                queue_trend,
                throughput_trend,
            });
        }
    }

    fn calculate_direction(&self, history: Vec<u64>, current: u64) -> Direction {
        if history.is_empty() {
            return Direction::Stable;
        }
        
        let avg: u64 = history.iter().sum::<u64>() / history.len() as u64;
        
        // 10% threshold for trend detection
        let threshold = avg / 10;
        
        if current > avg + threshold {
            Direction::Increasing
        } else if current + threshold < avg {
            Direction::Decreasing
        } else {
            Direction::Stable
        }
    }

    /// Get the latest snapshot
    pub fn latest(&self) -> Option<&MetricsSnapshot> {
        self.history.back()
    }

    /// Get history length
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

impl Default for MetricsAnalyzer {
    fn default() -> Self {
        Self::new(AnalysisRules::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Instant;

    fn create_test_snapshot(queue_len: u64, latency: u64) -> MetricsSnapshot {
        let mut stages = HashMap::new();
        stages.insert("parse", StageMetricsSnapshot {
            id: "parse",
            processed_count: 100,
            current_queue_len: queue_len,
            latency_sum_ns: latency * 100,
            latency_max_ns: latency,
            errors: 0,
        });
        
        MetricsSnapshot {
            timestamp: Instant::now(),
            stages,
            total_conns: 10,
            total_requests: 100,
        }
    }

    #[test]
    fn test_bottleneck_detection() {
        let mut analyzer = MetricsAnalyzer::default();
        
        // Add snapshot with high latency
        analyzer.add_snapshot(create_test_snapshot(100, 10_000_000)); // 10ms
        
        let bottleneck = analyzer.identify_bottleneck();
        assert_eq!(bottleneck, Some("parse"));
    }

    #[test]
    fn test_queue_overflow_detection() {
        let mut analyzer = MetricsAnalyzer::default();
        
        analyzer.add_snapshot(create_test_snapshot(1000, 1_000_000)); // High queue
        
        let overflows = analyzer.detect_queue_overflow();
        assert!(overflows.contains(&"parse"));
    }
}
