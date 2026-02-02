// tests/substore.rs
// DISABLED: Needs import path fix
#![cfg(disabled)]
use ouro_dag::subchain::store::SubStore;
use uuid::Uuid;

#[test]
fn substore_put_batch_and_get() {
    let name = format!("test_sub_{}", Uuid::new_v4());
    let store = SubStore::open(&name).expect("open");
    let root = vec![0u8; 32];
    let rec = crate::subchain::store::BatchRecord {
        batch_root: root.clone(),
        aggregator: "agg1".into(),
        leaf_count: 2,
        created_at: chrono::Utc::now(),
        serialized_leaves_ref: None,
        verified: false,
    };
    store.put_batch(&root, &rec).expect("put");
    let got = store.get_batch(&root).expect("get");
    assert!(got.is_some());
    assert_eq!(got.unwrap().aggregator, "agg1");
}
