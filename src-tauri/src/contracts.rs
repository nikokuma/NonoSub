use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearnerLevel {
    Beginner,
    Intermediate,
    Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    Pending,
    Complete,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptionProcessingMode {
    #[default]
    Translated,
    OriginalOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    File,
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiveSyncMode {
    #[default]
    Coordinated,
    FastSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiveTranslationEngine {
    #[default]
    Realtime,
    TranscriptLocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveSyncStatus {
    Steady,
    CatchingUp,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveSyncState {
    pub target_delay_ms: u64,
    pub observed_lag_ms: u64,
    pub status: LiveSyncStatus,
    pub visible_segment_id: Option<String>,
}

impl Default for LiveSyncState {
    fn default() -> Self {
        Self {
            target_delay_ms: 2_500,
            observed_lag_ms: 0,
            status: LiveSyncStatus::Steady,
            visible_segment_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageSettings {
    pub source: String,
    pub target: String,
    pub explanation: String,
}

impl Default for LanguageSettings {
    fn default() -> Self {
        Self {
            source: "auto".into(),
            target: "en".into(),
            explanation: "en".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSegment {
    pub id: String,
    pub origin: SessionMode,
    pub start_ms: u64,
    pub end_ms: u64,
    pub source_text: String,
    pub translation_text: Option<String>,
    pub ambiguity_note: Option<String>,
    pub speaker_id: Option<String>,
    pub is_provisional: bool,
    pub transcription_status: SegmentStatus,
    pub translation_status: SegmentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerProfile {
    pub id: String,
    pub display_name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecoverableError {
    pub code: String,
    pub message: String,
    pub segment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetranslatedSegment {
    pub segment_id: String,
    pub translation_text: String,
    pub ambiguity_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SessionEvent {
    SessionReset {
        mode: SessionMode,
        languages: LanguageSettings,
        processing_mode: CaptionProcessingMode,
        live_translation_engine: Option<LiveTranslationEngine>,
    },
    PhaseChanged {
        phase: String,
    },
    CaptionUpserted {
        segment: SubtitleSegment,
    },
    TranscriptFinalized {
        segment: SubtitleSegment,
    },
    TranslationFinalized {
        segment_id: String,
        translation_text: String,
        ambiguity_note: Option<String>,
    },
    FileRetranslationApplied {
        languages: LanguageSettings,
        translations: Vec<RetranslatedSegment>,
    },
    SpeakerDiscovered {
        speaker: SpeakerProfile,
    },
    CoverageChanged {
        ready_through_ms: u64,
    },
    LiveSyncChanged {
        sync: LiveSyncState,
    },
    LiveAudioGap {
        start_ms: u64,
        end_ms: u64,
    },
    LessonSelected {
        segment_id: Option<String>,
    },
    RecoverableError {
        error: RecoverableError,
    },
    FatalError {
        message: String,
    },
    Complete,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequencedSessionEvent {
    pub session_id: String,
    pub sequence: u64,
    pub event: SessionEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedMediaInfo {
    pub path: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub session_id: String,
    pub sequence: u64,
    pub mode: Option<SessionMode>,
    pub processing_mode: CaptionProcessingMode,
    pub live_translation_engine: Option<LiveTranslationEngine>,
    pub languages: LanguageSettings,
    pub phase: String,
    pub segments: Vec<SubtitleSegment>,
    pub speakers: HashMap<String, SpeakerProfile>,
    pub ready_through_ms: u64,
    pub live_sync: Option<LiveSyncState>,
    pub errors: Vec<RecoverableError>,
    pub fatal_error: Option<String>,
    pub selected_segment_id: Option<String>,
    pub media: Option<PreparedMediaInfo>,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            session_id: "idle".into(),
            sequence: 0,
            mode: None,
            processing_mode: CaptionProcessingMode::Translated,
            live_translation_engine: None,
            languages: LanguageSettings::default(),
            phase: "idle".into(),
            segments: Vec::new(),
            speakers: HashMap::new(),
            ready_through_ms: 0,
            live_sync: None,
            errors: Vec::new(),
            fatal_error: None,
            selected_segment_id: None,
            media: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoardSection {
    pub heading: String,
    pub lines: Vec<ChalkPhrase>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChalkColor {
    White,
    BabyBlue,
    Yellow,
    Pink,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChalkMark {
    None,
    Box,
    Bracket,
    Strike,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TailCue {
    None,
    Point,
    Underline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChalkPhrase {
    pub text: String,
    pub color: ChalkColor,
    pub mark: ChalkMark,
    pub tail_cue: TailCue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFocus {
    pub color: ChalkColor,
    pub tail_cue: TailCue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoardDemoKind {
    None,
    SentenceBreakdown,
    OmittedMeaning,
    LiteralToNatural,
    ToneScale,
    MiniDialogue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NonoGesture {
    #[default]
    Neutral,
    ThumbsUp,
    PointUser,
    PointSelf,
    Cheer,
    HeartTouch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoardDemoItem {
    pub label: String,
    pub detail: String,
    pub color: ChalkColor,
    pub mark: ChalkMark,
    pub tail_cue: TailCue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoardDemo {
    pub kind: BoardDemoKind,
    pub caption: Option<String>,
    pub items: Vec<BoardDemoItem>,
    pub result: Option<ChalkPhrase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TeachingMoment {
    pub title: String,
    pub speech_bubble: String,
    #[serde(default)]
    pub gesture: NonoGesture,
    pub source_focus: SourceFocus,
    pub board_sections: Vec<BoardSection>,
    pub demonstration: BoardDemo,
    pub ambiguity_note: Option<ChalkPhrase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LessonCard {
    pub schema_version: u8,
    pub selected_segment_id: String,
    pub moments: Vec<TeachingMoment>,
    pub suggested_follow_ups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorMessage {
    pub role: String,
    pub text: String,
}
