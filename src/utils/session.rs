use base64::prelude::*;
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub async fn send_session_data(
    session_data: serde_json::Value,
    auth_uri: &str,
    secret_key: String,
    token: String,
) -> Result<(), String> {
    let client = Client::new();

    let body = session_data.to_string();
    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes()).map_err(|e| {
        let error_message = format!("Failed to make new hmac slice : {}", e);
        error_message
    })?;
    mac.update(body.as_bytes());
    let signature = mac.finalize().into_bytes();

    let response = client
        .post(format!("{}/session", auth_uri))
        .header("X-Signature", BASE64_STANDARD.encode(&signature)) // Include signature in headers
        .header("Content-Type", "application/json")
        .bearer_auth(token)
        .body(body)
        .send()
        .await
        .map_err(|e| {
            let error_message = format!("Sending set session data response failed: {}", e);
            error_message
        })?;

    if response.status().is_success() {
    } else {
        let error_message = format!(
            "Failed to send request: {:?}",
            response.text().await.map_err(|e| {
                let error_message = format!("Response parsing as text failed: {}", e);
                error_message
            })?
        );
        return Err(error_message);
    }

    Ok(())
}
