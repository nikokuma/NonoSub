use futures_util::StreamExt;
use reqwest::{multipart, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path, time::Duration};

use crate::contracts::{
    ChalkColor, ChalkMark, ChalkPhrase, LanguageSettings, LessonCard, TailCue, TeachingMoment,
};

pub const TRANSCRIPTION_MODEL: &str = "gpt-4o-transcribe-diarize";
pub const LANGUAGE_MODEL: &str = "gpt-5.6-sol";
pub const REALTIME_TRANSLATION_MODEL: &str = "gpt-realtime-translate";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const RESPONSES_TIMEOUT: Duration = Duration::from_secs(120);
const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorKind {
    Authentication,
    ModelUnavailable,
    RateLimited,
    Network,
    MalformedResponse,
    Refused,
    Cancelled,
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
    pub source_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationOutput {
    pub segment_id: String,
    pub translation: String,
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
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_IDLE_TIMEOUT)
            .build()
            .map_err(|error| ApiError {
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
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .map_err(network_error)?;
            if !response.status().is_success() {
                return Err(response_error(response, model).await);
            }
        }
        Ok(())
    }

    pub async fn model_accessible(&self, model: &str) -> bool {
        self.http
            .get(format!("https://api.openai.com/v1/models/{model}"))
            .bearer_auth(&self.api_key)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn transcribe_chunk(
        &self,
        path: &Path,
        references: &[SpeakerReference],
        source_language: &str,
    ) -> Result<Vec<DiarizedSegment>, ApiError> {
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
            .map_err(|error| ApiError {
                kind: ApiErrorKind::Service,
                message: error.to_string(),
                retryable: false,
            })?;
        let mut form = multipart::Form::new()
            .part("file", file)
            .text("model", TRANSCRIPTION_MODEL)
            .text("response_format", "diarized_json")
            .text("stream", "true")
            .text("chunking_strategy", "auto");
        if source_language != "auto" {
            form = form.text("language", source_language.to_owned());
        }
        for reference in references {
            form = form
                .text("known_speaker_names[]", reference.name.clone())
                .text("known_speaker_references[]", reference.data_url.clone());
        }
        let response = self
            .http
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .timeout(TRANSCRIPTION_TIMEOUT)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(response_error(response, TRANSCRIPTION_MODEL).await);
        }

        let mut stream = response.bytes_stream();
        let mut pending = Vec::new();
        let mut segments = Vec::new();
        let mut terminal = false;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(network_error)?;
            pending.extend_from_slice(&bytes);
            while let Some(event) = take_sse_event(&mut pending)? {
                match parse_transcription_sse_event(&event)? {
                    TranscriptionStreamEvent::Segment(segment) => segments.push(segment),
                    TranscriptionStreamEvent::Done => terminal = true,
                    TranscriptionStreamEvent::Ignored => {}
                }
            }
        }
        if pending.iter().any(|byte| !byte.is_ascii_whitespace()) {
            let trailing = String::from_utf8(pending)
                .map_err(|error| malformed_transcription_stream(&error.to_string()))?;
            match parse_transcription_sse_event(&trailing)? {
                TranscriptionStreamEvent::Segment(segment) => segments.push(segment),
                TranscriptionStreamEvent::Done => terminal = true,
                TranscriptionStreamEvent::Ignored => {}
            }
        }
        require_transcription_terminal(terminal)?;
        Ok(segments)
    }

    pub async fn translate(
        &self,
        preceding_context: &[TranslationInput],
        segments: &[TranslationInput],
        languages: &LanguageSettings,
    ) -> Result<Vec<TranslationOutput>, ApiError> {
        if segments.is_empty() || segments.len() > 6 {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "Translation batches must contain one to six segments.".into(),
                retryable: false,
            });
        }
        let requested_ids = segments
            .iter()
            .map(|segment| segment.segment_id.clone())
            .collect::<Vec<_>>();
        if requested_ids.iter().collect::<HashSet<_>>().len() != requested_ids.len() {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "Translation batch segment IDs must be unique.".into(),
                retryable: false,
            });
        }
        let batch_size = segments.len();
        let request = json!({
            "model": LANGUAGE_MODEL,
            "reasoning": { "effort": "low" },
            "store": false,
            "input": [
                { "role": "system", "content": [{ "type": "input_text", "text": "Translate dialogue into concise, natural subtitles in the requested target language. Use speaker and preceding dialogue context. Preserve ambiguity; never invent missing certainty. Return exactly one result for each requested segment." }] },
                { "role": "user", "content": [{ "type": "input_text", "text": serde_json::to_string(&json!({"languages": languages, "preceding_context": preceding_context, "segments_to_translate": segments})).unwrap_or_default() }] }
            ],
            "text": { "format": {
                "type": "json_schema",
                "name": "contextual_subtitle_translations",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": { "translations": {
                        "type": "array",
                        "minItems": batch_size,
                        "maxItems": batch_size,
                        "items": {
                            "type": "object",
                            "properties": {
                                "segment_id": { "type": "string", "enum": requested_ids },
                                "translation": { "type": "string" },
                                "ambiguity_note": { "type": ["string", "null"] }
                            },
                            "required": ["segment_id", "translation", "ambiguity_note"],
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
            .timeout(RESPONSES_TIMEOUT)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(response_error(response, LANGUAGE_MODEL).await);
        }
        let value: Value = response.json().await.map_err(|error| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: format!("GPT-5.6 returned unreadable translation output: {error}"),
            retryable: true,
        })?;
        parse_translation_response(&value, segments)
    }

    pub async fn lesson(&self, lesson_context: &Value) -> Result<LessonCard, ApiError> {
        let chalk_phrase_schema = json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "maxLength": 90 },
                "color": { "type": "string", "enum": ["white", "baby_blue", "yellow", "pink"] },
                "mark": { "type": "string", "enum": ["none", "box", "bracket", "strike"] },
                "tailCue": { "type": "string", "enum": ["none", "point", "underline"] }
            },
            "required": ["text", "color", "mark", "tailCue"],
            "additionalProperties": false
        });
        let demo_item_schema = json!({
            "type": "object",
            "properties": {
                "label": { "type": "string", "maxLength": 30 },
                "detail": { "type": "string", "maxLength": 80 },
                "color": { "type": "string", "enum": ["white", "baby_blue", "yellow", "pink"] },
                "mark": { "type": "string", "enum": ["none", "box", "bracket", "strike"] },
                "tailCue": { "type": "string", "enum": ["none", "point", "underline"] }
            },
            "required": ["label", "detail", "color", "mark", "tailCue"],
            "additionalProperties": false
        });
        let request = json!({
            "model": LANGUAGE_MODEL,
            "reasoning": { "effort": "low" },
            "store": false,
            "input": [
                { "role": "system", "content": [{ "type": "input_text", "text": "You are Nono, an accurate language tutor and chalkboard director. Teach the source utterance in the requested explanation language at the learner's level. The selected line may intentionally lack a translation; translate it when useful. Use nearby dialogue for cultural and pragmatic meaning. Preserve source quotations exactly. Mark ambiguity instead of inventing certainty. Accuracy comes first; use at most one light cute, playful, slightly bratty aside. Return one focused teaching moment when enough, or two to three ordered moments only for genuinely distinct concepts. Every moment must fit on one non-scrolling board. A demonstration allows at most one section with three short lines; without a demonstration, use at most two sections with three short lines each. Use at most four demo items. Choose sentence_breakdown for phrase pieces, omitted_meaning for an understood blank, literal_to_natural for a meaning transformation, tone_scale for direct-to-soft comparisons, mini_dialogue for context, or none. Direct the approved chalk presentation: white for neutral context; baby_blue for source forms and grammar; yellow for meanings and takeaways; pink for omission, contrast, correction, exception, or uncertainty. You may deviate only when it makes the lesson clearer. Use box or bracket as structural cues. Use strike only for definitely incorrect or unnatural language, always in pink, never for ambiguity. Each moment must contain exactly one tailCue: either one point or one underline, never both. Attach the cue to the exact phrase or item Nono should indicate. Choose point when Nono should hold attention on the moment's active concept, or underline for one memorable takeaway. Do not output coordinates, selectors, CSS, SVG, bone names, durations, or animation code. sourceFocus controls the already-displayed selected utterance. A tail-drawn underline remains after the gesture. Return three useful follow-up prompts for the complete lesson." }] },
                { "role": "user", "content": [{ "type": "input_text", "text": serde_json::to_string(lesson_context).unwrap_or_default() }] }
            ],
            "text": { "format": {
                "type": "json_schema",
                "name": "nonosub_lesson_card",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "schemaVersion": { "type": "integer", "enum": [2] },
                        "selectedSegmentId": { "type": "string" },
                        "moments": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 3,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "title": { "type": "string", "maxLength": 48 },
                                    "speechBubble": { "type": "string", "maxLength": 180 },
                                    "sourceFocus": {
                                        "type": "object",
                                        "properties": {
                                            "color": { "type": "string", "enum": ["white", "baby_blue", "yellow", "pink"] },
                                            "tailCue": { "type": "string", "enum": ["none", "point", "underline"] }
                                        },
                                        "required": ["color", "tailCue"],
                                        "additionalProperties": false
                                    },
                                    "boardSections": {
                                        "type": "array",
                                        "maxItems": 2,
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "heading": { "type": "string", "maxLength": 28 },
                                                "lines": { "type": "array", "minItems": 1, "maxItems": 3, "items": chalk_phrase_schema.clone() }
                                            },
                                            "required": ["heading", "lines"],
                                            "additionalProperties": false
                                        }
                                    },
                                    "demonstration": {
                                        "type": "object",
                                        "properties": {
                                            "kind": { "type": "string", "enum": ["none", "sentence_breakdown", "omitted_meaning", "literal_to_natural", "tone_scale", "mini_dialogue"] },
                                            "caption": { "type": ["string", "null"], "maxLength": 90 },
                                            "items": {
                                                "type": "array",
                                                "maxItems": 4,
                                                "items": demo_item_schema
                                            },
                                            "result": { "anyOf": [chalk_phrase_schema.clone(), { "type": "null" }] }
                                        },
                                        "required": ["kind", "caption", "items", "result"],
                                        "additionalProperties": false
                                    },
                                    "ambiguityNote": { "anyOf": [chalk_phrase_schema, { "type": "null" }] }
                                },
                                "required": ["title", "speechBubble", "sourceFocus", "boardSections", "demonstration", "ambiguityNote"],
                                "additionalProperties": false
                            }
                        },
                        "suggestedFollowUps": { "type": "array", "minItems": 3, "maxItems": 3, "items": { "type": "string" } }
                    },
                    "required": ["schemaVersion", "selectedSegmentId", "moments", "suggestedFollowUps"],
                    "additionalProperties": false
                }
            } }
        });
        let response = self
            .http
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.api_key)
            .json(&request)
            .timeout(RESPONSES_TIMEOUT)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(response_error(response, LANGUAGE_MODEL).await);
        }
        let value: Value = response.json().await.map_err(|error| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: format!("GPT-5.6 returned unreadable lesson output: {error}"),
            retryable: true,
        })?;
        let output_text = extract_response_text(&value).ok_or_else(|| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: "GPT-5.6 returned no structured lesson text.".into(),
            retryable: true,
        })?;
        let card: LessonCard = serde_json::from_str(output_text).map_err(|error| ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: format!("GPT-5.6 returned a malformed lesson card: {error}"),
            retryable: true,
        })?;
        validate_lesson_card(card)
    }
}

fn take_sse_event(pending: &mut Vec<u8>) -> Result<Option<String>, ApiError> {
    let lf = pending
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    let crlf = pending
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    let Some((boundary, delimiter_len)) = [lf, crlf]
        .into_iter()
        .flatten()
        .min_by_key(|(index, _)| *index)
    else {
        return Ok(None);
    };
    let event = String::from_utf8(pending[..boundary].to_vec())
        .map_err(|error| malformed_transcription_stream(&error.to_string()))?;
    pending.drain(..boundary + delimiter_len);
    Ok(Some(event))
}

fn malformed_transcription_stream(detail: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: format!("Transcription stream contained invalid text: {detail}"),
        retryable: true,
    }
}

fn require_transcription_terminal(terminal: bool) -> Result<(), ApiError> {
    terminal.then_some(()).ok_or_else(|| {
        malformed_transcription_stream("the connection ended before transcript.text.done")
    })
}

fn validate_lesson_card(card: LessonCard) -> Result<LessonCard, ApiError> {
    let too_long = |value: &str, limit: usize| value.chars().count() > limit;
    let invalid = card.schema_version != 2
        || card.selected_segment_id.trim().is_empty()
        || card.moments.is_empty()
        || card.moments.len() > 3
        || card.moments.iter().any(|moment| {
            let (point_count, underline_count) = cue_counts(moment);
            moment.title.trim().is_empty()
                || too_long(&moment.title, 48)
                || moment.speech_bubble.trim().is_empty()
                || too_long(&moment.speech_bubble, 180)
                || point_count + underline_count != 1
                || moment.board_sections.len() > 2
                || moment.board_sections.iter().any(|section| {
                    section.heading.trim().is_empty()
                        || too_long(&section.heading, 28)
                        || section.lines.is_empty()
                        || section.lines.len() > 3
                        || section.lines.iter().any(|line| invalid_phrase(line, 72))
                })
                || moment.demonstration.items.len() > 4
                || moment
                    .demonstration
                    .items
                    .iter()
                    .any(|item| {
                        item.label.trim().is_empty()
                            || too_long(&item.label, 30)
                            || item.detail.trim().is_empty()
                            || too_long(&item.detail, 80)
                            || invalid_mark(item.color, item.mark)
                    })
                || moment
                    .demonstration
                    .caption
                    .as_deref()
                    .is_some_and(|caption| too_long(caption, 90))
                || moment
                    .demonstration
                    .result
                    .as_ref()
                    .is_some_and(|result| invalid_phrase(result, 90))
                || moment
                    .ambiguity_note
                    .as_ref()
                    .is_some_and(|note| {
                        invalid_phrase(note, 140) || matches!(note.mark, ChalkMark::Strike)
                    })
                || (matches!(
                    moment.demonstration.kind,
                    crate::contracts::BoardDemoKind::None
                ) && (moment.board_sections.is_empty()
                    || !moment.demonstration.items.is_empty()))
                || (!matches!(
                    moment.demonstration.kind,
                    crate::contracts::BoardDemoKind::None
                ) && (moment.demonstration.items.is_empty()
                    || moment.board_sections.len() > 1))
        })
        || card.suggested_follow_ups.len() != 3
        || card
            .suggested_follow_ups
            .iter()
            .any(|prompt| prompt.trim().is_empty());
    if invalid {
        Err(ApiError {
            kind: ApiErrorKind::MalformedResponse,
            message: "GPT-5.6 returned an incomplete lesson card: expected exactly one tail cue per moment."
                .into(),
            retryable: true,
        })
    } else {
        Ok(card)
    }
}

fn invalid_phrase(phrase: &ChalkPhrase, length_limit: usize) -> bool {
    phrase.text.trim().is_empty()
        || phrase.text.chars().count() > length_limit
        || invalid_mark(phrase.color, phrase.mark)
}

fn invalid_mark(color: ChalkColor, mark: ChalkMark) -> bool {
    matches!(mark, ChalkMark::Strike) && !matches!(color, ChalkColor::Pink)
}

fn cue_counts(moment: &TeachingMoment) -> (usize, usize) {
    let mut cues = vec![moment.source_focus.tail_cue];
    cues.extend(
        moment
            .board_sections
            .iter()
            .flat_map(|section| section.lines.iter().map(|line| line.tail_cue)),
    );
    cues.extend(
        moment
            .demonstration
            .items
            .iter()
            .map(|item| item.tail_cue),
    );
    if let Some(result) = &moment.demonstration.result {
        cues.push(result.tail_cue);
    }
    if let Some(note) = &moment.ambiguity_note {
        cues.push(note.tail_cue);
    }
    (
        cues.iter()
            .filter(|cue| matches!(cue, TailCue::Point))
            .count(),
        cues.iter()
            .filter(|cue| matches!(cue, TailCue::Underline))
            .count(),
    )
}

#[derive(Debug, PartialEq)]
enum TranscriptionStreamEvent {
    Segment(DiarizedSegment),
    Done,
    Ignored,
}

fn parse_transcription_sse_event(event: &str) -> Result<TranscriptionStreamEvent, ApiError> {
    let data = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(TranscriptionStreamEvent::Ignored);
    }
    let value: Value = serde_json::from_str(&data).map_err(|error| ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: format!("Transcription stream returned malformed JSON: {error}"),
        retryable: true,
    })?;
    match value.get("type").and_then(Value::as_str) {
        Some("transcript.text.done") => return Ok(TranscriptionStreamEvent::Done),
        Some("error") => {
            let error = value.get("error").unwrap_or(&value);
            let code = error
                .get("code")
                .or_else(|| error.get("type"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("stream_error");
            let request_id = value
                .get("request_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: sanitized_openai_error("Transcription stream failed", code, request_id),
                retryable: true,
            });
        }
        Some("transcript.text.segment") => {}
        _ => return Ok(TranscriptionStreamEvent::Ignored),
    }
    let segment = value.get("segment").unwrap_or(&value);
    Ok(TranscriptionStreamEvent::Segment(DiarizedSegment {
        id: segment
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        start_seconds: required_f64(segment, "start")?,
        end_seconds: required_f64(segment, "end")?,
        text: required_string(segment, "text")?,
        speaker: required_string(segment, "speaker")?,
    }))
}

#[cfg(test)]
pub fn parse_diarized_sse_event(event: &str) -> Result<Option<DiarizedSegment>, ApiError> {
    match parse_transcription_sse_event(event)? {
        TranscriptionStreamEvent::Segment(segment) => Ok(Some(segment)),
        TranscriptionStreamEvent::Done | TranscriptionStreamEvent::Ignored => Ok(None),
    }
}

fn sanitized_openai_error(prefix: &str, code: &str, request_id: Option<&str>) -> String {
    let safe_code = code
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(80)
        .collect::<String>();
    let safe_request_id = request_id.map(|request_id| {
        request_id
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
            .take(100)
            .collect::<String>()
    });
    match safe_request_id.filter(|value| !value.is_empty()) {
        Some(request_id) => format!("{prefix} ({safe_code}; request {request_id})."),
        None => format!("{prefix} ({safe_code})."),
    }
}

fn required_f64(value: &Value, key: &str) -> Result<f64, ApiError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| malformed_field(key))
}

fn required_string(value: &Value, key: &str) -> Result<String, ApiError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| malformed_field(key))
}

fn malformed_field(field: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: format!("Transcription segment omitted {field}."),
        retryable: true,
    }
}

pub fn extract_response_text(value: &Value) -> Option<&str> {
    value
        .get("output")?
        .as_array()?
        .iter()
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|content| content.get("text").and_then(Value::as_str))
}

fn extract_response_refusal(value: &Value) -> Option<&str> {
    value
        .get("output")?
        .as_array()?
        .iter()
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|content| {
            (content.get("type").and_then(Value::as_str) == Some("refusal"))
                .then(|| content.get("refusal").and_then(Value::as_str))
                .flatten()
        })
}

fn malformed_translation(message: impl Into<String>) -> ApiError {
    ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: message.into(),
        retryable: true,
    }
}

fn parse_translation_response(
    value: &Value,
    requested: &[TranslationInput],
) -> Result<Vec<TranslationOutput>, ApiError> {
    match value.get("status").and_then(Value::as_str) {
        Some("incomplete") => {
            return Err(malformed_translation(
                "GPT-5.6 returned an incomplete structured translation.",
            ));
        }
        Some("failed") => {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "GPT-5.6 could not complete this translation batch.".into(),
                retryable: true,
            });
        }
        _ => {}
    }
    if extract_response_refusal(value).is_some() {
        return Err(ApiError {
            kind: ApiErrorKind::Refused,
            message: "GPT-5.6 declined this translation batch.".into(),
            retryable: false,
        });
    }
    let output_text = extract_response_text(value)
        .ok_or_else(|| malformed_translation("GPT-5.6 returned no structured translation text."))?;
    let envelope: TranslationEnvelope = serde_json::from_str(output_text).map_err(|error| {
        malformed_translation(format!(
            "GPT-5.6 returned malformed structured translations: {error}"
        ))
    })?;
    validate_translation_outputs(requested, envelope.translations)
}

fn validate_translation_outputs(
    requested: &[TranslationInput],
    outputs: Vec<TranslationOutput>,
) -> Result<Vec<TranslationOutput>, ApiError> {
    let requested_ids = requested
        .iter()
        .map(|input| input.segment_id.as_str())
        .collect::<HashSet<_>>();
    if requested_ids.len() != requested.len() {
        return Err(ApiError {
            kind: ApiErrorKind::Service,
            message: "Translation batch segment IDs must be unique.".into(),
            retryable: false,
        });
    }

    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(outputs.len());
    for mut output in outputs {
        if !requested_ids.contains(output.segment_id.as_str()) {
            return Err(malformed_translation(
                "GPT-5.6 returned a translation for an unknown segment.",
            ));
        }
        if !seen.insert(output.segment_id.clone()) {
            return Err(malformed_translation(
                "GPT-5.6 returned more than one translation for a segment.",
            ));
        }
        output.translation = output.translation.trim().to_owned();
        if output.translation.is_empty() {
            return Err(malformed_translation(
                "GPT-5.6 returned a blank subtitle translation.",
            ));
        }
        output.ambiguity_note = output
            .ambiguity_note
            .map(|note| note.trim().to_owned())
            .filter(|note| !note.is_empty());
        normalized.push(output);
    }
    if normalized.len() != requested.len() || seen.len() != requested.len() {
        return Err(malformed_translation(
            "GPT-5.6 omitted one or more requested translations.",
        ));
    }

    normalized.sort_by_key(|output| {
        requested
            .iter()
            .position(|input| input.segment_id == output.segment_id)
            .unwrap_or(usize::MAX)
    });
    Ok(normalized)
}

#[cfg(test)]
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
    Ok(value
        .get("delta")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

async fn response_error(response: reqwest::Response, model: &str) -> ApiError {
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.min(30)));
    let metadata = response.json::<Value>().await.ok();
    let code = metadata
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("code").or_else(|| error.get("type")))
        .and_then(Value::as_str);
    if status == StatusCode::TOO_MANY_REQUESTS {
        if let Some(delay) = retry_after {
            tokio::time::sleep(delay).await;
        }
    }
    let mut error = classify_status(status, model);
    if let Some(code) = code {
        error.message = sanitized_openai_error(&error.message, code, request_id.as_deref());
    } else if let Some(request_id) = request_id.as_deref() {
        error.message = sanitized_openai_error(&error.message, "request_failed", Some(request_id));
    }
    error
}

fn classify_status(status: StatusCode, model: &str) -> ApiError {
    let (kind, retryable, message) = match status {
        StatusCode::UNAUTHORIZED => (
            ApiErrorKind::Authentication,
            false,
            "The OpenAI API key was rejected.".to_string(),
        ),
        StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => (
            ApiErrorKind::ModelUnavailable,
            false,
            format!("This API project cannot access {model}."),
        ),
        StatusCode::TOO_MANY_REQUESTS => (
            ApiErrorKind::RateLimited,
            true,
            "OpenAI rate-limited this request. NonoSub will retry once.".to_string(),
        ),
        status if status.is_server_error() => (
            ApiErrorKind::Service,
            true,
            "OpenAI is temporarily unavailable.".to_string(),
        ),
        _ => (
            ApiErrorKind::Service,
            false,
            format!("OpenAI rejected the request ({status})."),
        ),
    };
    ApiError {
        kind,
        message,
        retryable,
    }
}

fn network_error(error: reqwest::Error) -> ApiError {
    ApiError {
        kind: ApiErrorKind::Network,
        message: if error.is_timeout() {
            "The OpenAI request timed out.".into()
        } else if error.is_connect() {
            "Could not connect to OpenAI.".into()
        } else {
            "The OpenAI connection ended unexpectedly.".into()
        },
        retryable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        BoardDemo, BoardDemoItem, BoardDemoKind, BoardSection, ChalkPhrase, SourceFocus,
    };

    fn phrase(text: &str, color: ChalkColor, mark: ChalkMark, tail_cue: TailCue) -> ChalkPhrase {
        ChalkPhrase {
            text: text.into(),
            color,
            mark,
            tail_cue,
        }
    }

    fn focused_lesson_moment(
        line_tail_cue: TailCue,
        result_tail_cue: TailCue,
    ) -> TeachingMoment {
        TeachingMoment {
            title: "The missing ending".into(),
            speech_bubble: "The listener fills in the refusal.".into(),
            source_focus: SourceFocus {
                color: ChalkColor::White,
                tail_cue: TailCue::None,
            },
            board_sections: vec![BoardSection {
                heading: "Spoken".into(),
                lines: vec![phrase(
                    "今日はちょっと……",
                    ChalkColor::BabyBlue,
                    ChalkMark::None,
                    line_tail_cue,
                )],
            }],
            demonstration: BoardDemo {
                kind: BoardDemoKind::OmittedMeaning,
                caption: Some("The ending stays unspoken.".into()),
                items: vec![BoardDemoItem {
                    label: "[行けない]".into(),
                    detail: "[I cannot go]".into(),
                    color: ChalkColor::Pink,
                    mark: ChalkMark::Bracket,
                    tail_cue: TailCue::None,
                }],
                result: Some(phrase(
                    "Today does not work for me.",
                    ChalkColor::Yellow,
                    ChalkMark::None,
                    result_tail_cue,
                )),
            },
            ambiguity_note: None,
        }
    }

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
        assert!(
            parse_diarized_sse_event("data: {\"type\":\"transcript.text.delta\"}")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn recognizes_terminal_transcription_and_rejects_explicit_stream_errors() {
        assert_eq!(
            parse_transcription_sse_event(
                "data: {\"type\":\"transcript.text.done\",\"text\":\"complete\"}"
            )
            .unwrap(),
            TranscriptionStreamEvent::Done
        );
        let error = parse_transcription_sse_event(
            "data: {\"type\":\"error\",\"request_id\":\"req_safe-1\",\"error\":{\"code\":\"server_error\",\"message\":\"sensitive body\"}}",
        )
        .unwrap_err();
        assert!(error.retryable);
        assert!(error.message.contains("server_error"));
        assert!(error.message.contains("req_safe-1"));
        assert!(!error.message.contains("sensitive body"));
        let truncated = require_transcription_terminal(false).unwrap_err();
        assert_eq!(truncated.kind, ApiErrorKind::MalformedResponse);
        assert!(truncated.retryable);
    }

    #[test]
    fn decodes_crlf_and_lf_sse_events_without_joining_json() {
        let first = "event: transcript.text.segment\r\ndata: {\"type\":\"transcript.text.segment\",\"id\":\"seg_1\",\"start\":0.0,\"end\":1.0,\"speaker\":\"A\",\"text\":\"何ですか？\"}\r\n\r\n";
        let second = "event: transcript.text.done\ndata: {\"type\":\"transcript.text.done\",\"text\":\"何ですか？\"}\n\n";
        let mut pending = [first.as_bytes(), second.as_bytes()].concat();
        let first_event = take_sse_event(&mut pending).unwrap().unwrap();
        let second_event = take_sse_event(&mut pending).unwrap().unwrap();
        assert_eq!(
            parse_diarized_sse_event(&first_event)
                .unwrap()
                .unwrap()
                .text,
            "何ですか？"
        );
        assert!(parse_diarized_sse_event(&second_event).unwrap().is_none());
        assert!(pending.is_empty());
    }

    #[test]
    fn waits_for_a_complete_crlf_delimiter_across_network_chunks() {
        let mut pending = b"event: ping\r\ndata: {\"type\":\"ping\"}\r".to_vec();
        assert!(take_sse_event(&mut pending).unwrap().is_none());
        pending.extend_from_slice(b"\n\r\n");
        assert!(take_sse_event(&mut pending).unwrap().is_some());
        assert!(pending.is_empty());
    }

    #[test]
    fn extracts_structured_output_text() {
        let value =
            json!({"output":[{"content":[{"type":"output_text","text":"{\"translations\":[]}"}]}]});
        assert_eq!(extract_response_text(&value), Some("{\"translations\":[]}"));
    }

    fn translation_input(id: &str) -> TranslationInput {
        TranslationInput {
            segment_id: id.into(),
            speaker: "speaker-1".into(),
            source_text: "source".into(),
        }
    }

    fn translation_response(outputs: Value) -> Value {
        json!({
            "status": "completed",
            "output": [{
                "content": [{
                    "type": "output_text",
                    "text": json!({ "translations": outputs }).to_string()
                }]
            }]
        })
    }

    #[test]
    fn validates_exact_translation_set_and_restores_request_order() {
        let requested = [translation_input("a"), translation_input("b")];
        let value = translation_response(json!([
            { "segment_id": "b", "translation": "  Second  ", "ambiguity_note": "  " },
            { "segment_id": "a", "translation": " First ", "ambiguity_note": " maybe " }
        ]));
        let outputs = parse_translation_response(&value, &requested).unwrap();
        assert_eq!(outputs[0].segment_id, "a");
        assert_eq!(outputs[0].translation, "First");
        assert_eq!(outputs[0].ambiguity_note.as_deref(), Some("maybe"));
        assert_eq!(outputs[1].segment_id, "b");
        assert_eq!(outputs[1].ambiguity_note, None);
    }

    #[test]
    fn rejects_missing_duplicate_unknown_and_blank_translations() {
        let requested = [translation_input("a"), translation_input("b")];
        for outputs in [
            json!([{ "segment_id": "a", "translation": "First", "ambiguity_note": null }]),
            json!([
                { "segment_id": "a", "translation": "First", "ambiguity_note": null },
                { "segment_id": "a", "translation": "Again", "ambiguity_note": null }
            ]),
            json!([
                { "segment_id": "a", "translation": "First", "ambiguity_note": null },
                { "segment_id": "unknown", "translation": "Other", "ambiguity_note": null }
            ]),
            json!([
                { "segment_id": "a", "translation": "First", "ambiguity_note": null },
                { "segment_id": "b", "translation": "   ", "ambiguity_note": null }
            ]),
        ] {
            let error =
                parse_translation_response(&translation_response(outputs), &requested).unwrap_err();
            assert_eq!(error.kind, ApiErrorKind::MalformedResponse);
            assert!(error.retryable);
        }
    }

    #[test]
    fn classifies_incomplete_output_as_retryable_and_refusal_as_terminal() {
        let requested = [translation_input("a")];
        let incomplete = parse_translation_response(
            &json!({ "status": "incomplete", "incomplete_details": { "reason": "max_output_tokens" } }),
            &requested,
        )
        .unwrap_err();
        assert_eq!(incomplete.kind, ApiErrorKind::MalformedResponse);
        assert!(incomplete.retryable);

        let refusal = parse_translation_response(
            &json!({
                "status": "completed",
                "output": [{ "content": [{ "type": "refusal", "refusal": "No." }] }]
            }),
            &requested,
        )
        .unwrap_err();
        assert_eq!(refusal.kind, ApiErrorKind::Refused);
        assert!(!refusal.retryable);
    }

    #[test]
    fn parses_tutor_text_delta() {
        let event = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"何 means what.\"}";
        assert_eq!(
            parse_response_delta(event).unwrap(),
            Some("何 means what.".into())
        );
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

    #[test]
    fn known_speakers_use_repeated_multipart_array_fields() {
        let reference = SpeakerReference {
            name: "speaker-1".into(),
            data_url: "data:audio/wav;base64,AAAA".into(),
        };
        let form = multipart::Form::new()
            .text("known_speaker_names[]", reference.name)
            .text("known_speaker_references[]", reference.data_url);
        let debug = format!("{form:?}");
        assert!(debug.contains("known_speaker_names[]"));
        assert!(debug.contains("known_speaker_references[]"));
    }

    #[test]
    fn diarized_transcription_uses_language_without_unsupported_prompt() {
        let form = multipart::Form::new()
            .text("model", TRANSCRIPTION_MODEL)
            .text("language", "ja");
        let debug = format!("{form:?}");
        assert!(debug.contains("language"));
        assert!(!debug.contains("prompt"));
    }

    #[test]
    fn accepts_a_focused_multi_moment_lesson_deck() {
        let point_moment = focused_lesson_moment(TailCue::Point, TailCue::None);
        let underline_moment = focused_lesson_moment(TailCue::None, TailCue::Underline);
        let card = LessonCard {
            schema_version: 2,
            selected_segment_id: "seg-4".into(),
            moments: vec![point_moment, underline_moment],
            suggested_follow_ups: vec!["One?".into(), "Two?".into(), "Three?".into()],
        };
        assert!(validate_lesson_card(card).is_ok());
    }

    #[test]
    fn rejects_a_moment_with_both_point_and_underline() {
        let card = LessonCard {
            schema_version: 2,
            selected_segment_id: "seg-4".into(),
            moments: vec![focused_lesson_moment(
                TailCue::Point,
                TailCue::Underline,
            )],
            suggested_follow_ups: vec!["One?".into(), "Two?".into(), "Three?".into()],
        };
        assert!(validate_lesson_card(card).is_err());
    }

    #[test]
    fn rejects_a_moment_with_no_tail_cue() {
        let card = LessonCard {
            schema_version: 2,
            selected_segment_id: "seg-4".into(),
            moments: vec![focused_lesson_moment(TailCue::None, TailCue::None)],
            suggested_follow_ups: vec!["One?".into(), "Two?".into(), "Three?".into()],
        };
        assert!(validate_lesson_card(card).is_err());
    }

    #[test]
    fn rejects_an_empty_or_overloaded_lesson_moment() {
        let card = LessonCard {
            schema_version: 2,
            selected_segment_id: "seg-4".into(),
            moments: vec![TeachingMoment {
                title: "Too much".into(),
                speech_bubble: "This board is overloaded.".into(),
                source_focus: SourceFocus {
                    color: ChalkColor::White,
                    tail_cue: TailCue::Point,
                },
                board_sections: vec![BoardSection {
                    heading: "Crowded".into(),
                    lines: vec![
                        phrase("1", ChalkColor::White, ChalkMark::None, TailCue::None),
                        phrase("2", ChalkColor::White, ChalkMark::None, TailCue::None),
                        phrase("3", ChalkColor::White, ChalkMark::None, TailCue::None),
                        phrase("4", ChalkColor::White, ChalkMark::None, TailCue::None),
                        phrase("5", ChalkColor::White, ChalkMark::None, TailCue::None),
                    ],
                }],
                demonstration: BoardDemo {
                    kind: BoardDemoKind::None,
                    caption: None,
                    items: Vec::new(),
                    result: None,
                },
                ambiguity_note: None,
            }],
            suggested_follow_ups: vec!["One?".into(), "Two?".into(), "Three?".into()],
        };
        assert!(validate_lesson_card(card).is_err());
    }

    #[test]
    fn rejects_duplicate_tail_cues_and_non_pink_strikes() {
        let card = LessonCard {
            schema_version: 2,
            selected_segment_id: "seg-4".into(),
            moments: vec![TeachingMoment {
                title: "Bad score".into(),
                speech_bubble: "This presentation score is invalid.".into(),
                source_focus: SourceFocus {
                    color: ChalkColor::White,
                    tail_cue: TailCue::Point,
                },
                board_sections: vec![BoardSection {
                    heading: "Correction".into(),
                    lines: vec![phrase(
                        "Not this",
                        ChalkColor::Yellow,
                        ChalkMark::Strike,
                        TailCue::Point,
                    )],
                }],
                demonstration: BoardDemo {
                    kind: BoardDemoKind::None,
                    caption: None,
                    items: Vec::new(),
                    result: None,
                },
                ambiguity_note: None,
            }],
            suggested_follow_ups: vec!["One?".into(), "Two?".into(), "Three?".into()],
        };
        assert!(validate_lesson_card(card).is_err());
    }

    #[test]
    fn rejects_obsolete_lesson_schema_versions() {
        let card = LessonCard {
            schema_version: 1,
            selected_segment_id: "seg-4".into(),
            moments: Vec::new(),
            suggested_follow_ups: vec!["One?".into(), "Two?".into(), "Three?".into()],
        };
        assert!(validate_lesson_card(card).is_err());
    }
}
