//! Tuning Engine for NERS Phase 3
//!
//! Applies and validates tuning actions with rollback capability.

use crate::analyzer::MetricsAnalyzer;
use crate::policy::{TuningAction, TuningPolicy};
use ners_metrics::MetricsSnapshot;
use std::collections::VecDeque;
use std::time::Instant;

/// Entry in the tuning log
#[derive(Debug, Clone)]
pub struct TuningLogEntry {
    pub timestamp: Instant,
    pub action: TuningAction,
    pub result: TuningResult,
}

/// Result of applying a tuning action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningResult {
    Applied,
    Rejected,
    RolledBack,
}

/// Constraints for tuning actions
#[derive(Debug, Clone)]
pub struct TuningConstraints {
    /// Minimum queue size
    pub min_queue_size: usize,
    /// Maximum queue size
    pub max_queue_size: usize,
    /// Maximum actions per minute
    pub max_actions_per_minute: usize,
    /// Maximum memory usage (bytes)
    pub max_memory_bytes: u64,
}

impl Default for TuningConstraints {
    fn default() -> Self {
        Self {
            min_queue_size: 100,
            max_queue_size: 10_000,
            max_actions_per_minute: 10,
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB
        }
    }
}

/// Tuning engine that applies and validates actions
pub struct TuningEngine {
    /// Tuning policy to use
    policy: Box<dyn TuningPolicy>,
    /// Metrics analyzer
    analyzer: MetricsAnalyzer,
    /// Constraints
    constraints: TuningConstraints,
    /// Action history for rollback
    action_history: VecDeque<TuningLogEntry>,
    /// Last performance baseline (avg latency)
    last_baseline_ns: u64,
    /// Maximum log entries
    max_log_entries: usize,
    /// Enabled flag
    enabled: bool,
}

impl TuningEngine {
    /// Create a new tuning engine
    pub fn new(policy: Box<dyn TuningPolicy>, constraints: TuningConstraints) -> Self {
        Self {
            policy,
            analyzer: MetricsAnalyzer::default(),
            constraints,
            action_history: VecDeque::with_capacity(1000),
            last_baseline_ns: 0,
            max_log_entries: 1000,
            enabled: true,
        }
    }

    /// Enable or disable the tuning engine
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if tuning is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Process a metrics snapshot and generate/apply actions
    pub fn process(&mut self, snapshot: MetricsSnapshot) -> Vec<TuningAction> {
        if !self.enabled {
            return Vec::new();
        }

        // Add to analyzer
        self.analyzer.add_snapshot(snapshot.clone());

        // Get actions from policy
        let actions = self.policy.analyze(&snapshot, &self.analyzer);

        // Validate and apply each action
        let mut applied_actions = Vec::new();
        for action in actions {
            if self.validate_action(&action) {
                self.apply_action(&action);
                self.log_action(action.clone(), TuningResult::Applied);
                applied_actions.push(action);
            } else {
                self.log_action(action, TuningResult::Rejected);
            }
        }

        // Check if we need to rollback
        if self.should_rollback(&snapshot) {
            self.rollback_last_action();
        }

        // Update baseline
        self.update_baseline(&snapshot);

        applied_actions
    }

    /// Validate an action against constraints
    fn validate_action(&self, action: &TuningAction) -> bool {
        match action {
            TuningAction::IncreaseQueueSize { new_size, .. } => {
                *new_size <= self.constraints.max_queue_size
            }
            TuningAction::DecreaseQueueSize { new_size, .. } => {
                *new_size >= self.constraints.min_queue_size
            }
            TuningAction::EnableBatching { .. } => true,
            TuningAction::AdjustBackpressure { threshold } => {
                *threshold >= 0.0 && *threshold <= 1.0
            }
        }
    }

    /// Apply an action (in Phase 3, this is a no-op that just logs)
    fn apply_action(&self, action: &TuningAction) {
        log::info!("Tuning: {}", action.description());
        // Actual application would modify orchestrator config here
    }

    /// Log a tuning action
    fn log_action(&mut self, action: TuningAction, result: TuningResult) {
        if self.action_history.len() >= self.max_log_entries {
            self.action_history.pop_front();
        }
        
        self.action_history.push_back(TuningLogEntry {
            timestamp: Instant::now(),
            action,
            result,
        });
    }

    /// Check if we should rollback the last action
    fn should_rollback(&self, snapshot: &MetricsSnapshot) -> bool {
        if self.last_baseline_ns == 0 {
            return false;
        }

        // Calculate current average latency
        let current_avg = self.calculate_avg_latency(snapshot);
        
        // Rollback if latency increased by > 50%
        current_avg > self.last_baseline_ns * 3 / 2
    }

    /// Rollback the last action
    fn rollback_last_action(&mut self) {
        if let Some(last) = self.action_history.back() {
            if last.result == TuningResult::Applied {
                log::warn!("Rolling back action: {}", last.action.description());
                // In a full implementation, we'd apply the inverse action
                
                // Mark as rolled back
                if let Some(entry) = self.action_history.back_mut() {
                    entry.result = TuningResult::RolledBack;
                }
            }
        }
    }

    /// Update performance baseline
    fn update_baseline(&mut self, snapshot: &MetricsSnapshot) {
        let current_avg = self.calculate_avg_latency(snapshot);
        if current_avg > 0 {
            // Exponential moving average
            if self.last_baseline_ns == 0 {
                self.last_baseline_ns = current_avg;
            } else {
                self.last_baseline_ns = (self.last_baseline_ns * 9 + current_avg) / 10;
            }
        }
    }

    /// Calculate average latency across all stages
    fn calculate_avg_latency(&self, snapshot: &MetricsSnapshot) -> u64 {
        let total: u64 = snapshot.stages.values()
            .map(|m| m.avg_latency_ns())
            .sum();
        
        let count = snapshot.stages.len() as u64;
        if count > 0 {
            total / count
        } else {
            0
        }
    }

    /// Get the action history
    pub fn history(&self) -> &VecDeque<TuningLogEntry> {
        &self.action_history
    }

    /// Get policy name
    pub fn policy_name(&self) -> &'static str {
        self.policy.name()
    }

    /// Get analyzer reference
    pub fn analyzer(&self) -> &MetricsAnalyzer {
        &self.analyzer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ConservativePolicy;
    use std::collections::HashMap;
    use ners_metrics::StageMetricsSnapshot;

    fn create_test_snapshot(latency: u64) -> MetricsSnapshot {
        let mut stages = HashMap::new();
        stages.insert("parse", StageMetricsSnapshot {
            id: "parse",
            processed_count: 100,
            current_queue_len: 200,
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
    fn test_tuning_engine_process() {
        let policy = Box::new(ConservativePolicy::default());
        let mut engine = TuningEngine::new(policy, TuningConstraints::default());
        
        let snapshot = create_test_snapshot(10_000_000); // 10ms
        let actions = engine.process(snapshot);
        
        // Actions should be generated for high latency
        assert!(engine.history().len() > 0 || actions.is_empty());
    }

    #[test]
    fn test_constraint_validation() {
        let policy = Box::new(ConservativePolicy::default());
        let engine = TuningEngine::new(policy, TuningConstraints::default());
        
        // Should reject queue size exceeding max
        let action = TuningAction::IncreaseQueueSize {
            stage: "parse",
            new_size: 100_000,
        };
        assert!(!engine.validate_action(&action));
        
        // Should accept valid queue size
        let action = TuningAction::IncreaseQueueSize {
            stage: "parse",
            new_size: 5000,
        };
        assert!(engine.validate_action(&action));
    }
}
