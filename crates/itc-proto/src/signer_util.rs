//! Shared crypto helpers used by itc-proto (script analysis) and the oracle.

use sha2::{Digest, Sha256};
use sha3::Keccak256;

/// Bitcoin double-SHA256.
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(data)).into()
}

/// Keccak-256 (Ethereum address derivation from secp256k1 pubkeys).
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    Keccak256::digest(data).into()
}

/// Bitcoin hash160: RIPEMD160(SHA256(pubkey)).
/// Used to derive the hash160 from a known pubkey so we can match UTXOs.
pub fn hash160_from_pubkey(pubkey: &[u8]) -> [u8; 20] {
    use ripemd::Ripemd160;
    let sha = Sha256::digest(pubkey);
    Ripemd160::digest(sha).into()
}
