//! Lock-free queue for inter-stage communication
//!
//! Provides a bounded SPSC ring buffer for passing ConnId between stages.

use crate::conn::ConnId;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A bounded single-producer single-consumer ring buffer queue
pub struct RingQueue {
    /// Ring buffer storage
    buffer: Vec<AtomicUsize>,
    /// Capacity of the queue
    capacity: usize,
    /// Head index (for consumer)
    head: AtomicUsize,
    /// Tail index (for producer)
    tail: AtomicUsize,
}

impl RingQueue {
    /// Create a new ring queue with the given capacity
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(AtomicUsize::new(usize::MAX)); // MAX = empty slot
        }
        
        Self {
            buffer,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push a connection ID onto the queue
    /// Returns Err with the ID if the queue is full
    pub fn push(&self, id: ConnId) -> Result<(), ConnId> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % self.capacity;
        
        if next_tail == self.head.load(Ordering::Acquire) {
            return Err(id); // Queue is full
        }
        
        self.buffer[tail].store(id.0, Ordering::Release);
        self.tail.store(next_tail, Ordering::Release);
        
        Ok(())
    }

    /// Pop a connection ID from the queue
    /// Returns None if the queue is empty
    pub fn pop(&self) -> Option<ConnId> {
        let head = self.head.load(Ordering::Relaxed);
        
        if head == self.tail.load(Ordering::Acquire) {
            return None; // Queue is empty
        }
        
        let value = self.buffer[head].load(Ordering::Acquire);
        let next_head = (head + 1) % self.capacity;
        self.head.store(next_head, Ordering::Release);
        
        if value == usize::MAX {
            None
        } else {
            Some(ConnId(value))
        }
    }

    /// Get the current length of the queue
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        
        if tail >= head {
            tail - head
        } else {
            self.capacity - head + tail
        }
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let queue = RingQueue::new(4);
        
        assert!(queue.push(ConnId(1)).is_ok());
        assert!(queue.push(ConnId(2)).is_ok());
        
        assert_eq!(queue.pop(), Some(ConnId(1)));
        assert_eq!(queue.pop(), Some(ConnId(2)));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_full_queue() {
        let queue = RingQueue::new(3);
        
        assert!(queue.push(ConnId(1)).is_ok());
        assert!(queue.push(ConnId(2)).is_ok());
        assert!(queue.push(ConnId(3)).is_err()); // Full (capacity - 1 usable)
    }

    #[test]
    fn test_len() {
        let queue = RingQueue::new(4);
        
        assert_eq!(queue.len(), 0);
        queue.push(ConnId(1)).unwrap();
        assert_eq!(queue.len(), 1);
        queue.push(ConnId(2)).unwrap();
        assert_eq!(queue.len(), 2);
        queue.pop();
        assert_eq!(queue.len(), 1);
    }
}
