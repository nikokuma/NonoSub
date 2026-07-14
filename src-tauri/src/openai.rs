use futures_util::StreamExt;
use reqwest::{multipart, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

pub const TRANSCRIPTION_MODEL: &str = "gpt-4o-transcribe-diarize";
pub const LANGUAGE_MODEL: &str = "gpt-5.6-sol";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorKind {
    Authentication,
    ModelUnavailable,
    RateLimited,
    Network,
    MalformedResponse,
    Service,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub kind: ApiErrorKind,
    pub message: String,
    pub retryable: bool,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiarizedSegment {
    pub id: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub text: String,
    pub speaker: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationInput {
    pub segment_id: String,
    pub speaker: String,
    pub japanese: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationOutput {
    pub segment_id: String,
    pub natural_english: String,
    #[serde(default)]
    pub ambiguity_note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SpeakerReference {
    pub name: String,
    pub data_url: String,
}

#[derive(Debug, Deserialize)]
struct TranslationEnvelope {
    translations: Vec<TranslationOutput>,
}

#[derive(Clone)]
pub struct OpenAiClient {
    http: Client,
    api_key: String,
}

impl OpenAiClient {
    pub fn new(api_key: String) -> Result<Self, ApiError> {
        let http = Client::builder().build().map_err(|error| ApiError {
            kind: ApiErrorKind::Network,
            message: format!("Could not initialize secure networking: {error}"),
            retryable: false,
        })?;
        Ok(Self { http, api_key })
    }

    pub async fn validate_model_access(&self) -> Result<(), ApiError> {
        for model in [LANGUAGE_MODEL, TRANSCRIPTION_MODEL] {
            let response = self
                .http
                .get(format!("https://api.openai.com/v1/models/{model}"))
                .bearer_auth(&self.api_key)
                .send()
                .await
                .map_err(network_error)?;
            if !response.status().is_success() {
                return Err(response_error(response.status(), model).await);
            }
        }
        Ok(())
    }

    pub async fn transcribe_chunk(&self, path: &Path, references: &[SpeakerReference]) -> Result<Vec<DiarizedSegment>, ApiError> {
        let bytes = tokio::fs::read(path).await.map_err(|error| ApiError {
            kind: ApiErrorKind::Service,
            message: format!("Could not read a temporary audio chunk: {error}"),
            retryable: false,
        })?;
        if bytes.len() >= 25 * 1024 * 1024 {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "A generated audio chunk exceeded OpenAI's 25 MB upload limit.".into(),
                retryable: false,
            });
        }
        let file = multipart::Part::bytes(bytes)
            .file_name("chunk.wav")
            .mime_str("audio/wav")
            .map_err(|error| ApiError { kind: ApiErrorKind::Service, message: error.to_string(), retryable: false })?;
        let mut form = multipart::Form::new()
            .part("file", file)
            .text("model", TRANSCRIPTION_MODEL)
            .text("response_format", "diarized_json")
            .text("stream", "true")
            .text("chunking_strategy", "auto")
            .text("language", "ja")
            .text("prompt", "Japanese conversation. Preserve Japanese script, punctuation, hesitations, and unfinished phrases.");
        if !references.is_empty() {
            form = form
                .text("known_speaker_names", serde_json::to_string(&references.iter().map(|reference| &reference.name).collect::<Vec<_>>()).unwrap_or_default())
                .text("known_speaker_references", serde_json::to_string(&references.iter().map(|reference| &reference.data_url).collect::<Vec<_>>()).unwrap_or_default());
        }
        let response = self
            .http
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(response_error(response.status(), TRANSCRIPTION_MODEL).await);
        }

        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut segments = Vec::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(network_error)?;
            pending.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(boundary) = pending.find("\n\n") {
                let event = pending[..boundary].to_owned();
                pending.drain(..boundary + 2);
                if let Some(segment) = parse_diarized_sse_event(&event)? {
                    segments.push(segment);
                }
            }
        }
        if !pending.trim().is_empty() {
            if let Some(segment) = parse_diarized_sse_event(&pending)? {
                segments.push(segment);
            }
        }
        Ok(segments)
    }

    pub async fn translate(
        &self,
        preceding_context: &[TranslationInput],
        segments: &[TranslationInput],
    ) -> Result<Vec<TranslationOutput>, ApiError> {
        if segments.is_empty() || segments.len() > 6 {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "Translation batches must contain one to six segments.".into(),
                retryable: false,
            });
        }
        let request = json!({
            "model": LANGUAGE_MODEL,
            "reasoning": { "effort": "low" },
            "store": false,
            "input": [
                { "role": "system", "content": [{ "type": "input_text", "text": "Translate Japanese dialogue into concise, natural English subtitles. Use speaker and preceding dialogue context. Preserve ambiguity; never invent missing certainty. Return exactly one result for each requested segment." }] },
                { "role": "user", "content": [{ "type": "input_text", "text": serde_json::to_string(&json!({"preceding_context": preceding_context, "segments_to_translate": segments})).unwrap_or_default() }] }
            ],
            "text": { "format": {
                "type": "json_schema",
                "name": "contextual_subtitle_translations",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": { "translations": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "segment_id": { "type": "string" },
                                "natural_english": { "type": "string" },
                                "ambiguity_note": { "type": ["string", "null"] }
                            },
                            "required": ["segment_id", "natural_english", "ambiguity_note"],
                            "additionalProperties": false
                        }
                    } },
                    "required": ["translations"],
                    "additionalProperties": false
                }
            } }
        });
        let response = self
            .http
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(response_error(response.status(), LANGUAGE_MODEL).await);
        }
        let value: Value = response.json().await.map_err(|error| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: format!("GPT-5.6 returned unreadable translation output: {error}"),
            retryable: true,
        })?;
        let output_text = extract_response_text(&value).ok_or_else(|| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: "GPT-5.6 returned no structured translation text.".into(),
            retryable: true,
        })?;
        let envelope: TranslationEnvelope = serde_json::from_str(output_text).map_err(|error| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: format!("GPT-5.6 returned malformed structured translations: {error}"),
            retryable: true,
        })?;
        Ok(envelope.translations)
    }

    pub async fn tutor_stream<F>(
        &self,
        lesson_context: &Value,
        mut on_delta: F,
    ) -> Result<(), ApiError>
    where
        F: FnMut(String) -> Result<(), ApiError>,
    {
        let request = json!({
            "model": LANGUAGE_MODEL,
            "reasoning": { "effort": "low" },
            "store": false,
            "stream": true,
            "input": [
                { "role": "system", "content": [{ "type": "input_text", "text": "You are Nono, an accurate Japanese language tutor. Explain meaning in context at the requested learner level. Cover grammar, literal versus natural meaning, tone, politeness, omitted information, and culture when relevant. Explicitly label ambiguity instead of inventing certainty. Accuracy comes first; then add a light cute, playful, slightly bratty personality. Be concise unless the learner asks for depth." }] },
                { "role": "user", "content": [{ "type": "input_text", "text": serde_json::to_string(lesson_context).unwrap_or_default() }] }
            ]
        });
        let response = self
            .http
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(response_error(response.status(), LANGUAGE_MODEL).await);
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        while let Some(chunk) = stream.next().await {
            pending.push_str(&String::from_utf8_lossy(&chunk.map_err(network_error)?));
            while let Some(boundary) = pending.find("\n\n") {
                let event = pending[..boundary].to_owned();
                pending.drain(..boundary + 2);
                if let Some(delta) = parse_response_delta(&event)? {
                    on_delta(delta)?;
                }
            }
        }
        Ok(())
    }
}

pub fn parse_diarized_sse_event(event: &str) -> Result<Option<DiarizedSegment>, ApiError> {
    let data = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&data).map_err(|error| ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: format!("Transcription stream returned malformed JSON: {error}"),
        retryable: true,
    })?;
    if value.get("type").and_then(Value::as_str) != Some("transcript.text.segment") {
        return Ok(None);
    }
    let segment = value.get("segment").unwrap_or(&value);
    Ok(Some(DiarizedSegment {
        id: segment.get("id").and_then(Value::as_str).unwrap_or_default().to_owned(),
        start_seconds: required_f64(segment, "start")?,
        end_seconds: required_f64(segment, "end")?,
        text: required_string(segment, "text")?,
        speaker: required_string(segment, "speaker")?,
    }))
}

fn required_f64(value: &Value, key: &str) -> Result<f64, ApiError> {
    value.get(key).and_then(Value::as_f64).ok_or_else(|| malformed_field(key))
}

fn required_string(value: &Value, key: &str) -> Result<String, ApiError> {
    value.get(key).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| malformed_field(key))
}

fn malformed_field(field: &str) -> ApiError {
    ApiError { kind: ApiErrorKind::MalformedResponse, message: format!("Transcription segment omitted {field}."), retryable: true }
}

pub fn extract_response_text(value: &Value) -> Option<&str> {
    value.get("output")?
        .as_array()?
        .iter()
        .flat_map(|item| item.get("content").and_then(Value::as_array).into_iter().flatten())
        .find_map(|content| content.get("text").and_then(Value::as_str))
}

pub fn parse_response_delta(event: &str) -> Result<Option<String>, ApiError> {
    let data = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&data).map_err(|error| ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: format!("Tutor stream returned malformed JSON: {error}"),
        retryable: true,
    })?;
    if value.get("type").and_then(Value::as_str) != Some("response.output_text.delta") {
        return Ok(None);
    }
    Ok(value.get("delta").and_then(Value::as_str).map(str::to_owned))
}

async fn response_error(status: StatusCode, model: &str) -> ApiError {
    classify_status(status, model)
}

fn classify_status(status: StatusCode, model: &str) -> ApiError {
    let (kind, retryable, message) = match status {
        StatusCode::UNAUTHORIZED => (ApiErrorKind::Authentication, false, "The OpenAI API key was rejected.".to_string()),
        StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => (ApiErrorKind::ModelUnavailable, false, format!("This API project cannot access {model}.")),
        StatusCode::TOO_MANY_REQUESTS => (ApiErrorKind::RateLimited, true, "OpenAI rate-limited this request. NonoSub will retry once.".to_string()),
        status if status.is_server_error() => (ApiErrorKind::Service, true, "OpenAI is temporarily unavailable.".to_string()),
        _ => (ApiErrorKind::Service, false, format!("OpenAI rejected the request ({status}).")),
    };
    ApiError { kind, message, retryable }
}

fn network_error(error: reqwest::Error) -> ApiError {
    ApiError { kind: ApiErrorKind::Network, message: format!("Could not reach OpenAI: {error}"), retryable: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_finalized_diarized_segment_event() {
        let event = "event: transcript.text.segment\ndata: {\"type\":\"transcript.text.segment\",\"id\":\"seg_1\",\"start\":1.25,\"end\":2.5,\"speaker\":\"A\",\"text\":\"何ですか？\"}\n";
        let segment = parse_diarized_sse_event(event).unwrap().unwrap();
        assert_eq!(segment.id, "seg_1");
        assert_eq!(segment.speaker, "A");
        assert_eq!(segment.start_seconds, 1.25);
    }

    #[test]
    fn ignores_non_segment_and_done_events() {
        assert!(parse_diarized_sse_event("data: [DONE]").unwrap().is_none());
        assert!(parse_diarized_sse_event("data: {\"type\":\"transcript.text.delta\"}").unwrap().is_none());
    }

    #[test]
    fn extracts_structured_output_text() {
        let value = json!({"output":[{"content":[{"type":"output_text","text":"{\"translations\":[]}"}]}]});
        assert_eq!(extract_response_text(&value), Some("{\"translations\":[]}"));
    }

    #[test]
    fn parses_tutor_text_delta() {
        let event = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"何 means what.\"}";
        assert_eq!(parse_response_delta(event).unwrap(), Some("何 means what.".into()));
    }

    #[test]
    fn classifies_authentication_and_rate_limit_errors() {
        let authentication = classify_status(StatusCode::UNAUTHORIZED, LANGUAGE_MODEL);
        assert_eq!(authentication.kind, ApiErrorKind::Authentication);
        assert!(!authentication.retryable);
        let rate_limit = classify_status(StatusCode::TOO_MANY_REQUESTS, LANGUAGE_MODEL);
        assert_eq!(rate_limit.kind, ApiErrorKind::RateLimited);
        assert!(rate_limit.retryable);
    }
}
