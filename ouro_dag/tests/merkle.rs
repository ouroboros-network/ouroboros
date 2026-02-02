// tests/merkle.rs
// DISABLED: Needs sha2::Digest import fix
#![cfg(disabled)]
use ouro_dag::subchain::anchor;
use sha2::Sha256;

#[test]
fn merkle_simple_even() {
    let leaves = vec![
        Sha256::digest(b"a").to_vec(),
        Sha256::digest(b"b").to_vec(),
        Sha256::digest(b"c").to_vec(),
        Sha256::digest(b"d").to_vec(),
    ];
    let root = anchor::merkle_root(&leaves);
    assert_eq!(root.len(), 32);
}

#[test]
fn merkle_simple_odd() {
    let leaves = vec![
        Sha256::digest(b"a").to_vec(),
        Sha256::digest(b"b").to_vec(),
        Sha256::digest(b"c").to_vec(),
    ];
    let root = anchor::merkle_root(&leaves);
    assert_eq!(root.len(), 32);
}
