use deepgram::{
    speak::options::{Container, Encoding, Model, Options},
    Deepgram,
};
use futures::Stream;
use hyper::body::Bytes;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
pub async fn text_to_speech(
    api_token: &str,
    text: &str,
    is_started: bool,
) -> Result<impl Stream<Item = Bytes>, String> {
    let dg_client = Deepgram::new(api_token);
    if dg_client.is_err() {
        return Err(format!("Failed to create deepgram client"));
    }
    let dg_client = dg_client.unwrap();
    let sample_rate = 16000;
    let options = Options::builder()
        .model(Model::AuraAsteriaEn)
        .encoding(Encoding::Linear16)
        .sample_rate(sample_rate)
        .container(if is_started == false {
            Container::Wav
        } else {
            Container::CustomContainer("none".to_owned())
        })
        .build();
    let audio_stream = dg_client
        .text_to_speech()
        .speak_to_stream(text, &options)
        .await;
    if audio_stream.is_err() {
        return Err(format!("Failed to create deepgram response stream"));
    }
    Ok(audio_stream.unwrap())
}
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
