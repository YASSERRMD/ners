//! NERS Web Server - Main Entry Point
//!
//! Single-threaded event loop driving the 6-stage pipeline.

use ners_core::conn::ConnSlab;
use ners_core::net::TcpListener;
use ners_core::queue::RingQueue;
use ners_core::stage::{
    AppStage, EncodeStage, NetInStage, NetOutStage, ParseStage, RouteStage, Stage,
};
use ners_metrics::MetricsCollector;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> std::io::Result<()> {
    env_logger::init();
    
    log::info!("Starting NERS Web Server...");
    
    // Initialize listener
    let listener = TcpListener::new("0.0.0.0:8080")?;
    log::info!("Listening on 0.0.0.0:8080");
    
    // Initialize slab for connections
    let mut slab = ConnSlab::new(10_000);
    
    // Initialize metrics
    let metrics = MetricsCollector::new();
    
    // Create inter-stage queues
    let parse_queue = Arc::new(RingQueue::new(1024));
    let route_queue = Arc::new(RingQueue::new(1024));
    let app_queue = Arc::new(RingQueue::new(1024));
    let encode_queue = Arc::new(RingQueue::new(1024));
    let net_out_queue = Arc::new(RingQueue::new(1024));
    
    // Initialize stages
    let mut net_in = NetInStage::new(listener, Arc::clone(&parse_queue));
    let mut parse = ParseStage::new(Arc::clone(&parse_queue), Arc::clone(&route_queue));
    let mut route = RouteStage::new(Arc::clone(&route_queue), Arc::clone(&app_queue));
    let mut app = AppStage::new(Arc::clone(&app_queue), Arc::clone(&encode_queue));
    let mut encode = EncodeStage::new(Arc::clone(&encode_queue), Arc::clone(&net_out_queue));
    let mut net_out = NetOutStage::new(Arc::clone(&net_out_queue));
    
    let mut iteration: u64 = 0;
    let mut last_log = Instant::now();
    
    log::info!("NERS ready to serve requests");
    
    // Main event loop
    loop {
        // Process all stages
        net_in.process(&mut slab, &metrics);
        parse.process(&mut slab, &metrics);
        route.process(&mut slab, &metrics);
        app.process(&mut slab, &metrics);
        encode.process(&mut slab, &metrics);
        net_out.process(&mut slab, &metrics);
        
        iteration += 1;
        
        // Log metrics every second
        if last_log.elapsed() >= Duration::from_secs(1) {
            let snap = metrics.snapshot();
            log::info!(
                "Metrics: requests={}, active_conns={}",
                snap.total_requests,
                snap.total_conns
            );
            
            for (stage_id, stage_metrics) in &snap.stages {
                if stage_metrics.processed_count > 0 {
                    log::debug!(
                        "  {}: processed={}, queue_len={}",
                        stage_id,
                        stage_metrics.processed_count,
                        stage_metrics.current_queue_len
                    );
                }
            }
            
            last_log = Instant::now();
        }
        
        // Small sleep to prevent busy-looping when idle
        if slab.active_count() == 0 {
            std::thread::sleep(Duration::from_micros(100));
        }
    }
}
