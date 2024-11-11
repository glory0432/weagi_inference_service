use base64::{prelude::BASE64_STANDARD, Engine};
use hyper::body::Bytes;
use image::{ImageFormat, ImageReader};
use reqwest::{Client, Response};
use rs_openai::{
    audio::{AudioModel, CreateTranscriptionRequestBuilder, ResponseFormat},
    chat::Role,
    shared::types::FileMeta,
    OpenAI,
};
use serde::Deserialize;
use serde_json::json;
use std::io::Cursor;

#[derive(Debug, Deserialize)]
pub struct ChatChunkDelta {
    content: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct ChatChunkChoice {
    delta: ChatChunkDelta,
    index: usize,
    finish_reason: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    id: String,
    object: String,
    created: usize,
    model: String,
    choices: Vec<ChatChunkChoice>,
}
#[derive(Deserialize)]
struct ImageGenerationResponse {
    pub created: u32,
    pub data: Vec<serde_json::Value>,
}
pub async fn send_chat_completion(
    openai_key: String,
    model_name: String,
    conversations: Vec<(String, Role, Vec<String>)>,
) -> Result<Response, String> {
    let request_body = json!({
        "model": model_name,
        "stream": true,
        "messages": conversations
        .iter()
        .map(|&(ref message, ref role, ref images)| {

            let content = if images.is_empty() {
                json!([{
                    "type": "text",
                    "text": message.clone()
                }])
            } else {
                let mut content_items = vec![json!({
                    "type": "text",
                    "text": message.clone()
                })];

                for image in images {
                    let img = ImageReader::open(format!("./public/{}", image));
                    if img.is_err() {
                        continue;
                    }
                    let img = img.unwrap().decode();
                    if img.is_err() {
                        continue;
                    }
                    let img = img.unwrap().to_rgb8();
                    let mut jpeg_buffer = Vec::new();
                    {
                        let mut cursor = Cursor::new(&mut jpeg_buffer);
                        if img.write_to(&mut cursor, ImageFormat::Jpeg).is_err() {
                            continue;
                        }
                    }
                    let base64_string = BASE64_STANDARD.encode(&jpeg_buffer);
                    content_items.push(json!({
                        "type": "image_url",
                        "image_url": {
                            "url" : format!("data:image/jpeg;base64,{}", base64_string)
                        }
                    }));
                }
                json!(content_items)
            };

            json!({
                "role": role,
                "content": content
            })
        }).collect::<Vec<_>>(),
    });
    let client = Client::new();
    let request_url = "https://api.openai.com/v1/chat/completions";
    Ok(client
        .post(request_url)
        .bearer_auth(openai_key.clone())
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("OpenAI response failed: {}", e))?)
}
pub fn chunk_to_content_list(chunk: Bytes) -> Result<Vec<String>, String> {
    let mut content_list = vec![];
    let chunk_str = match std::str::from_utf8(&chunk) {
        Ok(v) => v,
        Err(e) => {
            return Err(e.to_string());
        }
    };
    let mut cached_str = "".to_string();
    for p in chunk_str.split("\n") {
        match p.strip_prefix("data: ") {
            Some(p) => {
                if p == "[DONE]" {
                    break;
                }
                let d =
                    serde_json::from_str::<ChatCompletionChunk>(&format!("{}{}", cached_str, p));
                if d.is_err() {
                    cached_str.push_str(p);
                    continue;
                }
                let d = d.unwrap();

                let c = d.choices.get(0);
                if c.is_none() {
                    continue;
                }
                let c = c.unwrap();
                cached_str = String::from("");
                if let Some(content) = &c.delta.content {
                    content_list.push(content.clone());
                }
            }
            None => {}
        }
    }
    Ok(vec![])
}
pub async fn speech_to_text(
    api_key: &str,
    audio_data: Vec<u8>,
    filename: String,
) -> Result<String, String> {
    let client = OpenAI::new(&OpenAI {
        api_key: api_key.into(),
        org_id: None,
    });
    let req = CreateTranscriptionRequestBuilder::default()
        .file(FileMeta {
            buffer: audio_data.to_vec(),
            filename: filename,
        })
        .model(AudioModel::Whisper1)
        .response_format(ResponseFormat::Text)
        .build()
        .map_err(|e| format!("OpenAI transcription request build failed: {}", e))?;

    let res = client
        .audio()
        .create_transcription_with_text_response(&req)
        .await
        .map_err(|e| format!("OpenAI transcription sending request failed: {}", e))?;
    Ok(res)
}

pub async fn text_to_image(api_key: &str, prompt: &str) -> Result<String, String> {
    let request_body = json!({
        "model":"dall-e-3",
        "prompt":prompt,
        "size":"1024x1024",
        "quality":"standard",
        "n":1,
    });
    let client = Client::new();
    let request_url = "https://api.openai.com/v1/images/generations";

    let response = client
        .post(request_url)
        .bearer_auth(api_key)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send OpenAI rquest: {}", e.to_string()))?
        .json::<ImageGenerationResponse>()
        .await
        .map_err(|e| format!("Failed to parse OpenAI response as json: {}", e))?;
    if response.data.len() != 1 {
        return Err(format!("Failed to generate one image for the text"));
    }
    let url = response.data[0].get("url");
    if url.is_none() {
        return Err(format!("Failed to get the url for the generated image"));
    }
    let url = url.unwrap().as_str();
    if url.is_none() {
        return Err(format!("Failed to parse the url as string"));
    }
    let url = url.unwrap();
    Ok(url.to_string())
}
