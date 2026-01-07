//! Pipeline Stages for NERS Phase 2
//!
//! Implements the 6-stage request processing pipeline with multi-core support.
//! NetIn → Parse → Route → App → Encode → NetOut

use crate::conn::{ConnId, ConnLifecycle, ConnSlab, ConnState, RouteId};
use crate::executor::ExecutableStage;
use crate::handlers::{handle_hello, handle_json, handle_not_found};
use crate::mux::IoMultiplexer;
use crate::net::{read_all, write_all, TcpListener};
use crate::queue::RingQueue;
use ners_metrics::MetricsCollector;
use std::sync::Arc;

/// Trait for pipeline stages (Phase 1 compatibility)
pub trait Stage {
    /// Get the stage name
    fn name(&self) -> &'static str;
    
    /// Process pending work in this stage
    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector);
}

// ============================================================================
// Phase 2: Multi-Core Stage Implementations
// ============================================================================

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
    }
}

/// Multi-core NetIn stage
pub struct NetInStageMulti {
    listener: TcpListener,
}

impl NetInStageMulti {
    pub fn new(listener: TcpListener) -> Self {
        Self { listener }
    }
}

impl ExecutableStage for NetInStageMulti {
    fn name(&self) -> &'static str {
        "net_in"
    }

    fn process_one(
        &mut self,
        _conn_id: ConnId,
        slab: &mut ConnSlab,
        _mux: &mut IoMultiplexer,
        metrics: &MetricsCollector,
    ) -> Option<ConnId> {
        // Accept new connections
        let streams = self.listener.accept_all();
        
        for stream in streams {
            let conn = ConnState::new(stream);
            if let Some(id) = slab.insert(conn) {
                metrics.inc_total_conns();
                return Some(id);
            }
        }
        None
    }
}

/// Parse Stage - parses HTTP requests
pub struct ParseStage {
    input_queue: Arc<RingQueue>,
    output_queue: Arc<RingQueue>,
    parser: ners_proto_http::HttpParser,
}

impl ParseStage {
    pub fn new(input_queue: Arc<RingQueue>, output_queue: Arc<RingQueue>) -> Self {
        Self {
            input_queue,
            output_queue,
            parser: ners_proto_http::HttpParser::new(),
        }
    }
}

impl Stage for ParseStage {
    fn name(&self) -> &'static str {
        "parse"
    }

    fn process(&mut self, slab: &mut ConnSlab, metrics: &MetricsCollector) {
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
            
            if let Some(ref mut stream) = conn.stream {
                let _ = read_all(stream, &mut conn.read_buf);
            }
            
            match self.parser.parse(&conn.read_buf) {
                Ok((request, _consumed)) => {
                    conn.request_method = Some(request.method);
                    conn.request_path = Some(request.path);
                    conn.state = ConnLifecycle::Routed;
                    conn.read_buf.clear();
                    
                    if self.output_queue.push(id).is_ok() {
                        metrics.inc_queue_len("route");
                    }
                }
                Err(ners_proto_http::ParseError::Incomplete) => {
                    if self.input_queue.push(id).is_ok() {
                        metrics.inc_queue_len("parse");
                    }
                }
                Err(_) => {
                    conn.state = ConnLifecycle::Closed;
                    if let Some(stage) = metrics.stage("parse") {
                        stage.inc_errors();
                    }
                }
            }
        }
    }
}

/// Multi-core Parse stage
pub struct ParseStageMulti {
    parser: ners_proto_http::HttpParser,
}

impl ParseStageMulti {
    pub fn new() -> Self {
        Self {
            parser: ners_proto_http::HttpParser::new(),
        }
    }
}

impl Default for ParseStageMulti {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableStage for ParseStageMulti {
    fn name(&self) -> &'static str {
        "parse"
    }

    fn process_one(
        &mut self,
        conn_id: ConnId,
        slab: &mut ConnSlab,
        _mux: &mut IoMultiplexer,
        metrics: &MetricsCollector,
    ) -> Option<ConnId> {
        let conn = slab.get_mut(conn_id)?;
        
        if let Some(ref mut stream) = conn.stream {
            let _ = read_all(stream, &mut conn.read_buf);
        }
        
        match self.parser.parse(&conn.read_buf) {
            Ok((request, _)) => {
                conn.request_method = Some(request.method);
                conn.request_path = Some(request.path);
                conn.state = ConnLifecycle::Routed;
                conn.read_buf.clear();
                Some(conn_id)
            }
            Err(ners_proto_http::ParseError::Incomplete) => {
                // Need more data - put back in queue
                None
            }
            Err(_) => {
                conn.state = ConnLifecycle::Closed;
                if let Some(stage) = metrics.stage("parse") {
                    stage.inc_errors();
                }
                None
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
            
            let route_id = match conn.request_path.as_deref() {
                Some("/") => RouteId(0),
                Some("/api/test") => RouteId(1),
                _ => RouteId(999),
            };
            
            conn.route_id = Some(route_id);
            conn.state = ConnLifecycle::Processing;
            
            if self.output_queue.push(id).is_ok() {
                metrics.inc_queue_len("app");
            }
        }
    }
}

/// Multi-core Route stage
pub struct RouteStageMulti;

impl RouteStageMulti {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RouteStageMulti {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableStage for RouteStageMulti {
    fn name(&self) -> &'static str {
        "route"
    }

    fn process_one(
        &mut self,
        conn_id: ConnId,
        slab: &mut ConnSlab,
        _mux: &mut IoMultiplexer,
        _metrics: &MetricsCollector,
    ) -> Option<ConnId> {
        let conn = slab.get_mut(conn_id)?;
        
        let route_id = match conn.request_path.as_deref() {
            Some("/") => RouteId(0),
            Some("/api/test") => RouteId(1),
            _ => RouteId(999),
        };
        
        conn.route_id = Some(route_id);
        conn.state = ConnLifecycle::Processing;
        Some(conn_id)
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

/// Multi-core App stage
pub struct AppStageMulti;

impl AppStageMulti {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AppStageMulti {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableStage for AppStageMulti {
    fn name(&self) -> &'static str {
        "app"
    }

    fn process_one(
        &mut self,
        conn_id: ConnId,
        slab: &mut ConnSlab,
        _mux: &mut IoMultiplexer,
        metrics: &MetricsCollector,
    ) -> Option<ConnId> {
        let conn = slab.get_mut(conn_id)?;
        
        match conn.route_id {
            Some(RouteId(0)) => handle_hello(conn),
            Some(RouteId(1)) => handle_json(conn),
            _ => handle_not_found(conn),
        }
        
        conn.state = ConnLifecycle::Sending;
        metrics.inc_total_requests();
        Some(conn_id)
    }
}

/// Encode Stage - currently a pass-through
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

    fn process(&mut self, _slab: &mut ConnSlab, _metrics: &MetricsCollector) {
        for _ in 0..100 {
            let id = match self.input_queue.pop() {
                Some(id) => id,
                None => break,
            };
            
            let _ = self.output_queue.push(id);
        }
    }
}

/// Multi-core Encode stage
pub struct EncodeStageMulti;

impl EncodeStageMulti {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EncodeStageMulti {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableStage for EncodeStageMulti {
    fn name(&self) -> &'static str {
        "encode"
    }

    fn process_one(
        &mut self,
        conn_id: ConnId,
        _slab: &mut ConnSlab,
        _mux: &mut IoMultiplexer,
        _metrics: &MetricsCollector,
    ) -> Option<ConnId> {
        // Pass-through
        Some(conn_id)
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
            
            if let Some(ref mut stream) = conn.stream {
                match write_all(stream, &conn.write_buf, conn.bytes_written) {
                    Ok(written) => {
                        conn.bytes_written += written;
                        
                        if conn.bytes_written >= conn.write_buf.len() {
                            conn.state = ConnLifecycle::Closed;
                            metrics.dec_total_conns();
                            slab.remove(id);
                        } else {
                            if self.input_queue.push(id).is_ok() {
                                metrics.inc_queue_len("net_out");
                            }
                        }
                    }
                    Err(_) => {
                        conn.state = ConnLifecycle::Closed;
                        metrics.dec_total_conns();
                        if let Some(stage) = metrics.stage("net_out") {
                            stage.inc_errors();
                        }
                        slab.remove(id);
                    }
                }
            } else {
                slab.remove(id);
            }
        }
    }
}

/// Multi-core NetOut stage
pub struct NetOutStageMulti;

impl NetOutStageMulti {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NetOutStageMulti {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableStage for NetOutStageMulti {
    fn name(&self) -> &'static str {
        "net_out"
    }

    fn process_one(
        &mut self,
        conn_id: ConnId,
        slab: &mut ConnSlab,
        _mux: &mut IoMultiplexer,
        metrics: &MetricsCollector,
    ) -> Option<ConnId> {
        let conn = slab.get_mut(conn_id)?;
        
        if let Some(ref mut stream) = conn.stream {
            match write_all(stream, &conn.write_buf, conn.bytes_written) {
                Ok(written) => {
                    conn.bytes_written += written;
                    
                    if conn.bytes_written >= conn.write_buf.len() {
                        conn.state = ConnLifecycle::Closed;
                        metrics.dec_total_conns();
                        slab.remove(conn_id);
                        return None;
                    }
                }
                Err(_) => {
                    conn.state = ConnLifecycle::Closed;
                    metrics.dec_total_conns();
                    if let Some(stage) = metrics.stage("net_out") {
                        stage.inc_errors();
                    }
                    slab.remove(conn_id);
                    return None;
                }
            }
        }
        None
    }
}
