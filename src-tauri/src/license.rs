//! Offline license verification. No network call, ever — a key is checked
//! entirely against the secret embedded below, which is why it works for a
//! fully local, privacy-first app. Keys are minted by the separate `keygen`
//! tool (outside this crate, holds the same secret) after a sale.
//!
//! This is a symmetric (HMAC) scheme, not asymmetric: the secret that
//! verifies a key is the same one embedded in every copy of this app, so it
//! is extractable by anyone willing to reverse-engineer the binary. That is
//! a deliberate tradeoff for a short, pleasant, copy-paste-able key on a
//! low-priced utility app — it stops casual key sharing, not a determined
//! attacker. See docs/licensing.md before raising the price point or
//! adding a tier where that tradeoff stops being acceptable.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

// Generated once via `license-keygen genkey`, must match the keygen tool's
// signing-key.secret exactly.
const SHARED_SECRET: [u8; 32] = [
    ***REMOVED (rotated dead secret, redacted from history)***
];

const MAC_LEN: usize = 10;

#[derive(Debug, Clone, serde::Serialize)]
pub struct LicenseInfo {
    pub tier: String,
    /// Short reference the user can quote in support requests without
    /// exposing the whole key — not a secret itself.
    pub license_ref: String,
}

pub const FREE_TIER_BACKUP_LIMIT: usize = 1;

pub fn verify_key(raw_key: &str) -> Result<LicenseInfo, String> {
    let cleaned = raw_key.trim().trim_start_matches("BVPR").trim_start_matches('-');
    let stripped: String = cleaned.chars().filter(|c| *c != '-').collect();

    let blob = base32::decode(base32::Alphabet::Crockford, &stripped)
        .ok_or("Ключ не распознан — проверьте, что скопирован полностью")?;

    if blob.len() != 1 + 6 + MAC_LEN {
        return Err("Ключ повреждён или неполный".into());
    }
    let payload = &blob[..7];
    let mac_bytes = &blob[7..];

    let mut mac = HmacSha256::new_from_slice(&SHARED_SECRET).expect("hmac accepts any key length");
    mac.update(payload);
    let full_mac = mac.finalize().into_bytes();
    // verify_slice() requires the full 32-byte output — we only ship a
    // truncated MAC to keep keys short, so the truncated comparison has to
    // be done by hand, still constant-time to avoid a timing side channel.
    let matches: bool = full_mac[..MAC_LEN].ct_eq(mac_bytes).into();
    if !matches {
        return Err("Ключ недействителен".into());
    }

    let tier = match payload[0] {
        b'P' => "pro",
        _ => "pro",
    };
    let license_ref = hex_short(&payload[1..7]);

    Ok(LicenseInfo { tier: tier.to_string(), license_ref })
}

fn hex_short(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_key_from_the_real_keygen_tool() {
        // Minted with the SHARED_SECRET above via `license-keygen issue`.
        let key = "BVPR-A3ZMX-PT3M3-RE4RN-53TAA-3ZKWA-B9G";
        let info = verify_key(key).expect("valid key must verify");
        assert_eq!(info.tier, "pro");
    }

    #[test]
    fn rejects_a_tampered_key() {
        let mut key = "BVPR-A3ZMX-PT3M3-RE4RN-53TAA-3ZKWA-B9G".to_string();
        key.replace_range(6..7, "0");
        assert!(verify_key(&key).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(verify_key("not a real key").is_err());
    }
}
