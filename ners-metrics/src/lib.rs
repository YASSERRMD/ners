//! NERS Metrics Collection
//!
//! Lock-free metrics collection for per-stage tracking.
//! Uses atomic operations for thread-safe counter updates.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Per-stage metrics
#[derive(Debug)]
pub struct StageMetrics {
    /// Stage identifier
    pub id: &'static str,
    /// Total requests processed
    processed_count: AtomicU64,
    /// Current queue length
    current_queue_len: AtomicU64,
    /// Cumulative latency in nanoseconds
    latency_sum_ns: AtomicU64,
    /// Maximum latency in nanoseconds
    latency_max_ns: AtomicU64,
    /// Error count
    errors: AtomicU64,
}

impl StageMetrics {
    /// Create new stage metrics
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            processed_count: AtomicU64::new(0),
            current_queue_len: AtomicU64::new(0),
            latency_sum_ns: AtomicU64::new(0),
            latency_max_ns: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    /// Increment processed count
    pub fn inc_processed(&self) {
        self.processed_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment queue length
    pub fn inc_queue_len(&self) {
        self.current_queue_len.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement queue length
    pub fn dec_queue_len(&self) {
        self.current_queue_len.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record latency in nanoseconds
    pub fn record_latency(&self, latency_ns: u64) {
        self.latency_sum_ns.fetch_add(latency_ns, Ordering::Relaxed);
        
        // Update max latency atomically
        let mut current_max = self.latency_max_ns.load(Ordering::Relaxed);
        while latency_ns > current_max {
            match self.latency_max_ns.compare_exchange_weak(
                current_max,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    /// Increment error count
    pub fn inc_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of the metrics
    pub fn snapshot(&self) -> StageMetricsSnapshot {
        StageMetricsSnapshot {
            id: self.id,
            processed_count: self.processed_count.load(Ordering::Relaxed),
            current_queue_len: self.current_queue_len.load(Ordering::Relaxed),
            latency_sum_ns: self.latency_sum_ns.load(Ordering::Relaxed),
            latency_max_ns: self.latency_max_ns.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of stage metrics (non-atomic, copyable)
#[derive(Debug, Clone)]
pub struct StageMetricsSnapshot {
    pub id: &'static str,
    pub processed_count: u64,
    pub current_queue_len: u64,
    pub latency_sum_ns: u64,
    pub latency_max_ns: u64,
    pub errors: u64,
}

impl StageMetricsSnapshot {
    /// Calculate average latency in nanoseconds
    pub fn avg_latency_ns(&self) -> u64 {
        if self.processed_count > 0 {
            self.latency_sum_ns / self.processed_count
        } else {
            0
        }
    }
}

/// Snapshot of all metrics
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: Instant,
    pub stages: HashMap<&'static str, StageMetricsSnapshot>,
    pub total_conns: usize,
    pub total_requests: u64,
}

/// Metrics collector for all stages
pub struct MetricsCollector {
    stages: HashMap<&'static str, StageMetrics>,
    total_conns: AtomicU64,
    total_requests: AtomicU64,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let mut stages = HashMap::new();
        
        // Pre-register all stages
        for stage_id in &["net_in", "parse", "route", "app", "encode", "net_out"] {
            stages.insert(*stage_id, StageMetrics::new(stage_id));
        }
        
        Self {
            stages,
            total_conns: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
        }
    }

    /// Get stage metrics by ID
    pub fn stage(&self, stage_id: &'static str) -> Option<&StageMetrics> {
        self.stages.get(stage_id)
    }

    /// Record the start of stage processing, returns timestamp in nanos
    pub fn record_stage_start(&self, _stage_id: &'static str) -> u128 {
        Instant::now().elapsed().as_nanos()
    }

    /// Record the end of stage processing
    pub fn record_stage_end(&self, stage_id: &'static str, start_ts: u128) {
        let end_ts = Instant::now().elapsed().as_nanos();
        let latency = (end_ts.saturating_sub(start_ts)) as u64;
        
        if let Some(stage) = self.stages.get(stage_id) {
            stage.record_latency(latency);
            stage.inc_processed();
        }
    }

    /// Increment queue length for a stage
    pub fn inc_queue_len(&self, stage_id: &'static str) {
        if let Some(stage) = self.stages.get(stage_id) {
            stage.inc_queue_len();
        }
    }

    /// Decrement queue length for a stage
    pub fn dec_queue_len(&self, stage_id: &'static str) {
        if let Some(stage) = self.stages.get(stage_id) {
            stage.dec_queue_len();
        }
    }

    /// Increment total connections
    pub fn inc_total_conns(&self) {
        self.total_conns.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement total connections
    pub fn dec_total_conns(&self) {
        self.total_conns.fetch_sub(1, Ordering::Relaxed);
    }

    /// Increment total requests
    pub fn inc_total_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        let mut stages = HashMap::new();
        
        for (id, stage) in &self.stages {
            stages.insert(*id, stage.snapshot());
        }
        
        MetricsSnapshot {
            timestamp: Instant::now(),
            stages,
            total_conns: self.total_conns.load(Ordering::Relaxed) as usize,
            total_requests: self.total_requests.load(Ordering::Relaxed),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        for stage in self.stages.values() {
            stage.processed_count.store(0, Ordering::Relaxed);
            stage.current_queue_len.store(0, Ordering::Relaxed);
            stage.latency_sum_ns.store(0, Ordering::Relaxed);
            stage.latency_max_ns.store(0, Ordering::Relaxed);
            stage.errors.store(0, Ordering::Relaxed);
        }
        self.total_conns.store(0, Ordering::Relaxed);
        self.total_requests.store(0, Ordering::Relaxed);
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_metrics() {
        let metrics = StageMetrics::new("test");
        
        metrics.inc_processed();
        metrics.inc_processed();
        metrics.record_latency(1000);
        metrics.record_latency(2000);
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.processed_count, 2);
        assert_eq!(snapshot.latency_max_ns, 2000);
    }

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        
        collector.inc_total_conns();
        collector.inc_total_requests();
        
        if let Some(stage) = collector.stage("net_in") {
            stage.inc_processed();
        }
        
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.total_conns, 1);
        assert_eq!(snapshot.total_requests, 1);
        assert_eq!(snapshot.stages["net_in"].processed_count, 1);
    }
}
