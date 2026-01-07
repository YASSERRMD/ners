//! Tuning Policy Engine for NERS Phase 3
//!
//! Defines tuning policies that generate actions based on metrics analysis.

use crate::analyzer::{AnalysisRules, Direction, MetricsAnalyzer};
use ners_metrics::MetricsSnapshot;
use std::time::{Duration, Instant};

/// Actions that can be taken to tune the system
#[derive(Debug, Clone)]
pub enum TuningAction {
    /// Increase queue size for a stage
    IncreaseQueueSize { stage: &'static str, new_size: usize },
    /// Decrease queue size for a stage
    DecreaseQueueSize { stage: &'static str, new_size: usize },
    /// Enable adaptive batching for a stage
    EnableBatching { stage: &'static str },
    /// Adjust backpressure threshold
    AdjustBackpressure { threshold: f64 },
}

impl TuningAction {
    /// Get a human-readable description of the action
    pub fn description(&self) -> String {
        match self {
            TuningAction::IncreaseQueueSize { stage, new_size } => {
                format!("Increase {} queue to {}", stage, new_size)
            }
            TuningAction::DecreaseQueueSize { stage, new_size } => {
                format!("Decrease {} queue to {}", stage, new_size)
            }
            TuningAction::EnableBatching { stage } => {
                format!("Enable batching for {}", stage)
            }
            TuningAction::AdjustBackpressure { threshold } => {
                format!("Adjust backpressure to {:.2}", threshold)
            }
        }
    }
}

/// Trait for tuning policies
pub trait TuningPolicy: Send + Sync {
    /// Analyze metrics and generate tuning actions
    fn analyze(&mut self, snapshot: &MetricsSnapshot, analyzer: &MetricsAnalyzer) -> Vec<TuningAction>;
    
    /// Get the policy name
    fn name(&self) -> &'static str;
}

/// Conservative tuning policy - safe, incremental changes
pub struct ConservativePolicy {
    rules: AnalysisRules,
    cooldown: Duration,
    last_action: Instant,
    /// Step size for queue changes (10%)
    step_size: f64,
}

impl ConservativePolicy {
    pub fn new(rules: AnalysisRules) -> Self {
        Self {
            rules,
            cooldown: Duration::from_secs(10),
            last_action: Instant::now() - Duration::from_secs(60), // Allow immediate first action
            step_size: 0.1,
        }
    }

    fn in_cooldown(&self) -> bool {
        self.last_action.elapsed() < self.cooldown
    }
}

impl Default for ConservativePolicy {
    fn default() -> Self {
        Self::new(AnalysisRules::default())
    }
}

impl TuningPolicy for ConservativePolicy {
    fn analyze(&mut self, snapshot: &MetricsSnapshot, analyzer: &MetricsAnalyzer) -> Vec<TuningAction> {
        if self.in_cooldown() {
            return Vec::new();
        }
        
        let mut actions = Vec::new();
        
        // Rule 1: If bottleneck detected with high latency, increase queue
        if let Some(bottleneck) = analyzer.identify_bottleneck() {
            if let Some(metrics) = snapshot.stages.get(bottleneck) {
                let avg_latency = metrics.avg_latency_ns();
                if avg_latency > self.rules.p99_target_ns {
                    let current_queue = metrics.current_queue_len as usize;
                    let new_size = ((current_queue as f64) * (1.0 + self.step_size)) as usize;
                    let new_size = new_size.min(10_000); // Cap at 10k
                    
                    if new_size > current_queue {
                        actions.push(TuningAction::IncreaseQueueSize {
                            stage: bottleneck,
                            new_size,
                        });
                    }
                }
            }
        }
        
        // Rule 2: If memory pressure high, reduce queues
        let memory_pressure = analyzer.get_memory_pressure();
        if memory_pressure > self.rules.memory_pressure_threshold {
            for (stage_id, metrics) in &snapshot.stages {
                let current_queue = metrics.current_queue_len as usize;
                if current_queue > 100 {
                    let new_size = ((current_queue as f64) * (1.0 - self.step_size)) as usize;
                    let new_size = new_size.max(100); // Min 100
                    
                    actions.push(TuningAction::DecreaseQueueSize {
                        stage: *stage_id,
                        new_size,
                    });
                }
            }
        }
        
        // Rule 3: If queue is consistently overflowing, enable batching
        for stage_id in analyzer.detect_queue_overflow() {
            if let Some(trend) = analyzer.get_trend(stage_id) {
                if trend.queue_trend == Direction::Increasing {
                    actions.push(TuningAction::EnableBatching { stage: stage_id });
                }
            }
        }
        
        if !actions.is_empty() {
            self.last_action = Instant::now();
        }
        
        // Limit to 3 actions max
        actions.truncate(3);
        actions
    }

    fn name(&self) -> &'static str {
        "conservative"
    }
}

/// Aggressive tuning policy - faster adaptation
pub struct AggressivePolicy {
    #[allow(dead_code)]
    rules: AnalysisRules,
    cooldown: Duration,
    last_action: Instant,
    /// Step size for queue changes (20%)
    step_size: f64,
}

impl AggressivePolicy {
    pub fn new(rules: AnalysisRules) -> Self {
        Self {
            rules,
            cooldown: Duration::from_secs(5),
            last_action: Instant::now() - Duration::from_secs(60),
            step_size: 0.2,
        }
    }

    fn in_cooldown(&self) -> bool {
        self.last_action.elapsed() < self.cooldown
    }
}

impl Default for AggressivePolicy {
    fn default() -> Self {
        Self::new(AnalysisRules::default())
    }
}

impl TuningPolicy for AggressivePolicy {
    fn analyze(&mut self, snapshot: &MetricsSnapshot, analyzer: &MetricsAnalyzer) -> Vec<TuningAction> {
        if self.in_cooldown() {
            return Vec::new();
        }
        
        let mut actions = Vec::new();
        
        // Similar logic but with larger step sizes
        if let Some(bottleneck) = analyzer.identify_bottleneck() {
            if let Some(metrics) = snapshot.stages.get(bottleneck) {
                let current_queue = metrics.current_queue_len as usize;
                let new_size = ((current_queue as f64) * (1.0 + self.step_size)) as usize;
                let new_size = new_size.min(10_000);
                
                actions.push(TuningAction::IncreaseQueueSize {
                    stage: bottleneck,
                    new_size,
                });
            }
        }
        
        let memory_pressure = analyzer.get_memory_pressure();
        if memory_pressure > 0.7 { // Lower threshold than conservative
            for (stage_id, metrics) in &snapshot.stages {
                let current_queue = metrics.current_queue_len as usize;
                if current_queue > 100 {
                    let new_size = ((current_queue as f64) * (1.0 - self.step_size)) as usize;
                    actions.push(TuningAction::DecreaseQueueSize {
                        stage: *stage_id,
                        new_size: new_size.max(100),
                    });
                }
            }
        }
        
        if !actions.is_empty() {
            self.last_action = Instant::now();
        }
        
        actions.truncate(5); // Allow more actions
        actions
    }

    fn name(&self) -> &'static str {
        "aggressive"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use ners_metrics::StageMetricsSnapshot;

    fn create_test_snapshot() -> MetricsSnapshot {
        let mut stages = HashMap::new();
        stages.insert("parse", StageMetricsSnapshot {
            id: "parse",
            processed_count: 100,
            current_queue_len: 200,
            latency_sum_ns: 10_000_000 * 100, // 10ms avg
            latency_max_ns: 15_000_000,
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
    fn test_conservative_policy() {
        let mut policy = ConservativePolicy::default();
        let mut analyzer = MetricsAnalyzer::default();
        let snapshot = create_test_snapshot();
        
        analyzer.add_snapshot(snapshot.clone());
        
        let actions = policy.analyze(&snapshot, &analyzer);
        
        // Should suggest increasing queue for bottleneck
        assert!(!actions.is_empty());
    }
}
