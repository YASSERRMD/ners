//! Pipeline Stages for NERS
//!
//! Implements the 6-stage request processing pipeline:
//! NetIn → Parse → Route → App → Encode → NetOut

use crate::conn::{ConnId, ConnLifecycle, ConnSlab, ConnState, RouteId};
use crate::handlers::{handle_hello, handle_json, handle_not_found};
use crate::net::{read_all, write_all, TcpListener};
use crate::queue::RingQueue;
use ners_metrics::MetricsCollector;
use ners_proto_http::HttpParser;
use std::sync::Arc;

/// Trait for pipeline stages
pub trait Stage {
    /// Get the stage name
    fn name(&self) -> &'static str;
    
    /// Process pending work in this stage
    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector);
}

/// Network Input Stage - accepts new connections
pub struct NetInStage {
    listener: TcpListener,
    parse_queue: Arc<RingQueue>,
}

impl NetInStage {
    pub fn new(listener: TcpListener, parse_queue: Arc<RingQueue>) -> Self {
        Self { listener, parse_queue }
    }
}

impl Stage for NetInStage {
    fn name(&self) -> &'static str {
        "net_in"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
        // Accept all pending connections
        let streams = self.listener.accept_all();
        
        for stream in streams {
            let conn = ConnState::new(stream);
            if let Some(id) = slab.insert(conn) {
                metrics.inc_total_conns();
                if self.parse_queue.push(id).is_ok() {
                    metrics.inc_queue_len("parse");
                }
            }
        }
        
        // Read data from existing connections in parse queue
        // This is handled implicitly when parse stage pulls connections
    }
}

/// Parse Stage - parses HTTP requests
pub struct ParseStage {
    input_queue: Arc<RingQueue>,
    output_queue: Arc<RingQueue>,
    parser: HttpParser,
}

impl ParseStage {
    pub fn new(input_queue: Arc<RingQueue>, output_queue: Arc<RingQueue>) -> Self {
        Self {
            input_queue,
            output_queue,
            parser: HttpParser::new(),
        }
    }
}

impl Stage for ParseStage {
    fn name(&self) -> &'static str {
        "parse"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
        // Process up to 100 connections per iteration
        for _ in 0..100 {
            let id = match self.input_queue.pop() {
                Some(id) => id,
                None => break,
            };
            metrics.dec_queue_len("parse");
            
            let conn = match slab.get_mut(id) {
                Some(c) => c,
                None => continue,
            };
            
            // Read more data if available
            if let Some(ref mut stream) = conn.stream {
                let _ = read_all(stream, &mut conn.read_buf);
            }
            
            // Try to parse the request
            match self.parser.parse(&conn.read_buf) {
                Ok((request, consumed)) => {
                    // Successfully parsed
                    conn.request_method = Some(request.method);
                    conn.request_path = Some(request.path);
                    conn.state = ConnLifecycle::Routed;
                    conn.read_buf.clear();
                    
                    // Move to the next stage
                    if self.output_queue.push(id).is_ok() {
                        metrics.inc_queue_len("route");
                    }
                }
                Err(ners_proto_http::ParseError::Incomplete) => {
                    // Need more data, re-queue
                    if self.input_queue.push(id).is_ok() {
                        metrics.inc_queue_len("parse");
                    }
                }
                Err(_) => {
                    // Invalid request, close connection
                    conn.state = ConnLifecycle::Closed;
                    if let Some(stage) = metrics.stage("parse") {
                        stage.inc_errors();
                    }
                }
            }
        }
    }
}

/// Route Stage - matches requests to handlers
pub struct RouteStage {
    input_queue: Arc<RingQueue>,
    output_queue: Arc<RingQueue>,
}

impl RouteStage {
    pub fn new(input_queue: Arc<RingQueue>, output_queue: Arc<RingQueue>) -> Self {
        Self { input_queue, output_queue }
    }
}

impl Stage for RouteStage {
    fn name(&self) -> &'static str {
        "route"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
        for _ in 0..100 {
            let id = match self.input_queue.pop() {
                Some(id) => id,
                None => break,
            };
            metrics.dec_queue_len("route");
            
            let conn = match slab.get_mut(id) {
                Some(c) => c,
                None => continue,
            };
            
            // Match route based on path
            let route_id = match conn.request_path.as_deref() {
                Some("/") => RouteId(0),           // Hello handler
                Some("/api/test") => RouteId(1),   // JSON handler
                _ => RouteId(999),                  // Not found
            };
            
            conn.route_id = Some(route_id);
            conn.state = ConnLifecycle::Processing;
            
            if self.output_queue.push(id).is_ok() {
                metrics.inc_queue_len("app");
            }
        }
    }
}

/// App Stage - executes handlers
pub struct AppStage {
    input_queue: Arc<RingQueue>,
    output_queue: Arc<RingQueue>,
}

impl AppStage {
    pub fn new(input_queue: Arc<RingQueue>, output_queue: Arc<RingQueue>) -> Self {
        Self { input_queue, output_queue }
    }
}

impl Stage for AppStage {
    fn name(&self) -> &'static str {
        "app"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
        for _ in 0..100 {
            let id = match self.input_queue.pop() {
                Some(id) => id,
                None => break,
            };
            metrics.dec_queue_len("app");
            
            let conn = match slab.get_mut(id) {
                Some(c) => c,
                None => continue,
            };
            
            // Execute the matched handler
            match conn.route_id {
                Some(RouteId(0)) => handle_hello(conn),
                Some(RouteId(1)) => handle_json(conn),
                _ => handle_not_found(conn),
            }
            
            conn.state = ConnLifecycle::Sending;
            metrics.inc_total_requests();
            
            if self.output_queue.push(id).is_ok() {
                metrics.inc_queue_len("net_out");
            }
        }
    }
}

/// Encode Stage - currently a pass-through since handlers build responses
pub struct EncodeStage {
    input_queue: Arc<RingQueue>,
    output_queue: Arc<RingQueue>,
}

impl EncodeStage {
    pub fn new(input_queue: Arc<RingQueue>, output_queue: Arc<RingQueue>) -> Self {
        Self { input_queue, output_queue }
    }
}

impl Stage for EncodeStage {
    fn name(&self) -> &'static str {
        "encode"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
        // Currently a pass-through since handlers already build HTTP responses
        for _ in 0..100 {
            let id = match self.input_queue.pop() {
                Some(id) => id,
                None => break,
            };
            
            if self.output_queue.push(id).is_ok() {
                // Pass through
            }
        }
    }
}

/// Network Output Stage - sends responses
pub struct NetOutStage {
    input_queue: Arc<RingQueue>,
}

impl NetOutStage {
    pub fn new(input_queue: Arc<RingQueue>) -> Self {
        Self { input_queue }
    }
}

impl Stage for NetOutStage {
    fn name(&self) -> &'static str {
        "net_out"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
        for _ in 0..100 {
            let id = match self.input_queue.pop() {
                Some(id) => id,
                None => break,
            };
            metrics.dec_queue_len("net_out");
            
            let conn = match slab.get_mut(id) {
                Some(c) => c,
                None => continue,
            };
            
            // Write the response
            if let Some(ref mut stream) = conn.stream {
                match write_all(stream, &conn.write_buf, conn.bytes_written) {
                    Ok(written) => {
                        conn.bytes_written += written;
                        
                        if conn.bytes_written >= conn.write_buf.len() {
                            // Fully sent, close connection
                            conn.state = ConnLifecycle::Closed;
                            metrics.dec_total_conns();
                            slab.remove(id);
                        } else {
                            // Partial write, re-queue
                            if self.input_queue.push(id).is_ok() {
                                metrics.inc_queue_len("net_out");
                            }
                        }
                    }
                    Err(_) => {
                        // Write error, close connection
                        conn.state = ConnLifecycle::Closed;
                        metrics.dec_total_conns();
                        if let Some(stage) = metrics.stage("net_out") {
                            stage.inc_errors();
                        }
                        slab.remove(id);
                    }
                }
            } else {
                // No stream, close connection
                slab.remove(id);
            }
        }
    }
}
