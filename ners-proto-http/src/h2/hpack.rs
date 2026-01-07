//! HPACK Header Compression
//!
//! Implements RFC 7541 for HTTP/2 header compression.

use bytes::{BufMut, Bytes, BytesMut};
use std::collections::VecDeque;

/// Static table for HPACK (first 61 entries)
const STATIC_TABLE: &[(&str, &str)] = &[
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

/// Dynamic table entry
#[derive(Debug, Clone)]
struct DynamicEntry {
    name: String,
    value: String,
    size: usize,
}

impl DynamicEntry {
    fn new(name: String, value: String) -> Self {
        let size = 32 + name.len() + value.len();
        Self { name, value, size }
    }
}

/// HPACK Decoder
pub struct HpackDecoder {
    dynamic_table: VecDeque<DynamicEntry>,
    max_size: usize,
    current_size: usize,
}

impl HpackDecoder {
    /// Create a new HPACK decoder
    pub fn new(max_size: usize) -> Self {
        Self {
            dynamic_table: VecDeque::new(),
            max_size,
            current_size: 0,
        }
    }

    /// Decode a header block
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<(String, String)>, String> {
        let mut headers = Vec::new();
        let mut pos = 0;
        
        while pos < data.len() {
            let first = data[pos];
            
            if first & 0x80 != 0 {
                // Indexed Header Field
                let (index, consumed) = self.decode_integer(&data[pos..], 7)?;
                pos += consumed;
                
                let (name, value) = self.get_indexed(index)?;
                headers.push((name, value));
            } else if first & 0x40 != 0 {
                // Literal Header Field with Incremental Indexing
                let (name, value, consumed) = self.decode_literal(&data[pos..], 6, true)?;
                pos += consumed;
                headers.push((name, value));
            } else if first & 0x20 != 0 {
                // Dynamic Table Size Update
                let (new_size, consumed) = self.decode_integer(&data[pos..], 5)?;
                pos += consumed;
                self.resize(new_size);
            } else {
                // Literal Header Field without Indexing or Never Indexed
                let prefix = if first & 0x10 != 0 { 4 } else { 4 };
                let (name, value, consumed) = self.decode_literal(&data[pos..], prefix, false)?;
                pos += consumed;
                headers.push((name, value));
            }
        }
        
        Ok(headers)
    }

    fn decode_integer(&self, data: &[u8], prefix: usize) -> Result<(usize, usize), String> {
        if data.is_empty() {
            return Err("Empty data for integer".to_string());
        }
        
        let mask = (1 << prefix) - 1;
        let mut value = (data[0] & mask) as usize;
        
        if value < mask as usize {
            return Ok((value, 1));
        }
        
        let mut m = 0usize;
        let mut pos = 1;
        
        loop {
            if pos >= data.len() {
                return Err("Truncated integer".to_string());
            }
            
            let b = data[pos] as usize;
            value += (b & 0x7f) << m;
            m += 7;
            pos += 1;
            
            if b & 0x80 == 0 {
                break;
            }
        }
        
        Ok((value, pos))
    }

    fn decode_literal(
        &mut self,
        data: &[u8],
        prefix: usize,
        index: bool,
    ) -> Result<(String, String, usize), String> {
        let (name_index, mut pos) = self.decode_integer(data, prefix)?;
        
        let name = if name_index == 0 {
            let (n, consumed) = self.decode_string(&data[pos..])?;
            pos += consumed;
            n
        } else {
            self.get_indexed(name_index)?.0
        };
        
        let (value, consumed) = self.decode_string(&data[pos..])?;
        pos += consumed;
        
        if index {
            self.add_entry(name.clone(), value.clone());
        }
        
        Ok((name, value, pos))
    }

    fn decode_string(&self, data: &[u8]) -> Result<(String, usize), String> {
        if data.is_empty() {
            return Err("Empty string data".to_string());
        }
        
        let huffman = data[0] & 0x80 != 0;
        let (len, mut pos) = self.decode_integer(data, 7)?;
        
        if pos + len > data.len() {
            return Err("String too long".to_string());
        }
        
        let bytes = &data[pos..pos + len];
        pos += len;
        
        let s = if huffman {
            // Simplified: just use raw bytes for now
            String::from_utf8_lossy(bytes).to_string()
        } else {
            String::from_utf8_lossy(bytes).to_string()
        };
        
        Ok((s, pos))
    }

    fn get_indexed(&self, index: usize) -> Result<(String, String), String> {
        if index == 0 {
            return Err("Invalid index 0".to_string());
        }
        
        if index <= STATIC_TABLE.len() {
            let (name, value) = STATIC_TABLE[index - 1];
            return Ok((name.to_string(), value.to_string()));
        }
        
        let dyn_index = index - STATIC_TABLE.len() - 1;
        if dyn_index >= self.dynamic_table.len() {
            return Err(format!("Invalid dynamic index {}", dyn_index));
        }
        
        let entry = &self.dynamic_table[dyn_index];
        Ok((entry.name.clone(), entry.value.clone()))
    }

    fn add_entry(&mut self, name: String, value: String) {
        let entry = DynamicEntry::new(name, value);
        let entry_size = entry.size;
        
        // Evict entries if needed
        while self.current_size + entry_size > self.max_size && !self.dynamic_table.is_empty() {
            if let Some(removed) = self.dynamic_table.pop_back() {
                self.current_size -= removed.size;
            }
        }
        
        if entry_size <= self.max_size {
            self.current_size += entry_size;
            self.dynamic_table.push_front(entry);
        }
    }

    fn resize(&mut self, new_size: usize) {
        self.max_size = new_size;
        while self.current_size > self.max_size && !self.dynamic_table.is_empty() {
            if let Some(removed) = self.dynamic_table.pop_back() {
                self.current_size -= removed.size;
            }
        }
    }
}

impl Default for HpackDecoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

/// HPACK Encoder
pub struct HpackEncoder {
    dynamic_table: VecDeque<DynamicEntry>,
    max_size: usize,
    current_size: usize,
}

impl HpackEncoder {
    /// Create a new HPACK encoder
    pub fn new(max_size: usize) -> Self {
        Self {
            dynamic_table: VecDeque::new(),
            max_size,
            current_size: 0,
        }
    }

    /// Encode headers to a byte buffer
    pub fn encode(&mut self, headers: &[(String, String)]) -> Bytes {
        let mut buf = BytesMut::new();
        
        for (name, value) in headers {
            // Try to find in static table
            if let Some((index, has_value)) = self.find_static(name, value) {
                if has_value {
                    // Indexed Header Field
                    self.encode_integer(&mut buf, index, 7, 0x80);
                } else {
                    // Literal with Incremental Indexing, name from index
                    self.encode_integer(&mut buf, index, 6, 0x40);
                    self.encode_string(&mut buf, value);
                    self.add_entry(name.clone(), value.clone());
                }
            } else {
                // Literal with Incremental Indexing, new name
                buf.put_u8(0x40);
                self.encode_string(&mut buf, name);
                self.encode_string(&mut buf, value);
                self.add_entry(name.clone(), value.clone());
            }
        }
        
        buf.freeze()
    }

    fn find_static(&self, name: &str, value: &str) -> Option<(usize, bool)> {
        for (i, (n, v)) in STATIC_TABLE.iter().enumerate() {
            if *n == name {
                if *v == value {
                    return Some((i + 1, true));
                }
                return Some((i + 1, false));
            }
        }
        None
    }

    fn encode_integer(&self, buf: &mut BytesMut, value: usize, prefix: usize, first_byte: u8) {
        let max_prefix = (1 << prefix) - 1;
        
        if value < max_prefix {
            buf.put_u8(first_byte | value as u8);
        } else {
            buf.put_u8(first_byte | max_prefix as u8);
            let mut remaining = value - max_prefix;
            while remaining >= 128 {
                buf.put_u8((remaining & 0x7f | 0x80) as u8);
                remaining >>= 7;
            }
            buf.put_u8(remaining as u8);
        }
    }

    fn encode_string(&self, buf: &mut BytesMut, s: &str) {
        let bytes = s.as_bytes();
        self.encode_integer(buf, bytes.len(), 7, 0);
        buf.extend_from_slice(bytes);
    }

    fn add_entry(&mut self, name: String, value: String) {
        let entry = DynamicEntry::new(name, value);
        let entry_size = entry.size;
        
        while self.current_size + entry_size > self.max_size && !self.dynamic_table.is_empty() {
            if let Some(removed) = self.dynamic_table.pop_back() {
                self.current_size -= removed.size;
            }
        }
        
        if entry_size <= self.max_size {
            self.current_size += entry_size;
            self.dynamic_table.push_front(entry);
        }
    }
}

impl Default for HpackEncoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_table_lookup() {
        let decoder = HpackDecoder::new(4096);
        let (name, value) = decoder.get_indexed(2).unwrap();
        assert_eq!(name, ":method");
        assert_eq!(value, "GET");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut encoder = HpackEncoder::new(4096);
        let mut decoder = HpackDecoder::new(4096);
        
        let headers = vec![
            (":method".to_string(), "GET".to_string()),
            (":path".to_string(), "/test".to_string()),
            ("content-type".to_string(), "text/html".to_string()),
        ];
        
        let encoded = encoder.encode(&headers);
        let decoded = decoder.decode(&encoded).unwrap();
        
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0].0, ":method");
        assert_eq!(decoded[0].1, "GET");
    }
}
