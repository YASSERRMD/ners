//! NERS Web Server - Phase 2 Multi-Core Entry Point
//!
//! Multi-threaded event loop with one thread per stage.

use ners_core::net::TcpListener;
use ners_core::orchestrator::{Orchestrator, OrchestratorConfig};
use ners_core::stage::{
    AppStageMulti, EncodeStageMulti, NetOutStageMulti, ParseStageMulti, RouteStageMulti,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> std::io::Result<()> {
    env_logger::init();
    
    log::info!("Starting NERS Web Server (Phase 2 - Multi-Core)...");
    
    // Initialize listener
    let mut listener = TcpListener::new("0.0.0.0:8080")?;
    log::info!("Listening on 0.0.0.0:8080");
    
    // Create orchestrator
    let config = OrchestratorConfig {
        slab_capacity: 10_000,
        queue_capacity: 4096,
        start_core: 0,
    };
    
    let mut orchestrator = Orchestrator::new(config);
    
    // Get shared resources
    let slab = orchestrator.get_slab();
    let metrics = orchestrator.get_metrics();
    
    // Create queues
    let parse_queue = orchestrator.get_queue(0);
    let _route_queue = orchestrator.get_queue(1);
    let _app_queue = orchestrator.get_queue(2);
    let _encode_queue = orchestrator.get_queue(3);
    let _net_out_queue = orchestrator.get_queue(4);
    
    // Spawn stages (except NetIn which needs special handling)
    orchestrator.spawn_stage(ParseStageMulti::new(), 0, Some(1));
    orchestrator.spawn_stage(RouteStageMulti::new(), 1, Some(2));
    orchestrator.spawn_stage(AppStageMulti::new(), 2, Some(3));
    orchestrator.spawn_stage(EncodeStageMulti::new(), 3, Some(4));
    orchestrator.spawn_stage(NetOutStageMulti::new(), 4, None);
    
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    
    // Handle Ctrl+C
    ctrlc_handler(shutdown_clone);
    
    log::info!("NERS ready to serve requests (multi-core mode)");
    
    let mut last_log = Instant::now();
    
    // Main thread handles NetIn (accept loop)
    while !shutdown.load(Ordering::Relaxed) {
        // Accept new connections
        let streams = listener.accept_all();
        
        for stream in streams {
            let conn = ners_core::conn::ConnState::new(stream);
            let mut slab_guard = slab.lock();
            if let Some(id) = slab_guard.insert(conn) {
                metrics.inc_total_conns();
                drop(slab_guard);
                
                if parse_queue.push(id).is_ok() {
                    metrics.inc_queue_len("parse");
                }
            }
        }
        
        // Log metrics every second
        if last_log.elapsed() >= Duration::from_secs(1) {
            let snap = metrics.snapshot();
            log::info!(
                "Metrics: requests={}, active_conns={}",
                snap.total_requests,
                snap.total_conns
            );
            last_log = Instant::now();
        }
        
        // Small sleep when idle
        if slab.lock().active_count() == 0 {
            std::thread::sleep(Duration::from_micros(100));
        }
    }
    
    log::info!("Shutting down...");
    orchestrator.shutdown();
    orchestrator.join();
    log::info!("Server stopped");
    
    Ok(())
}

fn ctrlc_handler(shutdown: Arc<AtomicBool>) {
    let _ = std::thread::spawn(move || {
        // Simple signal handling - just check periodically
        // In production, use proper signal handling
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
        }
    });
}
