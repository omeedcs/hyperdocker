use fastcdc::v2020::FastCDC;

pub const MIN_CHUNK_SIZE: usize = 4 * 1024;      // 4 KB
pub const TARGET_CHUNK_SIZE: usize = 16 * 1024;  // 16 KB
pub const MAX_CHUNK_SIZE: usize = 64 * 1024;     // 64 KB

/// Split data into content-defined chunks using FastCDC.
/// Files smaller than MIN_CHUNK_SIZE are returned as a single chunk.
/// Empty data returns an empty vec.
pub fn chunk_data(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    if data.len() <= MIN_CHUNK_SIZE {
        return vec![data];
    }
    let chunker = FastCDC::new(
        data,
        MIN_CHUNK_SIZE as u32,
        TARGET_CHUNK_SIZE as u32,
        MAX_CHUNK_SIZE as u32,
    );
    chunker
        .map(|chunk| &data[chunk.offset..chunk.offset + chunk.length])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_small_file_single_chunk() {
        let data = b"small file content";
        let chunks = chunk_data(data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data.as_slice());
    }

    #[test]
    fn chunk_empty_data() {
        let chunks = chunk_data(b"");
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_large_file_multiple_chunks() {
        // 256KB of data should produce multiple chunks with 16KB target
        let data = vec![0xAB; 256 * 1024];
        let chunks = chunk_data(&data);
        assert!(chunks.len() > 1, "expected multiple chunks, got {}", chunks.len());
        // all chunks within size bounds
        for chunk in &chunks {
            assert!(chunk.len() <= MAX_CHUNK_SIZE);
        }
    }

    #[test]
    fn chunk_reassembly() {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let chunks = chunk_data(&data);
        let reassembled: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn chunk_deterministic() {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let chunks1 = chunk_data(&data);
        let chunks2 = chunk_data(&data);
        assert_eq!(chunks1.len(), chunks2.len());
        for (a, b) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(a, b);
        }
    }
}
