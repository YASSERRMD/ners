//! Stage Executor for NERS Phase 2
//!
//! Each stage runs on a dedicated thread with optional core pinning.

use crate::affinity::pin_to_core;
use crate::conn::{ConnId, ConnSlab};
use crate::mux::IoMultiplexer;
use crate::queue::RingQueue;
use ners_metrics::MetricsCollector;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Trait for stages that can be executed by a StageExecutor
pub trait ExecutableStage: Send + 'static {
    /// Get the stage name
    fn name(&self) -> &'static str;
    
    /// Process a single connection
    fn process_one(
        &mut self,
        conn_id: ConnId,
        slab: &mut ConnSlab,
        mux: &mut IoMultiplexer,
        metrics: &MetricsCollector,
    ) -> Option<ConnId>;
}

/// Executor that runs a stage on a dedicated thread
pub struct StageExecutor {
    /// Thread handle
    handle: Option<JoinHandle<()>>,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    /// Stage name for logging
    stage_name: &'static str,
}

impl StageExecutor {
    /// Create and start a new stage executor
    pub fn spawn<S: ExecutableStage>(
        stage: S,
        core_id: usize,
        input_queue: Arc<RingQueue>,
        output_queue: Option<Arc<RingQueue>>,
        slab: Arc<Mutex<ConnSlab>>,
        metrics: Arc<MetricsCollector>,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let stage_name = stage.name();
        
        let handle = thread::Builder::new()
            .name(format!("ners-{}", stage_name))
            .spawn(move || {
                Self::run_loop(
                    stage,
                    core_id,
                    input_queue,
                    output_queue,
                    slab,
                    metrics,
                    shutdown_clone,
                );
            })
            .expect("Failed to spawn stage thread");
        
        Self {
            handle: Some(handle),
            shutdown,
            stage_name,
        }
    }

    fn run_loop<S: ExecutableStage>(
        mut stage: S,
        core_id: usize,
        input_queue: Arc<RingQueue>,
        output_queue: Option<Arc<RingQueue>>,
        slab: Arc<Mutex<ConnSlab>>,
        metrics: Arc<MetricsCollector>,
        shutdown: Arc<AtomicBool>,
    ) {
        // Pin to core
        if let Err(e) = pin_to_core(core_id) {
            log::warn!("Failed to pin {} to core {}: {}", stage.name(), core_id, e);
        } else {
            log::info!("Stage {} pinned to core {}", stage.name(), core_id);
        }

        let mut mux = IoMultiplexer::new(256).expect("Failed to create IoMultiplexer");
        let mut idle_count = 0u32;

        while !shutdown.load(Ordering::Relaxed) {
            let mut processed = 0;

            // Process up to 100 items per iteration
            for _ in 0..100 {
                let conn_id = match input_queue.pop() {
                    Some(id) => id,
                    None => break,
                };

                let mut slab_guard = slab.lock();
                if let Some(next_id) = stage.process_one(conn_id, &mut slab_guard, &mut mux, &metrics) {
                    if let Some(ref out_q) = output_queue {
                        let _ = out_q.push(next_id);
                    }
                }
                drop(slab_guard);
                
                processed += 1;
            }

            // Adaptive sleep when idle
            if processed == 0 {
                idle_count += 1;
                if idle_count > 1000 {
                    thread::sleep(std::time::Duration::from_micros(100));
                } else {
                    thread::yield_now();
                }
            } else {
                idle_count = 0;
            }
        }

        log::info!("Stage {} shutting down", stage.name());
    }

    /// Signal the executor to shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Wait for the executor to finish
    pub fn join(mut self) {
        self.shutdown();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Get the stage name
    pub fn name(&self) -> &'static str {
        self.stage_name
    }
}

impl Drop for StageExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}
