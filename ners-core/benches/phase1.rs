//! Phase 1 Benchmarks
//!
//! Measures throughput and latency for the NERS server.

use criterion::{criterion_group, criterion_main, Criterion};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn benchmark_hello_world(c: &mut Criterion) {
    // Note: Server must be running externally for this benchmark
    // In a real setup, we'd spawn the server in a background thread
    
    c.bench_function("sequential_requests", |b| {
        b.iter(|| {
            if let Ok(mut stream) = TcpStream::connect("127.0.0.1:8080") {
                stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                stream.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                
                let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
                let _ = stream.write_all(request);
                
                let mut response = [0u8; 1024];
                let _ = stream.read(&mut response);
            }
        });
    });
}

fn benchmark_json_endpoint(c: &mut Criterion) {
    c.bench_function("json_requests", |b| {
        b.iter(|| {
            if let Ok(mut stream) = TcpStream::connect("127.0.0.1:8080") {
                stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                stream.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                
                let request = b"GET /api/test HTTP/1.1\r\nHost: localhost\r\n\r\n";
                let _ = stream.write_all(request);
                
                let mut response = [0u8; 1024];
                let _ = stream.read(&mut response);
            }
        });
    });
}

criterion_group!(benches, benchmark_hello_world, benchmark_json_endpoint);
criterion_main!(benches);
