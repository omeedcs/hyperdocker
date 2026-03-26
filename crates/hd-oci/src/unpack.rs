// OCI layer unpacking into CAS.

use std::io::Read;

use hd_cas::ContentHash;
use hd_cas::ContentStore;

#[derive(Debug, Clone)]
pub struct UnpackedEntry {
    pub path: String,
    pub manifest_hash: ContentHash,
    pub mode: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("tar error: {0}")]
    Tar(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(#[from] hd_cas::store::StoreError),
}

/// Unpack an OCI layer (tar or tar+gzip) into the CAS.
/// Returns a list of unpacked file entries with their CAS manifest hashes.
pub fn unpack_layer(
    data: &[u8],
    store: &ContentStore,
    gzipped: bool,
) -> Result<Vec<UnpackedEntry>, UnpackError> {
    let reader: Box<dyn Read> = if gzipped {
        Box::new(flate2::read::GzDecoder::new(data))
    } else {
        Box::new(data)
    };

    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();

    for entry_result in archive
        .entries()
        .map_err(|e| UnpackError::Tar(e.to_string()))?
    {
        let mut entry = entry_result.map_err(|e| UnpackError::Tar(e.to_string()))?;

        // Skip directories and non-regular files
        if entry.header().entry_type() != tar::EntryType::Regular {
            continue;
        }

        let path = entry
            .path()
            .map_err(|e| UnpackError::Tar(e.to_string()))?
            .to_string_lossy()
            .to_string();

        let mode = entry.header().mode().unwrap_or(0o644);

        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;

        let manifest_hash = store.put_file_from_bytes(&content, mode)?;

        entries.push(UnpackedEntry {
            path,
            manifest_hash,
            mode,
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentStore;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_tar() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let data = b"hello world";
        let mut header = tar::Header::new_gnu();
        header.set_path("test.txt").unwrap();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &data[..]).unwrap();
        builder.into_inner().unwrap()
    }

    fn create_test_targz() -> Vec<u8> {
        let tar_data = create_test_tar();
        let mut encoder =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn unpack_tar_into_cas() {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();
        let tar_data = create_test_tar();

        let entries = unpack_layer(&tar_data, &store, false).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");

        // Verify content is in CAS
        let out_path = dir.path().join("recovered.txt");
        store
            .get_file(&entries[0].manifest_hash, &out_path)
            .unwrap();
        assert_eq!(std::fs::read(&out_path).unwrap(), b"hello world");
    }

    #[test]
    fn unpack_targz_into_cas() {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();
        let targz_data = create_test_targz();

        let entries = unpack_layer(&targz_data, &store, true).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.txt");
    }

    #[test]
    fn unpack_multiple_files() {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();

        let mut builder = tar::Builder::new(Vec::new());
        for name in &["a.txt", "b.txt", "c.txt"] {
            let data = name.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_path(name).unwrap();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, data).unwrap();
        }
        let tar_data = builder.into_inner().unwrap();

        let entries = unpack_layer(&tar_data, &store, false).unwrap();
        assert_eq!(entries.len(), 3);
    }
}
