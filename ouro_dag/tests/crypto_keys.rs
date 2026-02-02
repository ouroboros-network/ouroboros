// tests/crypto_keys.rs
use ouro_dag::crypto::keys;

#[test]
fn key_sign_verify() {
    // Generate a 32-byte seed (in practice, use a secure random source)
    let seed: [u8; 32] = [42u8; 32]; // Test seed

    // Derive public key from seed
    let pubkey = keys::public_from_seed(&seed).expect("should derive pubkey");
    assert_eq!(pubkey.len(), 32, "pubkey should be 32 bytes");

    // Sign a message
    let msg = b"hello ouro";
    let sig = keys::sign_bytes(&seed, msg).expect("should sign");
    assert_eq!(sig.len(), 64, "signature should be 64 bytes");

    // Verify the signature
    let ok = keys::verify_bytes(&pubkey, msg, &sig);
    assert!(ok, "signature should verify");

    // Verify with wrong message should fail
    let wrong_msg = b"wrong message";
    let ok_wrong = keys::verify_bytes(&pubkey, wrong_msg, &sig);
    assert!(!ok_wrong, "signature should not verify with wrong message");

    // Verify with wrong pubkey should fail
    let wrong_pubkey: [u8; 32] = [99u8; 32];
    let ok_wrong_key = keys::verify_bytes(&wrong_pubkey, msg, &sig);
    assert!(
        !ok_wrong_key,
        "signature should not verify with wrong pubkey"
    );
}
