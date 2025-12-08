use std::io::{BufReader, Read};
use tracing::{debug, trace, warn};

use crate::modules::storage::content::chunking::rabin::IPFS_POLYNOMIAL;

use super::config::ChunkingConfig;
use super::types::{ChunkData, StreamingChunkError, StreamingChunkResult};

/// Default buffer reader capacity.
const DEFAULT_BUF_CAPACITY: usize = 64 * 1024; // 64KB

// ============================================================================
// StreamBuffer
// ============================================================================

/// Internal buffer for streaming chunkers.
///
/// Manages a sliding window of data from the input stream,
/// allowing chunk boundary detection without loading entire files.
pub struct StreamBuffer<R: Read> {
    reader: BufReader<R>,
    /// Data buffer
    buffer: Vec<u8>,
    /// Number of valid bytes currently in buffer
    len: usize,
    /// Start position of unconsumed data in buffer
    start: usize,
    /// Global byte offset at `start`
    offset: u64,
    /// Whether we've reached EOF
    pub(crate) eof: bool,
    /// Maximum single read size
    read_size: usize,
}

impl<R: Read> StreamBuffer<R> {
    /// Create a new stream buffer.
    ///
    /// # Arguments
    /// * `reader` - Source to read from
    /// * `capacity` - Buffer capacity (should be >= max_size + window_size)
    pub fn new(reader: R, capacity: usize) -> Self {
        debug!(capacity, "Creating stream buffer");
        Self {
            reader: BufReader::with_capacity(DEFAULT_BUF_CAPACITY, reader),
            buffer: vec![0u8; capacity],
            len: 0,
            start: 0,
            offset: 0,
            eof: false,
            read_size: capacity / 4,
        }
    }

    /// Get current global offset (position in input stream).
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Check if we've consumed all input.
    pub fn is_exhausted(&self) -> bool {
        self.eof && self.len == 0
    }

    /// Get a view of currently buffered data.
    pub fn data(&self) -> &[u8] {
        &self.buffer[self.start..self.start + self.len]
    }

    /// Ensure at least `min_bytes` are buffered (if available).
    ///
    /// Returns the number of bytes now available.
    pub fn fill(&mut self, min_bytes: usize) -> StreamingChunkResult<usize> {
        // Compact buffer if needed
        if self.start > 0 && self.start + self.len + self.read_size > self.buffer.len() {
            trace!(
                start = self.start,
                len = self.len,
                "Compacting stream buffer"
            );
            self.buffer
                .copy_within(self.start..self.start + self.len, 0);
            self.start = 0;
        }

        // Fill until we have enough or hit EOF
        while !self.eof && self.len < min_bytes {
            let read_start = self.start + self.len;
            let available = self.buffer.len() - read_start;
            let to_read = available.min(self.read_size);

            if to_read == 0 {
                break;
            }

            match self
                .reader
                .read(&mut self.buffer[read_start..read_start + to_read])
            {
                Ok(0) => {
                    debug!(offset = self.offset, len = self.len, "Reached EOF");
                    self.eof = true;
                }
                Ok(n) => {
                    trace!(bytes_read = n, "Read from stream");
                    self.len += n;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    warn!(error = %e, "Read error during chunking");
                    return Err(StreamingChunkError::Io(e));
                }
            }
        }

        Ok(self.len)
    }

    /// Consume `n` bytes from the buffer.
    ///
    /// Returns the consumed data as a Vec.
    pub fn consume(&mut self, n: usize) -> Vec<u8> {
        assert!(n <= self.len, "Cannot consume more than buffered");

        let data = self.buffer[self.start..self.start + n].to_vec();

        self.start += n;
        self.len -= n;
        self.offset += n as u64;

        trace!(
            bytes_consumed = n,
            new_offset = self.offset,
            remaining = self.len,
            "Consumed from buffer"
        );

        data
    }
}

// ============================================================================
// FixedStreamingIter
// ============================================================================

/// Streaming iterator for fixed-size chunking.
pub struct FixedStreamingIter<R: Read> {
    buffer: StreamBuffer<R>,
    chunk_size: usize,
}

impl<R: Read> FixedStreamingIter<R> {
    /// Create a new fixed-size streaming iterator.
    pub fn new(reader: R, config: &ChunkingConfig) -> Self {
        let capacity = config.target_size * 2;
        Self {
            buffer: StreamBuffer::new(reader, capacity),
            chunk_size: config.target_size,
        }
    }
}

impl<R: Read> Iterator for FixedStreamingIter<R> {
    type Item = StreamingChunkResult<ChunkData>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.is_exhausted() {
            return None;
        }

        match self.buffer.fill(self.chunk_size) {
            Ok(available) if available == 0 => None,
            Ok(available) => {
                let offset = self.buffer.offset();
                let size = available.min(self.chunk_size);
                let data = self.buffer.consume(size);
                Some(Ok(ChunkData::new(data, offset)))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// FastCdcStreamingIter
// ============================================================================

/// Streaming iterator for FastCDC chunking.
///
/// Uses internal buffering since the fastcdc crate requires slice input.
pub struct FastCdcStreamingIter<R: Read> {
    buffer: StreamBuffer<R>,
    config: ChunkingConfig,
}

impl<R: Read> FastCdcStreamingIter<R> {
    /// Create a new FastCDC streaming iterator.
    pub fn new(reader: R, config: ChunkingConfig) -> Self {
        let capacity = config.max_size * 2;
        Self {
            buffer: StreamBuffer::new(reader, capacity),
            config,
        }
    }

    fn find_boundary(&self, data: &[u8]) -> usize {
        use fastcdc::v2020::FastCDC;

        if data.is_empty() {
            return 0;
        }

        let chunker = FastCDC::new(
            data,
            self.config.min_size as u32,
            self.config.target_size as u32,
            self.config.max_size as u32,
        );

        if let Some(chunk) = chunker.into_iter().next() {
            chunk.length
        } else {
            data.len()
        }
    }
}

impl<R: Read> Iterator for FastCdcStreamingIter<R> {
    type Item = StreamingChunkResult<ChunkData>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.is_exhausted() {
            return None;
        }

        match self.buffer.fill(self.config.max_size) {
            Ok(available) if available == 0 => return None,
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }

        let offset = self.buffer.offset();
        let chunk_size = self.find_boundary(self.buffer.data());

        if chunk_size == 0 {
            return None;
        }

        let data = self.buffer.consume(chunk_size);
        Some(Ok(ChunkData::new(data, offset)))
    }
}

// ============================================================================
// RabinStreamingIter
// ============================================================================

/// Streaming iterator for Rabin fingerprint chunking.
pub struct RabinStreamingIter<R: Read> {
    buffer: StreamBuffer<R>,
    config: ChunkingConfig,
    mod_table: [u64; 256],
    out_table: [u64; 256],
    mask: u64,
}

impl<R: Read> RabinStreamingIter<R> {
    const WINDOW_SIZE: usize = 64;

    /// Create a new Rabin streaming iterator.
    pub fn new(reader: R, config: ChunkingConfig) -> Self {
        let capacity = config.max_size + Self::WINDOW_SIZE + 1024;
        let bits = (config.target_size as f64).log2().floor() as u32;
        let mask = (1u64 << bits) - 1;

        Self {
            buffer: StreamBuffer::new(reader, capacity),
            mod_table: Self::compute_mod_table(IPFS_POLYNOMIAL),
            out_table: Self::compute_out_table(IPFS_POLYNOMIAL),
            mask,
            config,
        }
    }

    fn compute_mod_table(polynomial: u64) -> [u64; 256] {
        let mut table = [0u64; 256];
        for i in 0..256 {
            let mut hash = (i as u64) << 55;
            for _ in 0..8 {
                if hash & (1 << 63) != 0 {
                    hash = (hash << 1) ^ polynomial;
                } else {
                    hash <<= 1;
                }
            }
            table[i] = hash;
        }
        table
    }

    fn compute_out_table(polynomial: u64) -> [u64; 256] {
        let mut table = [0u64; 256];
        for i in 0..256 {
            let mut hash = 0u64;
            hash = Self::update_hash_raw(hash, i as u8, polynomial);
            for _ in 1..Self::WINDOW_SIZE {
                hash = Self::update_hash_raw(hash, 0, polynomial);
            }
            table[i] = hash;
        }
        table
    }

    fn update_hash_raw(hash: u64, byte: u8, polynomial: u64) -> u64 {
        let mut h = hash;
        for i in (0..8).rev() {
            if h & (1 << 63) != 0 {
                h = (h << 1) ^ polynomial;
            } else {
                h <<= 1;
            }
            if byte & (1 << i) != 0 {
                h ^= polynomial;
            }
        }
        h
    }

    #[inline]
    fn update_hash(&self, hash: u64, byte: u8) -> u64 {
        let top = (hash >> 56) as usize;
        (hash << 8) ^ self.mod_table[top] ^ (byte as u64)
    }

    #[inline]
    fn slide_hash(&self, hash: u64, out_byte: u8, in_byte: u8) -> u64 {
        let h = hash ^ self.out_table[out_byte as usize];
        self.update_hash(h, in_byte)
    }

    #[inline]
    fn is_boundary(&self, hash: u64) -> bool {
        (hash & self.mask) == 0
    }

    fn find_boundary(&self, data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }

        let mut hash = 0u64;

        for (i, &byte) in data.iter().enumerate() {
            let chunk_len = i + 1;

            // Update rolling hash
            if i < Self::WINDOW_SIZE {
                hash = self.update_hash(hash, byte);
            } else {
                hash = self.slide_hash(hash, data[i - Self::WINDOW_SIZE], byte);
            }

            // Check for boundary
            if self.is_boundary(hash) && chunk_len >= self.config.min_size {
                return chunk_len;
            }

            // Hit max size
            if chunk_len >= self.config.max_size {
                return chunk_len;
            }
        }

        // No boundary found - if this is all remaining data, return it
        data.len()
    }
}

impl<R: Read> Iterator for RabinStreamingIter<R> {
    type Item = StreamingChunkResult<ChunkData>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.is_exhausted() {
            return None;
        }

        match self.buffer.fill(self.config.max_size) {
            Ok(available) if available == 0 => return None,
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }

        let offset = self.buffer.offset();
        let data = self.buffer.data();

        let chunk_size = self.find_boundary(data);

        if chunk_size == 0 {
            return None;
        }

        // At EOF, or boundary found, just emit the chunk.
        let chunk_data = self.buffer.consume(chunk_size);
        Some(Ok(ChunkData::new(chunk_data, offset)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn default_config() -> ChunkingConfig {
        ChunkingConfig::new(64, 256, 1024)
    }

    #[test]
    fn test_stream_buffer_basic() {
        let data = b"hello world";
        let cursor = Cursor::new(data.to_vec());
        let mut buffer = StreamBuffer::new(cursor, 1024);

        assert!(!buffer.is_exhausted());
        assert_eq!(buffer.offset(), 0);

        buffer.fill(5).unwrap();
        assert_eq!(buffer.data(), b"hello world");

        let consumed = buffer.consume(6);
        assert_eq!(consumed, b"hello ");
        assert_eq!(buffer.offset(), 6);
        assert_eq!(buffer.data(), b"world");
    }

    #[test]
    fn test_stream_buffer_eof() {
        let data = b"short";
        let cursor = Cursor::new(data.to_vec());
        let mut buffer = StreamBuffer::new(cursor, 1024);

        buffer.fill(1000).unwrap();
        assert_eq!(buffer.data().len(), 5);

        buffer.consume(5);
        assert!(buffer.is_exhausted());
    }

    #[test]
    fn test_fixed_streaming_iter_basic() {
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let config = ChunkingConfig::new(100, 100, 100);

        let cursor = Cursor::new(data.clone());
        let iter = FixedStreamingIter::new(cursor, &config);

        let chunks: Vec<_> = iter.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 10);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.offset, (i * 100) as u64);
            assert_eq!(chunk.size(), 100);
        }

        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_fixed_streaming_iter_uneven() {
        let data: Vec<u8> = (0..250).map(|i| (i % 256) as u8).collect();
        let config = ChunkingConfig::new(100, 100, 100);

        let cursor = Cursor::new(data.clone());
        let iter = FixedStreamingIter::new(cursor, &config);

        let chunks: Vec<_> = iter.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].size(), 100);
        assert_eq!(chunks[1].size(), 100);
        assert_eq!(chunks[2].size(), 50);

        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_fastcdc_streaming_iter() {
        let data: Vec<u8> = (0..10000).map(|i| ((i * 17) % 256) as u8).collect();
        let config = default_config();

        let cursor = Cursor::new(data.clone());
        let iter = FastCdcStreamingIter::new(cursor, config);

        let chunks: Vec<_> = iter.map(|r| r.unwrap()).collect();

        assert!(!chunks.is_empty());

        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_rabin_streaming_iter() {
        let data: Vec<u8> = (0..10000).map(|i| ((i * 17) % 256) as u8).collect();
        let config = default_config();

        let cursor = Cursor::new(data.clone());
        let iter = RabinStreamingIter::new(cursor, config);

        let chunks: Vec<_> = iter.map(|r| r.unwrap()).collect();

        assert!(!chunks.is_empty());

        let reconstructed: Vec<u8> = chunks.into_iter().flat_map(|c| c.data).collect();
        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_streaming_empty_input() {
        let config = default_config();

        let cursor = Cursor::new(Vec::<u8>::new());
        let chunks: Vec<_> = FixedStreamingIter::new(cursor, &config).collect();
        assert!(chunks.is_empty());

        let cursor = Cursor::new(Vec::<u8>::new());
        let chunks: Vec<_> = FastCdcStreamingIter::new(cursor, config.clone()).collect();
        assert!(chunks.is_empty());

        let cursor = Cursor::new(Vec::<u8>::new());
        let chunks: Vec<_> = RabinStreamingIter::new(cursor, config).collect();
        assert!(chunks.is_empty());
    }
}
