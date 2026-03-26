use hd_cas::store::ContentStore;
use hd_cas::gc::GarbageCollector;
use tempfile::TempDir;
use std::fs;

#[test]
fn full_lifecycle_put_file_gc() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let gc = GarbageCollector::new(dir.path()).unwrap();

    // Create and ingest two files with some shared content
    let file1_path = dir.path().join("file1.txt");
    let file2_path = dir.path().join("file2.txt");
    fs::write(&file1_path, b"shared prefix - unique suffix one").unwrap();
    fs::write(&file2_path, b"shared prefix - unique suffix two").unwrap();

    let mh1 = store.put_file(&file1_path).unwrap();
    let mh2 = store.put_file(&file2_path).unwrap();
    assert_ne!(mh1, mh2);

    // Reference only file1
    gc.add_ref(&mh1).unwrap();

    // GC should remove file2's manifest (and any unique chunks)
    let stats = gc.collect(&store).unwrap();
    assert_eq!(stats.manifests_removed, 1);

    // file1 should still be retrievable
    let out_path = dir.path().join("recovered.txt");
    store.get_file(&mh1, &out_path).unwrap();
    assert_eq!(fs::read(&out_path).unwrap(), b"shared prefix - unique suffix one");

    // file2's manifest should be gone
    assert!(store.get_manifest(&mh2).is_err());
}

#[test]
fn dedup_across_identical_files() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();

    let content = b"identical content in both files";
    let f1 = dir.path().join("a.txt");
    let f2 = dir.path().join("b.txt");
    fs::write(&f1, content).unwrap();
    fs::write(&f2, content).unwrap();

    let mh1 = store.put_file(&f1).unwrap();
    let mh2 = store.put_file(&f2).unwrap();

    // Same content, same mode → same manifest hash
    assert_eq!(mh1, mh2);

    // Only one set of chunks should exist
    let chunks = store.list_chunks().unwrap();
    assert_eq!(chunks.len(), 1);
}

#[test]
fn large_file_roundtrip_integrity() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();

    // 1MB of pseudo-random data
    let data: Vec<u8> = (0..1_000_000u64)
        .map(|i| ((i.wrapping_mul(6364136223846793005).wrapping_add(1)) >> 33) as u8)
        .collect();

    let file_path = dir.path().join("large.bin");
    fs::write(&file_path, &data).unwrap();

    let mhash = store.put_file(&file_path).unwrap();

    let manifest = store.get_manifest(&mhash).unwrap();
    assert!(manifest.chunks.len() > 1, "should have multiple chunks");
    assert_eq!(manifest.size, 1_000_000);

    let out_path = dir.path().join("large_out.bin");
    store.get_file(&mhash, &out_path).unwrap();
    assert_eq!(fs::read(&out_path).unwrap(), data);
}
