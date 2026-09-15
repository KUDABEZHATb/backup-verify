//! License verification against signed receipts from the backup-verify
//! license server (see `activation.rs` for the HTTP side).
//!
//! This used to be a fully offline, symmetric (HMAC) scheme: the same
//! secret that verified a key had to be embedded in every copy of the app,
//! which meant (a) it was extractable by anyone willing to reverse-engineer
//! the binary, and (b) it briefly sat in plaintext in this very file,
//! readable by anyone with repo access, no reverse-engineering needed at
//! all. Worse, license *status* was only ever checked by asking "does a row
//! exist in the local SQLite database" — never re-verified — so a garbage
//! row inserted with any free DB browser passed as licensed forever.
//!
//! The fix is asymmetric and online: the server holds the only signing key
//! (Ed25519) and is the sole source of truth for which keys exist and are
//! active. This file embeds only the server's PUBLIC key, which can verify
//! a receipt but never forge one — reading this source, or the server's,
//! no longer hands anyone a way to issue themselves a license. A receipt is
//! re-verified from scratch (signature + expiry) on every read of license
//! status; nothing about a stored row is ever trusted on its own.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Safe to publish — verify-only, can't be used to mint a receipt.
const SERVER_PUBLIC_KEY: [u8; 32] = [
    27, 193, 234, 250, 122, 182, 200, 162, 50, 118, 128, 9, 117, 118, 24, 122, 9, 230, 204, 208,
    250, 179, 6, 235, 178, 219, 145, 188, 56, 210, 80, 140,
];

pub const FREE_TIER_BACKUP_LIMIT: i64 = 1;

/// The signed payload the server issues on activation/revalidation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Receipt {
    pub key: String,
    pub tier: String,
    pub max_backups: i64,
    pub machine_id: String,
    pub issued_at: String,
    pub expires_at: String,
}

/// What a verified, still-valid receipt means for the UI/enforcement — the
/// only thing callers outside this module ever get to see or trust.
#[derive(Debug, Clone, Serialize)]
pub struct LicenseInfo {
    pub tier: String,
    /// Short reference for support requests — the key itself, not a secret.
    pub license_ref: String,
    pub max_backups: i64,
    pub expires_at: String,
}

/// How long before a receipt's real expiry the scheduler starts trying to
/// revalidate it (see `scheduler.rs`). Receipts live 30 days server-side;
/// this gives a week of headroom so a stretch offline doesn't lock a paying
/// customer out of their own backups.
const REVALIDATE_WINDOW_DAYS: i64 = 7;

pub fn needs_revalidation(expires_at: &str) -> bool {
    match chrono::DateTime::parse_from_rfc3339(expires_at) {
        Ok(expires_at) => {
            expires_at < chrono::Utc::now() + chrono::Duration::days(REVALIDATE_WINDOW_DAYS)
        }
        Err(_) => true,
    }
}

/// Verifies the Ed25519 signature over the raw (base64-decoded) receipt
/// bytes, then parses it. This is the only path by which `Receipt` fields
/// are ever trusted — a tampered byte anywhere fails the signature check
/// before the JSON is even looked at.
pub fn verify_receipt(receipt_b64: &str, signature_b64: &str) -> Result<Receipt, String> {
    let verifying_key = VerifyingKey::from_bytes(&SERVER_PUBLIC_KEY)
        .expect("SERVER_PUBLIC_KEY is a valid, hardcoded Ed25519 key");
    verify_receipt_with_key(receipt_b64, signature_b64, &verifying_key)
}

/// Split out from `verify_receipt` purely so tests can exercise the full
/// decode/verify/parse path with a throwaway keypair instead of the real
/// production public key — there is no legitimate reason for any other
/// caller to use this directly.
fn verify_receipt_with_key(
    receipt_b64: &str,
    signature_b64: &str,
    verifying_key: &VerifyingKey,
) -> Result<Receipt, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let receipt_bytes = STANDARD.decode(receipt_b64).map_err(|_| "corrupt receipt".to_string())?;
    let sig_bytes = STANDARD.decode(signature_b64).map_err(|_| "corrupt signature".to_string())?;
    let sig_bytes: [u8; 64] =
        sig_bytes.try_into().map_err(|_| "signature has the wrong length".to_string())?;
    let signature = Signature::from_bytes(&sig_bytes);

    verifying_key
        .verify(&receipt_bytes, &signature)
        .map_err(|_| "signature does not match — the receipt was tampered with or is forged".to_string())?;

    serde_json::from_slice(&receipt_bytes).map_err(|e| format!("malformed receipt: {e}"))
}

/// Re-derives license status from a stored (receipt, signature) pair —
/// verifying the signature and checking expiry fresh every time, rather
/// than trusting that a row exists at all. An invalid signature or an
/// expired receipt both just fall back to unlicensed; the caller (see
/// `db::get_license`) is responsible for attempting a live revalidation
/// before giving up.
pub fn status_from_stored(receipt_b64: &str, signature_b64: &str) -> Option<LicenseInfo> {
    let receipt = verify_receipt(receipt_b64, signature_b64).ok()?;
    if is_expired(&receipt) {
        return None;
    }
    Some(LicenseInfo {
        tier: receipt.tier,
        license_ref: receipt.key,
        max_backups: receipt.max_backups,
        expires_at: receipt.expires_at,
    })
}

fn is_expired(receipt: &Receipt) -> bool {
    match chrono::DateTime::parse_from_rfc3339(&receipt.expires_at) {
        Ok(expires_at) => expires_at < chrono::Utc::now(),
        Err(_) => true, // an unparseable expiry is not a valid receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(receipt: &Receipt) -> (String, String, SigningKey) {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let bytes = serde_json::to_vec(receipt).unwrap();
        let signature = signing_key.sign(&bytes);
        (STANDARD.encode(&bytes), STANDARD.encode(signature.to_bytes()), signing_key)
    }

    fn sample_receipt(expires_in_days: i64) -> Receipt {
        Receipt {
            key: "BVPR-TEST".into(),
            tier: "pro".into(),
            max_backups: 999,
            machine_id: "machine-1".into(),
            issued_at: chrono::Utc::now().to_rfc3339(),
            expires_at: (chrono::Utc::now() + chrono::Duration::days(expires_in_days)).to_rfc3339(),
        }
    }

    #[test]
    fn a_genuine_signature_verifies_and_round_trips_fields() {
        let receipt = sample_receipt(30);
        let (b64, sig, signing_key) = sign(&receipt);
        let verified =
            verify_receipt_with_key(&b64, &sig, &signing_key.verifying_key()).expect("must verify");
        assert_eq!(verified.max_backups, 999);
        assert_eq!(verified.machine_id, "machine-1");
    }

    #[test]
    fn a_signature_from_the_wrong_key_is_rejected() {
        // verify_receipt() checks against the real, hardcoded
        // SERVER_PUBLIC_KEY — signing with a throwaway key here means this
        // exercises the "wrong signer" rejection path, not a true positive
        // (that's the test above, via verify_receipt_with_key).
        let receipt = sample_receipt(30);
        let (b64, sig, _) = sign(&receipt);
        assert!(verify_receipt(&b64, &sig).is_err());
    }

    #[test]
    fn garbage_input_is_rejected() {
        assert!(verify_receipt("not-base64!!", "also-not-base64!!").is_err());
    }

    #[test]
    fn an_expired_receipt_is_detected() {
        assert!(is_expired(&sample_receipt(-1)));
        assert!(!is_expired(&sample_receipt(30)));
    }

    #[test]
    fn revalidation_is_due_inside_the_window_but_not_before_it() {
        let far_away = (chrono::Utc::now() + chrono::Duration::days(20)).to_rfc3339();
        let soon = (chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339();
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        assert!(!needs_revalidation(&far_away));
        assert!(needs_revalidation(&soon));
        assert!(needs_revalidation(&past));
    }
}
