//! Multi-Stage Orchestrator for NERS Phase 2
//!
//! Manages spawning and coordination of all pipeline stages.

use crate::affinity::available_cores;
use crate::conn::ConnSlab;
use crate::executor::{ExecutableStage, StageExecutor};
use crate::queue::RingQueue;
use ners_metrics::MetricsCollector;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Configuration for the multi-stage orchestrator
pub struct OrchestratorConfig {
    /// Number of connections to pre-allocate
    pub slab_capacity: usize,
    /// Queue capacity between stages
    pub queue_capacity: usize,
    /// Starting core for pinning (stages use consecutive cores)
    pub start_core: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            slab_capacity: 10_000,
            queue_capacity: 4096,
            start_core: 0,
        }
    }
}

/// Multi-stage orchestrator that manages all pipeline stages
pub struct Orchestrator {
    /// Shared connection slab
    slab: Arc<Mutex<ConnSlab>>,
    /// Shared metrics collector
    metrics: Arc<MetricsCollector>,
    /// Inter-stage queues
    queues: Vec<Arc<RingQueue>>,
    /// Stage executors
    executors: Vec<StageExecutor>,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    /// Configuration
    config: OrchestratorConfig,
}

impl Orchestrator {
    /// Create a new orchestrator with the given configuration
    pub fn new(config: OrchestratorConfig) -> Self {
        let slab = Arc::new(Mutex::new(ConnSlab::new(config.slab_capacity)));
        let metrics = Arc::new(MetricsCollector::new());
        
        // Create queues for each stage transition
        // NetIn -> Parse -> Route -> App -> Encode -> NetOut
        let queues: Vec<Arc<RingQueue>> = (0..6)
            .map(|_| Arc::new(RingQueue::new(config.queue_capacity)))
            .collect();
        
        let shutdown = Arc::new(AtomicBool::new(false));
        
        Self {
            slab,
            metrics,
            queues,
            executors: Vec::new(),
            shutdown,
            config,
        }
    }

    /// Get a clone of the queue at the given index
    pub fn get_queue(&self, index: usize) -> Arc<RingQueue> {
        Arc::clone(&self.queues[index])
    }

    /// Get a clone of the shared slab
    pub fn get_slab(&self) -> Arc<Mutex<ConnSlab>> {
        Arc::clone(&self.slab)
    }

    /// Get a clone of the metrics collector
    pub fn get_metrics(&self) -> Arc<MetricsCollector> {
        Arc::clone(&self.metrics)
    }

    /// Spawn a stage with automatic core assignment
    pub fn spawn_stage<S: ExecutableStage>(
        &mut self,
        stage: S,
        input_queue_idx: usize,
        output_queue_idx: Option<usize>,
    ) {
        let core_id = (self.config.start_core + self.executors.len()) % available_cores();
        
        let input_queue = Arc::clone(&self.queues[input_queue_idx]);
        let output_queue = output_queue_idx.map(|idx| Arc::clone(&self.queues[idx]));
        
        let executor = StageExecutor::spawn(
            stage,
            core_id,
            input_queue,
            output_queue,
            Arc::clone(&self.slab),
            Arc::clone(&self.metrics),
        );
        
        log::info!("Spawned stage '{}' on core {}", executor.name(), core_id);
        self.executors.push(executor);
    }

    /// Check if shutdown was requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Signal all stages to shutdown
    pub fn shutdown(&self) {
        log::info!("Orchestrator initiating shutdown...");
        self.shutdown.store(true, Ordering::Relaxed);
        
        for executor in &self.executors {
            executor.shutdown();
        }
    }

    /// Wait for all stages to complete
    pub fn join(mut self) {
        for executor in self.executors.drain(..) {
            executor.join();
        }
        log::info!("All stages stopped");
    }

    /// Get the number of active connections
    pub fn active_connections(&self) -> usize {
        self.slab.lock().active_count()
    }

    /// Get a metrics snapshot
    pub fn metrics_snapshot(&self) -> ners_metrics::MetricsSnapshot {
        self.metrics.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::ConnId;
    use crate::mux::IoMultiplexer;

    struct DummyStage;

    impl ExecutableStage for DummyStage {
        fn name(&self) -> &'static str {
            "dummy"
        }

        fn process_one(
            &mut self,
            conn_id: ConnId,
            _slab: &mut ConnSlab,
            _mux: &mut IoMultiplexer,
            _metrics: &MetricsCollector,
        ) -> Option<ConnId> {
            Some(conn_id)
        }
    }

    #[test]
    fn test_orchestrator_creation() {
        let config = OrchestratorConfig::default();
        let orch = Orchestrator::new(config);
        
        assert_eq!(orch.queues.len(), 6);
        assert_eq!(orch.executors.len(), 0);
    }
}
