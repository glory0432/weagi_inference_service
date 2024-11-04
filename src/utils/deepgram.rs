use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
pub async fn speech_to_text(
    api_token: &str,
    language: &str,
    audio_data: Vec<u8>,
) -> Result<String, String> {
    let url = format!(
        "https://api.deepgram.com/v1/listen?language={}&model=nova-2",
        language
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Token {}", api_token))
            .map_err(|e| format!("Invalid Header Value: {}", e))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("audio/*"));

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .headers(headers)
        .body(audio_data)
        .send()
        .await
        .map_err(|e| format!("Error in sending deepgram request: {}", e))?;

    let json_value = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Error in parsing the response into string: {}", e))?;

    if let Some(transcript) = json_value
        .get("results")
        .and_then(|results| results.get("channels"))
        .and_then(|result| result.get(0))
        .and_then(|channels| channels.get("alternatives"))
        .and_then(|channel| channel.get(0))
        .and_then(|alternatives| alternatives.get("transcript"))
    {
        return Ok(transcript.to_string());
    }

    return Err(format!(
        "Error in retrieving transcript field data in response"
    ));
}
