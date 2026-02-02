// tests/microstore.rs
use ouro_dag::microchain::store::MicroStore;
use uuid::Uuid;

#[test]
fn microstore_put_get_tip() {
    let name = format!("test_micro_{}", Uuid::new_v4());
    let store = MicroStore::open(&name).expect("open microstore");
    // create header
    let hdr = ouro_dag::microchain::store::MicroHeader {
        id: Uuid::new_v4(),
        height: 1,
        timestamp: chrono::Utc::now(),
        leaf_hash: vec![1, 2, 3],
    };
    store.put_header(&hdr).expect("put");
    let tip = store.tip().expect("tip");
    assert!(tip.is_some());
    let t = tip.unwrap();
    assert_eq!(t.height, 1);
}
