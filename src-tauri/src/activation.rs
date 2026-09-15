//! HTTP client for the backup-verify license server (see
//! `license.rs` for what happens to its response once it arrives).
//!
//! Deliberately thin: this module's only job is to POST a key + this
//! install's machine_id and hand back whatever (receipt, signature) pair
//! the server returned, verbatim. It never decides whether that pair is
//! trustworthy — `license::verify_receipt` does that from scratch every
//! time, so a compromised or spoofed response here still can't grant a
//! license it can't cryptographically prove.

use serde::Deserialize;

pub const LICENSE_SERVER_URL: &str = "https://193-233-137-110.sslip.io:8766";

pub struct StoredReceipt {
    pub receipt_b64: String,
    pub signature_b64: String,
}

#[derive(Deserialize)]
struct ServerResponse {
    receipt: String,
    signature: String,
}

#[derive(Deserialize)]
struct ServerError {
    error: String,
}

async fn call(path: &str, key: &str, machine_id: &str) -> Result<StoredReceipt, String> {
    use tauri_plugin_http::reqwest;

    let client = reqwest::Client::new();
    let body = serde_json::to_vec(&serde_json::json!({ "key": key, "machine_id": machine_id }))
        .expect("a two-field string map always serializes");

    let response = client
        .post(format!("{LICENSE_SERVER_URL}{path}"))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|_| {
            "Не удалось связаться с сервером лицензий — проверьте подключение к интернету."
                .to_string()
        })?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        let message = serde_json::from_str::<ServerError>(&text)
            .map(|e| translate_server_error(&e.error))
            .unwrap_or_else(|_| "Сервер лицензий вернул непонятный ответ".to_string());
        return Err(message);
    }

    let parsed: ServerResponse =
        serde_json::from_str(&text).map_err(|_| "Сервер лицензий вернул непонятный ответ".to_string())?;
    Ok(StoredReceipt { receipt_b64: parsed.receipt, signature_b64: parsed.signature })
}

/// First-time activation of a key on this install (this install's
/// `machine_id`, generated once and stored locally — see `db::machine_id`).
pub async fn activate(key: &str, machine_id: &str) -> Result<StoredReceipt, String> {
    call("/activate", key, machine_id).await
}

/// Refreshes an already-activated key's receipt before it expires. Only
/// succeeds for a `machine_id` the server has already seen for this key.
pub async fn revalidate(key: &str, machine_id: &str) -> Result<StoredReceipt, String> {
    call("/revalidate", key, machine_id).await
}

fn translate_server_error(code: &str) -> String {
    match code {
        "unknown key" => "Ключ не найден".to_string(),
        "key revoked" => "Ключ отозван".to_string(),
        "activation limit reached for this key" => {
            "Этот ключ уже активирован на максимальном числе устройств".to_string()
        }
        "machine not activated for this key" => {
            "Устройство не было активировано этим ключом — активируйте его заново".to_string()
        }
        other => format!("Сервер лицензий отклонил запрос: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real network round trip against the deployed license server — not
    /// run by default (`cargo test`), only with `-- --ignored`, since CI
    /// and offline dev builds shouldn't depend on this server being up.
    /// Exists to prove the client and server actually agree on the wire
    /// format and signature scheme end to end, not just in isolation.
    #[test]
    #[ignore]
    fn activation_round_trip_against_the_real_server() {
        // Issued locally on the server via `server.py issue` for this test.
        let key = "BVPR-3FCCD-56D09-BC0A7-91248";
        let machine_id = "integration-test-machine";
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let stored = rt.block_on(activate(key, machine_id)).expect("activation must succeed");
        let receipt = crate::license::verify_receipt(&stored.receipt_b64, &stored.signature_b64)
            .expect("server's own signature must verify against our embedded public key");
        assert_eq!(receipt.key, key);
        assert_eq!(receipt.machine_id, machine_id);

        let revalidated = rt
            .block_on(revalidate(key, machine_id))
            .expect("revalidation of an already-activated machine must succeed");
        crate::license::verify_receipt(&revalidated.receipt_b64, &revalidated.signature_b64)
            .expect("revalidated receipt must also verify");
    }
}
