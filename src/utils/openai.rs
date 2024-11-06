use std::io::Cursor;

use base64::{prelude::BASE64_STANDARD, Engine};
use image::{ImageFormat, ImageReader};
use reqwest::{Client, Response};
use rs_openai::chat::Role;
use serde::Deserialize;
use serde_json::json;
use tracing::error;

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
        .map_err(|e| {
            let error_message = format!("OpenAI response failed: {}", e);
            error!("{}", error_message);
            error_message
        })?)
}
