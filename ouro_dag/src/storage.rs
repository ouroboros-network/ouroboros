// src/storage.rs
// RocksDB-backed persistent storage

use once_cell::sync::OnceCell;
use parking_lot::Mutex as SyncMutex;
use rocksdb::{IteratorMode, Options, WriteBatch, DB};
use serde::{de::DeserializeOwned, Serialize};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

/// Per-key locks for atomic read-modify-write operations (e.g., counter increments).
/// Prevents TOCTOU race conditions that could enable double-spend or reward duplication.
static COUNTER_LOCKS: once_cell::sync::Lazy<SyncMutex<HashMap<String, Arc<SyncMutex<()>>>>> =
    once_cell::sync::Lazy::new(|| SyncMutex::new(HashMap::new()));

/// Type alias for RocksDB (Arc for cheap cloning)
pub type RocksDb = Arc<DB>;

/// Global storage instance (initialized once during startup)
static GLOBAL_DB: OnceCell<RocksDb> = OnceCell::new();

/// Initialize the global storage instance.
pub fn init_global_storage(db: RocksDb) {
    let _ = GLOBAL_DB.set(db);
}

/// Get the global storage instance.
pub fn get_global_storage() -> Option<RocksDb> {
    GLOBAL_DB.get().cloned()
}

/// Open RocksDB with optimized settings and retry/backoff
/// Returns Result instead of panicking on failure
pub fn open_db(path: &str) -> RocksDb {
    // Wrapper that panics for backward compatibility - use try_open_db for Result
    try_open_db(path).unwrap_or_else(|e| {
        eprintln!(" FATAL: Failed to open database at '{}': {}", path, e);
        eprintln!("   Possible causes:");
        eprintln!("   - Another node instance is running (database locked)");
        eprintln!("   - Disk is full or permissions denied");
        eprintln!("   - Database is corrupted (try removing {})", path);
        std::process::exit(1);
    })
}

/// Try to open RocksDB, returning Result for graceful error handling
pub fn try_open_db(path: &str) -> Result<RocksDb, String> {
    let mut attempt = 0u32;
    let max_attempts = 8u32;
    let mut wait = 250u64;

    loop {
        match open_rocksdb_internal(path) {
            Ok(db) => return Ok(Arc::new(db)),
            Err(e) => {
                attempt += 1;
                if attempt >= max_attempts {
                    return Err(format!(
                        "Failed to open RocksDB at '{}' after {} attempts: {}",
                        path, attempt, e
                    ));
                }
                eprintln!(
                    "open_db attempt {}/{} failed: {} — retrying in {}ms",
                    attempt, max_attempts, e, wait
                );
                sleep(Duration::from_millis(wait));
                wait = std::cmp::min(wait * 2, 2000);
            }
        }
    }
}

/// Internal RocksDB opener with optimized settings
fn open_rocksdb_internal(path: &str) -> Result<DB, rocksdb::Error> {
    let mut opts = Options::default();
    opts.create_if_missing(true);

    // Performance optimizations
    let num_cpus = num_cpus::get() as i32;
    opts.increase_parallelism(num_cpus);
    opts.set_max_background_jobs(4);
    opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
    opts.set_max_total_wal_size(512 * 1024 * 1024); // 512MB
    opts.set_level_zero_file_num_compaction_trigger(8);
    opts.set_max_open_files(512); // Reduced to prevent fd exhaustion

    // Compression
    opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

    DB::open(&opts, path)
}

/// Put a serializable value under a byte-key.
pub fn put<K: AsRef<[u8]>, V: Serialize>(db: &RocksDb, key: K, val: &V) -> Result<(), String> {
    let bytes = serde_json::to_vec(val).map_err(|e| e.to_string())?;
    db.put(key, bytes).map_err(|e| e.to_string())?;
    Ok(())
}

/// Get and deserialize a value stored under a byte-key.
pub fn get<K: AsRef<[u8]>, T: DeserializeOwned>(db: &RocksDb, key: K) -> Result<Option<T>, String> {
    match db.get(key).map_err(|e| e.to_string())? {
        Some(bytes) => {
            let v = serde_json::from_slice::<T>(&bytes).map_err(|e| e.to_string())?;
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

/// Delete a key
pub fn delete<K: AsRef<[u8]>>(db: &RocksDb, key: K) -> Result<(), String> {
    db.delete(key).map_err(|e| e.to_string())?;
    Ok(())
}

/// Batch put: apply multiple (key, value) entries atomically.
pub fn batch_put<V: Serialize>(db: &RocksDb, entries: Vec<(Vec<u8>, V)>) -> Result<(), String> {
    let mut batch = WriteBatch::default();
    for (k, v) in entries.into_iter() {
        let b = serde_json::to_vec(&v).map_err(|e| e.to_string())?;
        batch.put(k, b);
    }
    db.write(batch).map_err(|e| e.to_string())?;
    Ok(())
}

/// Iterate values whose keys start with the given prefix and deserialize them into Vec<T>.
pub fn iter_prefix<T: DeserializeOwned>(db: &RocksDb, prefix: &[u8]) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    let iter = db.prefix_iterator(prefix);

    for item in iter {
        let (k, v) = item.map_err(|e| e.to_string())?;

        // Check if key still has the prefix
        if !k.starts_with(prefix) {
            break;
        }

        let obj = serde_json::from_slice::<T>(&v).map_err(|e| e.to_string())?;
        out.push(obj);
    }
    Ok(out)
}

/// Iterate prefix returning (key_string, value) pairs
pub fn iter_prefix_kv<T: DeserializeOwned>(
    db: &RocksDb,
    prefix: &str,
) -> Result<Vec<(String, T)>, String> {
    let mut out = Vec::new();
    let prefix_bytes = prefix.as_bytes();
    let iter = db.prefix_iterator(prefix_bytes);

    for item in iter {
        let (k, v) = item.map_err(|e| e.to_string())?;

        // Check if key still has the prefix
        if !k.starts_with(prefix_bytes) {
            break;
        }

        let kstr = String::from_utf8_lossy(&k).to_string();
        let obj = serde_json::from_slice::<T>(&v).map_err(|e| e.to_string())?;
        out.push((kstr, obj));
    }
    Ok(out)
}

/// Convenience helpers (string-key versions)
pub fn put_str<V: Serialize>(db: &RocksDb, key: &str, val: &V) -> Result<(), String> {
    put(db, key.as_bytes(), val)
}

pub fn get_str<T: DeserializeOwned>(db: &RocksDb, key: &str) -> Result<Option<T>, String> {
    get(db, key.as_bytes())
}

pub fn delete_str(db: &RocksDb, key: &str) -> Result<(), String> {
    delete(db, key.as_bytes())
}

/// Check if a key exists
pub fn exists(db: &RocksDb, key: &str) -> bool {
    db.get(key.as_bytes()).ok().flatten().is_some()
}

/// Count entries with a given prefix
pub fn count_prefix(db: &RocksDb, prefix: &str) -> usize {
    let prefix_bytes = prefix.as_bytes();
    let iter = db.prefix_iterator(prefix_bytes);
    let mut count = 0;

    for item in iter {
        if let Ok((k, _)) = item {
            if !k.starts_with(prefix_bytes) {
                break;
            }
            count += 1;
        }
    }
    count
}

/// Get values with prefix, with limit and offset
pub fn iter_prefix_limited<T: DeserializeOwned>(
    db: &RocksDb,
    prefix: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<T>, String> {
    let prefix_bytes = prefix.as_bytes();
    let iter = db.prefix_iterator(prefix_bytes);
    let mut results = Vec::new();

    for (idx, item) in iter.enumerate() {
        let (k, v) = item.map_err(|e| e.to_string())?;

        // Check if key still has the prefix
        if !k.starts_with(prefix_bytes) {
            break;
        }

        if idx < offset {
            continue;
        }
        if results.len() >= limit {
            break;
        }

        let obj = serde_json::from_slice::<T>(&v).map_err(|e| e.to_string())?;
        results.push(obj);
    }

    Ok(results)
}

/// Store a raw u64 counter
pub fn put_counter(db: &RocksDb, key: &str, value: u64) -> Result<(), String> {
    db.put(key.as_bytes(), &value.to_le_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Get a u64 counter
pub fn get_counter(db: &RocksDb, key: &str) -> Result<u64, String> {
    match db.get(key.as_bytes()).map_err(|e| e.to_string())? {
        Some(bytes) => {
            let arr: [u8; 8] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| "Invalid counter bytes".to_string())?;
            Ok(u64::from_le_bytes(arr))
        }
        None => Ok(0),
    }
}

/// Increment a counter atomically using per-key locking.
/// Prevents TOCTOU race conditions on concurrent read-modify-write.
pub fn increment_counter(db: &RocksDb, key: &str, amount: u64) -> Result<u64, String> {
    // Acquire a per-key lock to make the read-modify-write atomic
    let lock = {
        let mut locks = COUNTER_LOCKS.lock();
        locks.entry(key.to_string()).or_insert_with(|| Arc::new(SyncMutex::new(()))).clone()
    };
    let _guard = lock.lock();

    let current = get_counter(db, key)?;
    let new_value = current.saturating_add(amount);
    put_counter(db, key, new_value)?;
    Ok(new_value)
}
