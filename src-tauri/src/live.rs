use crate::{
    contracts::{
        CaptionProcessingMode, LanguageSettings, LiveSyncMode, LiveSyncState, LiveSyncStatus,
        RecoverableError, SegmentStatus, SessionEvent, SessionMode, SubtitleSegment,
    },
    openai::{ApiError, ApiErrorKind},
    record_event_for_generation,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use screencapturekit::{
    async_api::{AsyncSCShareableContent, AsyncSCStream},
    cm::CMSampleBufferExt,
    shareable_content::SCShareableContent,
    stream::{
        configuration::SCStreamConfiguration, content_filter::SCContentFilter,
        output_type::SCStreamOutputType,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Error as WebSocketError, Message},
    MaybeTlsStream, WebSocketStream,
};
use unicode_segmentation::UnicodeSegmentation;

const REALTIME_TRANSLATION_URL: &str =
    "wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate";
const REALTIME_TRANSCRIPTION_URL: &str =
    "wss://api.openai.com/v1/realtime?model=gpt-realtime-whisper";
const REALTIME_CONFIGURATION_TIMEOUT: Duration = Duration::from_secs(8);
const SEND_SAMPLES: usize = 2_400;
const TERMINAL_QUIET_MS: u64 = 350;
const IDLE_CLAUSE_MS: u64 = 1_200;
const MAX_ALIGNED_CLAUSE_MS: u64 = 8_000;
const MAX_CAPTURE_CLAUSE_MS: u64 = 10_000;
const MAX_CLAUSE_GRAPHEMES: usize = 220;
const CLAUSE_PAIR_TOLERANCE_MS: u64 = 1_000;
const SOURCE_FALLBACK_MS: u64 = 6_000;
const MAX_RECENT_EVENT_IDS: usize = 2_048;
const MAX_RETAINED_LIVE_UNITS: usize = 256;
const MAX_RETAINED_LIVE_MS: u64 = 120_000;
type RealtimeSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type RealtimeWriter = SplitSink<RealtimeSocket, Message>;
type RealtimeReader = SplitStream<RealtimeSocket>;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveCaptureSourceKind {
    Application,
    Window,
    Display,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveCaptureSourceSelection {
    pub kind: LiveCaptureSourceKind,
    pub process_id: Option<i32>,
    pub window_id: Option<u32>,
    pub display_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveCaptureSource {
    pub id: String,
    pub kind: LiveCaptureSourceKind,
    pub title: String,
    pub detail: String,
    pub application_name: Option<String>,
    pub bundle_identifier: Option<String>,
    pub process_id: Option<i32>,
    pub window_id: Option<u32>,
    pub display_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveCaptureSources {
    pub applications: Vec<LiveCaptureSource>,
    pub windows: Vec<LiveCaptureSource>,
    pub displays: Vec<LiveCaptureSource>,
}

pub struct LiveStartOptions {
    pub api_key: String,
    pub languages: LanguageSettings,
    pub sync_mode: LiveSyncMode,
    pub processing_mode: CaptionProcessingMode,
    pub source: LiveCaptureSourceSelection,
}

#[derive(Debug)]
struct RecentEventIds {
    set: HashSet<String>,
    order: VecDeque<String>,
    capacity: usize,
}

impl Default for RecentEventIds {
    fn default() -> Self {
        Self {
            set: HashSet::new(),
            order: VecDeque::new(),
            capacity: MAX_RECENT_EVENT_IDS,
        }
    }
}

impl RecentEventIds {
    fn insert(&mut self, id: &str) -> bool {
        if self.set.contains(id) {
            return false;
        }
        self.set.insert(id.to_owned());
        self.order.push_back(id.to_owned());
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.set.remove(&oldest);
            }
        }
        true
    }

    fn clear(&mut self) {
        self.set.clear();
        self.order.clear();
    }
}

#[derive(Debug, Default)]
pub struct LiveState {
    cancelled: Arc<AtomicBool>,
    start_sequence: std::sync::atomic::AtomicU64,
    start_lock: tokio::sync::Mutex<()>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimedText {
    event_id: Option<String>,
    text: String,
    elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RealtimeEvent {
    SessionCreated(Value),
    SessionUpdated(Value),
    SourceDelta(TimedText),
    TranslationDelta(TimedText),
    TranscriptionDelta {
        item_id: String,
        event_id: Option<String>,
        text: String,
    },
    TranscriptionDone {
        item_id: String,
        event_id: Option<String>,
        text: String,
    },
    Closed,
    Error(RealtimeServerError),
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RealtimeServerError {
    message: String,
    kind: Option<String>,
    code: Option<String>,
    client_event_id: Option<String>,
}

#[derive(Debug, Default)]
struct ReconnectAllowance {
    used: bool,
}

impl ReconnectAllowance {
    fn claim(&mut self, closing: bool) -> bool {
        if closing || self.used {
            return false;
        }
        self.used = true;
        true
    }
}

#[derive(Debug, Clone)]
struct TextClause {
    epoch: u64,
    group_id: u64,
    timed: bool,
    start_ms: u64,
    end_ms: u64,
    text: String,
    created_at_ms: u64,
    last_at_ms: u64,
    closed_at_ms: Option<u64>,
    closed: bool,
    unit_index: Option<usize>,
}

impl TextClause {
    fn new(epoch: u64, group_id: u64, timed: bool, aligned_ms: u64, capture_clock_ms: u64) -> Self {
        Self {
            epoch,
            group_id,
            timed,
            start_ms: aligned_ms,
            end_ms: aligned_ms.saturating_add(200),
            text: String::new(),
            created_at_ms: capture_clock_ms,
            last_at_ms: capture_clock_ms,
            closed_at_ms: None,
            closed: false,
            unit_index: None,
        }
    }

    fn should_rotate_before(&self, aligned_ms: u64, capture_clock_ms: u64) -> bool {
        !self.text.is_empty()
            && (aligned_ms.saturating_sub(self.start_ms) >= MAX_ALIGNED_CLAUSE_MS
                || capture_clock_ms.saturating_sub(self.created_at_ms) >= MAX_CAPTURE_CLAUSE_MS
                || self.text.graphemes(true).count() >= MAX_CLAUSE_GRAPHEMES)
    }

    fn should_close(&self, capture_clock_ms: u64) -> bool {
        if self.text.is_empty() {
            return false;
        }
        let quiet_ms = capture_clock_ms.saturating_sub(self.last_at_ms);
        self.end_ms.saturating_sub(self.start_ms) >= MAX_ALIGNED_CLAUSE_MS
            || capture_clock_ms.saturating_sub(self.created_at_ms) >= MAX_CAPTURE_CLAUSE_MS
            || self.text.graphemes(true).count() >= MAX_CLAUSE_GRAPHEMES
            || quiet_ms >= IDLE_CLAUSE_MS
            || (terminal(&self.text) && quiet_ms >= TERMINAL_QUIET_MS)
    }
}

#[derive(Debug)]
struct CaptionDraft {
    id: String,
    epoch: u64,
    source_group_id: u64,
    source_timed: bool,
    source_origin_unit: Option<usize>,
    translation_clause: Option<usize>,
    start_ms: u64,
    end_ms: u64,
    source: String,
    translation: String,
    last_source_at_ms: u64,
    last_translation_at_ms: u64,
    source_closed_at_ms: Option<u64>,
    source_closed: bool,
    translation_closed: bool,
}

impl CaptionDraft {
    fn new(
        id: String,
        epoch: u64,
        source_group_id: u64,
        source_timed: bool,
        aligned_ms: u64,
        capture_clock_ms: u64,
    ) -> Self {
        Self {
            id,
            epoch,
            source_group_id,
            source_timed,
            source_origin_unit: None,
            translation_clause: None,
            start_ms: aligned_ms,
            end_ms: aligned_ms.saturating_add(200),
            source: String::new(),
            translation: String::new(),
            last_source_at_ms: capture_clock_ms,
            last_translation_at_ms: capture_clock_ms,
            source_closed_at_ms: None,
            source_closed: false,
            translation_closed: false,
        }
    }

    fn segment(&self) -> SubtitleSegment {
        SubtitleSegment {
            id: self.id.clone(),
            origin: SessionMode::Live,
            start_ms: self.start_ms,
            end_ms: self.end_ms.max(self.start_ms.saturating_add(250)),
            source_text: self.source.trim().to_owned(),
            translation_text: (!self.translation.trim().is_empty())
                .then(|| self.translation.trim().to_owned()),
            ambiguity_note: None,
            speaker_id: Some("live-audio".into()),
            is_provisional: !self.source_closed,
            transcription_status: if self.source_closed {
                SegmentStatus::Complete
            } else {
                SegmentStatus::Pending
            },
            translation_status: if self.translation_closed && !self.translation.trim().is_empty() {
                SegmentStatus::Complete
            } else {
                SegmentStatus::Pending
            },
        }
    }
}

#[derive(Debug)]
struct LagSample {
    epoch: u64,
    elapsed_ms: u64,
    received_at_ms: u64,
    lag_ms: u64,
}

#[derive(Debug)]
struct LagEstimator {
    samples: VecDeque<LagSample>,
    target_delay_ms: u64,
    observed_lag_ms: u64,
    last_decrease_at_ms: u64,
    status: LiveSyncStatus,
}

impl Default for LagEstimator {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
            target_delay_ms: 2_500,
            observed_lag_ms: 0,
            last_decrease_at_ms: 0,
            status: LiveSyncStatus::Steady,
        }
    }
}

impl LagEstimator {
    fn observe(&mut self, epoch: u64, capture_clock_ms: u64, elapsed_ms: u64, aligned_ms: u64) {
        let lag = capture_clock_ms.saturating_sub(aligned_ms);
        while self
            .samples
            .front()
            .is_some_and(|sample| capture_clock_ms.saturating_sub(sample.received_at_ms) > 30_000)
        {
            self.samples.pop_front();
        }
        if let Some(sample) = self
            .samples
            .iter_mut()
            .find(|sample| sample.epoch == epoch && sample.elapsed_ms == elapsed_ms)
        {
            sample.received_at_ms = capture_clock_ms;
            sample.lag_ms = sample.lag_ms.max(lag);
        } else {
            self.samples.push_back(LagSample {
                epoch,
                elapsed_ms,
                received_at_ms: capture_clock_ms,
                lag_ms: lag,
            });
        }
        let mut values: Vec<u64> = self.samples.iter().map(|sample| sample.lag_ms).collect();
        values.sort_unstable();
        let index = (values.len() * 90).div_ceil(100).saturating_sub(1);
        self.observed_lag_ms = values.get(index).copied().unwrap_or_default();
        let desired_unclamped = self.observed_lag_ms.saturating_add(600);
        let desired = desired_unclamped.clamp(1_500, 6_000);
        if desired > self.target_delay_ms {
            self.target_delay_ms = desired;
            self.last_decrease_at_ms = capture_clock_ms;
        } else if self.target_delay_ms > desired
            && capture_clock_ms.saturating_sub(self.last_decrease_at_ms) >= 10_000
        {
            self.target_delay_ms = self.target_delay_ms.saturating_sub(200).max(desired);
            self.last_decrease_at_ms = capture_clock_ms;
        }
        self.status = if desired_unclamped > 6_000 {
            LiveSyncStatus::Degraded
        } else if lag.saturating_add(300) >= self.target_delay_ms {
            LiveSyncStatus::CatchingUp
        } else {
            LiveSyncStatus::Steady
        };
    }
}

#[derive(Debug)]
struct CaptionCoordinator {
    mode: LiveSyncMode,
    epoch: u64,
    epoch_offset_ms: u64,
    segment_index: u64,
    source_group_index: u64,
    translation_group_index: u64,
    drafts: VecDeque<CaptionDraft>,
    source_clauses: Vec<TextClause>,
    translation_clauses: Vec<TextClause>,
    active_source_clause: Option<usize>,
    active_translation_clause: Option<usize>,
    unpaired_translation_clauses: VecDeque<usize>,
    translation_pair_cursor: usize,
    translation_group_units: HashMap<u64, usize>,
    paired_source_groups: HashSet<u64>,
    seen_event_ids: RecentEventIds,
    lag: LagEstimator,
    display_cursor_ms: u64,
    visible_segment_id: Option<String>,
    last_emitted_sync: Option<LiveSyncState>,
}

impl CaptionCoordinator {
    fn new(mode: LiveSyncMode) -> Self {
        Self {
            mode,
            epoch: 0,
            epoch_offset_ms: 0,
            segment_index: 1,
            source_group_index: 1,
            translation_group_index: 1,
            drafts: VecDeque::new(),
            source_clauses: Vec::new(),
            translation_clauses: Vec::new(),
            active_source_clause: None,
            active_translation_clause: None,
            unpaired_translation_clauses: VecDeque::new(),
            translation_pair_cursor: 0,
            translation_group_units: HashMap::new(),
            paired_source_groups: HashSet::new(),
            seen_event_ids: RecentEventIds::default(),
            lag: LagEstimator::default(),
            display_cursor_ms: 0,
            visible_segment_id: None,
            last_emitted_sync: None,
        }
    }

    fn next_source_group(&mut self) -> u64 {
        let group = self.source_group_index;
        self.source_group_index = self.source_group_index.saturating_add(1);
        group
    }

    fn next_translation_group(&mut self) -> u64 {
        let group = self.translation_group_index;
        self.translation_group_index = self.translation_group_index.saturating_add(1);
        group
    }

    fn next_source_clause(
        &mut self,
        group_id: u64,
        timed: bool,
        aligned_ms: u64,
        capture_clock_ms: u64,
    ) -> usize {
        let clause_index = self.source_clauses.len();
        self.source_clauses.push(TextClause::new(
            self.epoch,
            group_id,
            timed,
            aligned_ms,
            capture_clock_ms,
        ));
        let id = format!("live-{}", self.segment_index);
        self.segment_index += 1;
        let unit_index = self.drafts.len();
        self.drafts.push_back(CaptionDraft::new(
            id,
            self.epoch,
            group_id,
            timed,
            aligned_ms,
            capture_clock_ms,
        ));
        self.source_clauses[clause_index].unit_index = Some(unit_index);
        self.active_source_clause = Some(clause_index);
        clause_index
    }

    fn accept(&mut self, event_id: &Option<String>) -> bool {
        match event_id {
            Some(id) if !id.is_empty() => self.seen_event_ids.insert(id),
            _ => false,
        }
    }

    fn aligned_ms(&self, elapsed_ms: Option<u64>, capture_clock_ms: u64) -> u64 {
        elapsed_ms.map_or(capture_clock_ms, |elapsed| {
            self.epoch_offset_ms.saturating_add(elapsed)
        })
    }

    fn next_translation_clause(
        &mut self,
        group_id: u64,
        timed: bool,
        aligned_ms: u64,
        capture_clock_ms: u64,
    ) -> usize {
        let clause_index = self.translation_clauses.len();
        self.translation_clauses.push(TextClause::new(
            self.epoch,
            group_id,
            timed,
            aligned_ms,
            capture_clock_ms,
        ));
        self.active_translation_clause = Some(clause_index);
        if let Some(base_unit) = self.translation_group_units.get(&group_id).copied() {
            let unit_index = self.next_translation_continuation(base_unit, capture_clock_ms);
            self.bind_translation(clause_index, unit_index);
        } else if let Some(unit_index) = self.claim_next_translation_unit(clause_index) {
            self.bind_translation(clause_index, unit_index);
        } else {
            self.unpaired_translation_clauses.push_back(clause_index);
        }
        clause_index
    }

    fn translation_compatible(&self, unit_index: usize, clause_index: usize) -> bool {
        let draft = &self.drafts[unit_index];
        let clause = &self.translation_clauses[clause_index];
        draft.epoch == clause.epoch
            && (!draft.source_timed
                || !clause.timed
                || (clause.start_ms <= draft.end_ms.saturating_add(CLAUSE_PAIR_TOLERANCE_MS)
                    && draft.start_ms <= clause.end_ms.saturating_add(CLAUSE_PAIR_TOLERANCE_MS)))
    }

    fn claim_next_translation_unit(&mut self, clause_index: usize) -> Option<usize> {
        let clause = self.translation_clauses[clause_index].clone();
        while self.translation_pair_cursor < self.drafts.len() {
            let index = self.translation_pair_cursor;
            let draft = &self.drafts[index];
            if draft.epoch > clause.epoch {
                return None;
            }
            if draft.epoch < clause.epoch
                || draft.translation_clause.is_some()
                || self.paired_source_groups.contains(&draft.source_group_id)
            {
                self.translation_pair_cursor += 1;
                continue;
            }
            if self.translation_compatible(index, clause_index) {
                self.translation_pair_cursor += 1;
                return Some(index);
            }
            if draft.source_timed
                && clause.timed
                && draft.end_ms.saturating_add(CLAUSE_PAIR_TOLERANCE_MS) < clause.start_ms
            {
                self.translation_pair_cursor += 1;
                continue;
            }
            return None;
        }
        None
    }

    fn flush_pairable_translations(&mut self, events: &mut Vec<SessionEvent>) {
        loop {
            let Some(clause_index) = self.unpaired_translation_clauses.front().copied() else {
                break;
            };
            let group_id = self.translation_clauses[clause_index].group_id;
            let unit_index = if let Some(base_unit) = self.translation_group_units.get(&group_id) {
                Some(self.next_translation_continuation(
                    *base_unit,
                    self.translation_clauses[clause_index].last_at_ms,
                ))
            } else {
                self.claim_next_translation_unit(clause_index)
            };
            let Some(unit_index) = unit_index else {
                let clause = &self.translation_clauses[clause_index];
                let permanently_before_next_source = self
                    .drafts
                    .get(self.translation_pair_cursor)
                    .is_some_and(|draft| {
                        draft.epoch == clause.epoch
                            && draft.source_timed
                            && clause.timed
                            && clause.end_ms.saturating_add(CLAUSE_PAIR_TOLERANCE_MS)
                                < draft.start_ms
                    });
                if permanently_before_next_source {
                    self.unpaired_translation_clauses.pop_front();
                    continue;
                }
                break;
            };
            self.unpaired_translation_clauses.pop_front();
            self.bind_translation(clause_index, unit_index);
            if let Some(event) = self.upsert_for_clause(false, clause_index) {
                events.push(event);
            }
        }
    }

    fn next_translation_continuation(&mut self, base_unit: usize, capture_clock_ms: u64) -> usize {
        let base = &self.drafts[base_unit];
        let id = format!("live-{}", self.segment_index);
        self.segment_index = self.segment_index.saturating_add(1);
        let mut continuation = CaptionDraft::new(
            id,
            base.epoch,
            base.source_group_id,
            base.source_timed,
            base.start_ms,
            capture_clock_ms,
        );
        continuation.start_ms = base.start_ms;
        continuation.end_ms = base.end_ms;
        continuation.source = base.source.clone();
        continuation.last_source_at_ms = base.last_source_at_ms;
        continuation.source_closed_at_ms = base.source_closed_at_ms;
        continuation.source_closed = base.source_closed;
        continuation.source_origin_unit = Some(base.source_origin_unit.unwrap_or(base_unit));
        self.drafts.push_back(continuation);
        self.drafts.len() - 1
    }

    fn bind_translation(&mut self, clause_index: usize, unit_index: usize) {
        let group_id = self.translation_clauses[clause_index].group_id;
        self.translation_clauses[clause_index].unit_index = Some(unit_index);
        self.drafts[unit_index].translation_clause = Some(clause_index);
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.translation_group_units.entry(group_id)
        {
            entry.insert(unit_index);
            self.paired_source_groups
                .insert(self.drafts[unit_index].source_group_id);
        }
        self.sync_translation_unit(clause_index);
    }

    fn attach_unpaired_translation_continuation(&mut self, clause_index: usize) {
        if self.translation_clauses[clause_index].unit_index.is_some() {
            return;
        }
        let base_unit = self
            .drafts
            .iter()
            .enumerate()
            .rev()
            .find(|(index, draft)| {
                draft.translation_clause.is_some()
                    && self.translation_compatible(*index, clause_index)
            })
            .map(|(index, _)| index);
        let Some(base_unit) = base_unit else {
            return;
        };
        let group_id = self.translation_clauses[clause_index].group_id;
        self.translation_group_units.insert(group_id, base_unit);
        self.unpaired_translation_clauses
            .retain(|index| *index != clause_index);
        let unit_index = self.next_translation_continuation(
            base_unit,
            self.translation_clauses[clause_index].last_at_ms,
        );
        self.bind_translation(clause_index, unit_index);
    }

    fn sync_source_unit(&mut self, clause_index: usize) {
        let clause = self.source_clauses[clause_index].clone();
        let Some(unit_index) = clause.unit_index else {
            return;
        };
        let draft = &mut self.drafts[unit_index];
        draft.start_ms = clause.start_ms;
        draft.end_ms = clause.end_ms;
        draft.source = clause.text;
        draft.last_source_at_ms = clause.last_at_ms;
        draft.source_closed_at_ms = clause.closed_at_ms;
        draft.source_closed = clause.closed;
        let source = draft.source.clone();
        let start_ms = draft.start_ms;
        let end_ms = draft.end_ms;
        let last_source_at_ms = draft.last_source_at_ms;
        let source_closed_at_ms = draft.source_closed_at_ms;
        let source_closed = draft.source_closed;
        for continuation in self
            .drafts
            .iter_mut()
            .filter(|candidate| candidate.source_origin_unit == Some(unit_index))
        {
            continuation.source = source.clone();
            continuation.start_ms = start_ms;
            continuation.end_ms = end_ms;
            continuation.last_source_at_ms = last_source_at_ms;
            continuation.source_closed_at_ms = source_closed_at_ms;
            continuation.source_closed = source_closed;
        }
    }

    fn sync_translation_unit(&mut self, clause_index: usize) {
        let clause = self.translation_clauses[clause_index].clone();
        let Some(unit_index) = clause.unit_index else {
            return;
        };
        let draft = &mut self.drafts[unit_index];
        draft.translation = clause.text;
        draft.last_translation_at_ms = clause.last_at_ms;
        draft.translation_closed = clause.closed;
    }

    fn upsert_for_clause(&self, source: bool, clause_index: usize) -> Option<SessionEvent> {
        let unit_index = if source {
            self.source_clauses[clause_index].unit_index
        } else {
            self.translation_clauses[clause_index].unit_index
        }?;
        let draft = &self.drafts[unit_index];
        (!draft.source.trim().is_empty()).then(|| SessionEvent::CaptionUpserted {
            segment: draft.segment(),
        })
    }

    fn source_continuation_events(&self, source_unit: usize, finalized: bool) -> Vec<SessionEvent> {
        self.drafts
            .iter()
            .filter(|draft| draft.source_origin_unit == Some(source_unit))
            .map(|draft| {
                if finalized {
                    SessionEvent::TranscriptFinalized {
                        segment: draft.segment(),
                    }
                } else {
                    SessionEvent::CaptionUpserted {
                        segment: draft.segment(),
                    }
                }
            })
            .collect()
    }

    fn close_source_clause(
        &mut self,
        clause_index: usize,
        capture_clock_ms: u64,
        events: &mut Vec<SessionEvent>,
    ) {
        if self.source_clauses[clause_index].closed {
            return;
        }
        self.source_clauses[clause_index].closed = true;
        self.source_clauses[clause_index].closed_at_ms = Some(capture_clock_ms);
        if self.active_source_clause == Some(clause_index) {
            self.active_source_clause = None;
        }
        self.sync_source_unit(clause_index);
        if let Some(unit_index) = self.source_clauses[clause_index].unit_index {
            if !self.drafts[unit_index].source.trim().is_empty() {
                events.push(SessionEvent::TranscriptFinalized {
                    segment: self.drafts[unit_index].segment(),
                });
                events.extend(self.source_continuation_events(unit_index, true));
            }
        }
    }

    fn close_translation_clause(
        &mut self,
        clause_index: usize,
        capture_clock_ms: u64,
        events: &mut Vec<SessionEvent>,
    ) {
        if self.translation_clauses[clause_index].closed {
            return;
        }
        self.translation_clauses[clause_index].closed = true;
        self.translation_clauses[clause_index].closed_at_ms = Some(capture_clock_ms);
        if self.active_translation_clause == Some(clause_index) {
            self.active_translation_clause = None;
        }
        self.attach_unpaired_translation_continuation(clause_index);
        self.sync_translation_unit(clause_index);
        if let Some(event) = self.upsert_for_clause(false, clause_index) {
            events.push(event);
        }
    }

    fn on_source(&mut self, delta: TimedText, capture_clock_ms: u64) -> Vec<SessionEvent> {
        if !self.accept(&delta.event_id) || delta.text.is_empty() {
            return Vec::new();
        }
        let aligned_ms = self.aligned_ms(delta.elapsed_ms, capture_clock_ms);
        let timed = delta.elapsed_ms.is_some();
        let mut events = Vec::new();
        let mut remaining = delta.text;
        let mut continuation_group = None;
        while !remaining.is_empty() {
            if self.active_source_clause.is_some_and(|index| {
                self.source_clauses[index].should_close(capture_clock_ms)
                    || self.source_clauses[index].should_rotate_before(aligned_ms, capture_clock_ms)
            }) {
                let index = self.active_source_clause.expect("active source clause");
                self.close_source_clause(index, capture_clock_ms, &mut events);
            }
            let clause_index = if let Some(index) = self.active_source_clause {
                index
            } else {
                let group_id = continuation_group
                    .take()
                    .unwrap_or_else(|| self.next_source_group());
                self.next_source_clause(group_id, timed, aligned_ms, capture_clock_ms)
            };
            let used = self.source_clauses[clause_index]
                .text
                .graphemes(true)
                .count();
            let budget = MAX_CLAUSE_GRAPHEMES.saturating_sub(used).max(1);
            let (head, tail) = split_for_grapheme_budget(&remaining, budget);
            {
                let clause = &mut self.source_clauses[clause_index];
                clause.text.push_str(&head);
                clause.end_ms = clause.end_ms.max(aligned_ms.saturating_add(200));
                clause.last_at_ms = capture_clock_ms;
            }
            self.sync_source_unit(clause_index);
            if let Some(event) = self.upsert_for_clause(true, clause_index) {
                events.push(event);
            }
            if let Some(unit_index) = self.source_clauses[clause_index].unit_index {
                events.extend(self.source_continuation_events(unit_index, false));
            }
            self.flush_pairable_translations(&mut events);
            remaining = tail;
            if !remaining.is_empty()
                || self.source_clauses[clause_index]
                    .text
                    .graphemes(true)
                    .count()
                    >= MAX_CLAUSE_GRAPHEMES
            {
                continuation_group = Some(self.source_clauses[clause_index].group_id);
                self.close_source_clause(clause_index, capture_clock_ms, &mut events);
            }
        }
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn on_translation(&mut self, delta: TimedText, capture_clock_ms: u64) -> Vec<SessionEvent> {
        if !self.accept(&delta.event_id) || delta.text.is_empty() {
            return Vec::new();
        }
        let aligned_ms = self.aligned_ms(delta.elapsed_ms, capture_clock_ms);
        let timed = delta.elapsed_ms.is_some();
        if let Some(elapsed_ms) = delta.elapsed_ms {
            self.lag
                .observe(self.epoch, capture_clock_ms, elapsed_ms, aligned_ms);
        }
        let mut events = Vec::new();
        let mut remaining = delta.text;
        let mut continuation_group = None;
        while !remaining.is_empty() {
            if self.active_translation_clause.is_some_and(|index| {
                self.translation_clauses[index].should_close(capture_clock_ms)
                    || self.translation_clauses[index]
                        .should_rotate_before(aligned_ms, capture_clock_ms)
            }) {
                let index = self
                    .active_translation_clause
                    .expect("active translation clause");
                self.close_translation_clause(index, capture_clock_ms, &mut events);
            }
            let clause_index = if let Some(index) = self.active_translation_clause {
                index
            } else {
                let group_id = continuation_group
                    .take()
                    .unwrap_or_else(|| self.next_translation_group());
                self.next_translation_clause(group_id, timed, aligned_ms, capture_clock_ms)
            };
            let used = self.translation_clauses[clause_index]
                .text
                .graphemes(true)
                .count();
            let budget = MAX_CLAUSE_GRAPHEMES.saturating_sub(used).max(1);
            let (head, tail) = split_for_grapheme_budget(&remaining, budget);
            {
                let clause = &mut self.translation_clauses[clause_index];
                clause.text.push_str(&head);
                clause.end_ms = clause.end_ms.max(aligned_ms.saturating_add(200));
                clause.last_at_ms = capture_clock_ms;
            }
            self.sync_translation_unit(clause_index);
            if let Some(event) = self.upsert_for_clause(false, clause_index) {
                events.push(event);
            }
            remaining = tail;
            if !remaining.is_empty()
                || self.translation_clauses[clause_index]
                    .text
                    .graphemes(true)
                    .count()
                    >= MAX_CLAUSE_GRAPHEMES
            {
                continuation_group = Some(self.translation_clauses[clause_index].group_id);
                self.close_translation_clause(clause_index, capture_clock_ms, &mut events);
            }
        }
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn tick(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if self
            .active_source_clause
            .is_some_and(|index| self.source_clauses[index].should_close(capture_clock_ms))
        {
            let index = self.active_source_clause.expect("active source clause");
            self.close_source_clause(index, capture_clock_ms, &mut events);
        }
        if self
            .active_translation_clause
            .is_some_and(|index| self.translation_clauses[index].should_close(capture_clock_ms))
        {
            let index = self
                .active_translation_clause
                .expect("active translation clause");
            self.close_translation_clause(index, capture_clock_ms, &mut events);
        }
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn advance(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        self.display_cursor_ms = self
            .display_cursor_ms
            .max(capture_clock_ms.saturating_sub(self.lag.target_delay_ms));
        let candidate = match self.mode {
            LiveSyncMode::FastSource => self
                .drafts
                .iter()
                .rev()
                .find(|draft| !draft.source.trim().is_empty())
                .map(|draft| draft.id.clone()),
            LiveSyncMode::Coordinated => self
                .drafts
                .iter()
                .rev()
                .find(|draft| {
                    let fallback = draft.source_closed_at_ms.is_some_and(|closed| {
                        capture_clock_ms.saturating_sub(closed) >= SOURCE_FALLBACK_MS
                    });
                    draft.source_closed
                        && draft.start_ms <= self.display_cursor_ms
                        && ((!draft.translation.trim().is_empty() && draft.translation_closed)
                            || fallback)
                })
                .map(|draft| draft.id.clone()),
        };
        // Hold the last released caption until a newer one satisfies the current
        // mode's release rules. This prevents the watching overlay from flashing
        // to a generic waiting state between otherwise continuous clauses.
        let next_visible = candidate.or_else(|| self.visible_segment_id.clone());
        if next_visible != self.visible_segment_id {
            self.visible_segment_id = next_visible;
        }
        let fallback_visible = self
            .visible_segment_id
            .as_ref()
            .and_then(|id| self.drafts.iter().find(|draft| &draft.id == id))
            .is_some_and(|draft| draft.translation.trim().is_empty());
        let sync = LiveSyncState {
            target_delay_ms: self.lag.target_delay_ms,
            observed_lag_ms: self.lag.observed_lag_ms,
            status: if fallback_visible {
                LiveSyncStatus::Degraded
            } else {
                self.lag.status.clone()
            },
            visible_segment_id: self.visible_segment_id.clone(),
        };
        let events = if self.last_emitted_sync.as_ref() == Some(&sync) {
            Vec::new()
        } else {
            self.last_emitted_sync = Some(sync.clone());
            vec![SessionEvent::LiveSyncChanged { sync }]
        };
        self.prune_history(capture_clock_ms);
        events
    }

    fn prune_history(&mut self, capture_clock_ms: u64) {
        let cutoff = capture_clock_ms.saturating_sub(MAX_RETAINED_LIVE_MS);
        let mut remove_drafts = 0;
        while remove_drafts < self.drafts.len() {
            let draft = &self.drafts[remove_drafts];
            let outside_time_window = draft.end_ms < cutoff;
            let outside_count_window = self.drafts.len() - remove_drafts > MAX_RETAINED_LIVE_UNITS;
            let translation_settled = draft.translation_closed
                || (draft.translation.is_empty()
                    && draft.source_closed_at_ms.is_some_and(|closed| {
                        capture_clock_ms.saturating_sub(closed) >= SOURCE_FALLBACK_MS
                    }));
            if (!outside_time_window && !outside_count_window)
                || !draft.source_closed
                || !translation_settled
                || self.visible_segment_id.as_deref() == Some(draft.id.as_str())
            {
                break;
            }
            remove_drafts += 1;
        }
        if remove_drafts == 0 {
            return;
        }

        self.drafts.drain(..remove_drafts);
        self.translation_pair_cursor = self.translation_pair_cursor.saturating_sub(remove_drafts);
        for draft in &mut self.drafts {
            draft.source_origin_unit = draft
                .source_origin_unit
                .and_then(|index| index.checked_sub(remove_drafts));
        }
        for clause in &mut self.source_clauses {
            clause.unit_index = clause
                .unit_index
                .and_then(|index| index.checked_sub(remove_drafts));
        }
        for clause in &mut self.translation_clauses {
            clause.unit_index = clause
                .unit_index
                .and_then(|index| index.checked_sub(remove_drafts));
        }
        self.translation_group_units.retain(|_, unit| {
            if let Some(adjusted) = unit.checked_sub(remove_drafts) {
                *unit = adjusted;
                true
            } else {
                false
            }
        });
        self.paired_source_groups = self
            .drafts
            .iter()
            .filter(|draft| draft.translation_clause.is_some())
            .map(|draft| draft.source_group_id)
            .collect();

        let mut source_map = vec![None; self.source_clauses.len()];
        let mut retained_sources = Vec::new();
        for (index, clause) in self.source_clauses.drain(..).enumerate() {
            if clause.unit_index.is_some()
                || self.active_source_clause == Some(index)
                || !clause.closed
            {
                source_map[index] = Some(retained_sources.len());
                retained_sources.push(clause);
            }
        }
        self.source_clauses = retained_sources;
        self.active_source_clause = self
            .active_source_clause
            .and_then(|index| source_map.get(index).copied().flatten());

        let unpaired = self
            .unpaired_translation_clauses
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let mut translation_map = vec![None; self.translation_clauses.len()];
        let mut retained_translations = Vec::new();
        for (index, clause) in self.translation_clauses.drain(..).enumerate() {
            let recent_unpaired = unpaired.contains(&index)
                && (!clause.closed || clause.end_ms >= cutoff);
            if clause.unit_index.is_some()
                || self.active_translation_clause == Some(index)
                || recent_unpaired
            {
                translation_map[index] = Some(retained_translations.len());
                retained_translations.push(clause);
            }
        }
        self.translation_clauses = retained_translations;
        self.active_translation_clause = self
            .active_translation_clause
            .and_then(|index| translation_map.get(index).copied().flatten());
        self.unpaired_translation_clauses = self
            .unpaired_translation_clauses
            .drain(..)
            .filter_map(|index| translation_map.get(index).copied().flatten())
            .collect();
        for draft in &mut self.drafts {
            draft.translation_clause = draft
                .translation_clause
                .and_then(|index| translation_map.get(index).copied().flatten());
        }
    }

    fn begin_epoch(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if let Some(index) = self.active_source_clause {
            self.close_source_clause(index, capture_clock_ms, &mut events);
        }
        if let Some(index) = self.active_translation_clause {
            self.close_translation_clause(index, capture_clock_ms, &mut events);
        }
        self.epoch = self.epoch.saturating_add(1);
        self.epoch_offset_ms = capture_clock_ms;
        self.active_source_clause = None;
        self.active_translation_clause = None;
        self.unpaired_translation_clauses.clear();
        self.translation_pair_cursor = self.drafts.len();
        self.seen_event_ids.clear();
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn finish(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if let Some(index) = self.active_source_clause {
            self.close_source_clause(index, capture_clock_ms, &mut events);
        }
        if let Some(index) = self.active_translation_clause {
            self.close_translation_clause(index, capture_clock_ms, &mut events);
        }
        self.visible_segment_id = None;
        let sync = LiveSyncState {
            target_delay_ms: self.lag.target_delay_ms,
            observed_lag_ms: self.lag.observed_lag_ms,
            status: self.lag.status.clone(),
            visible_segment_id: None,
        };
        if self.last_emitted_sync.as_ref() != Some(&sync) {
            self.last_emitted_sync = Some(sync.clone());
            events.push(SessionEvent::LiveSyncChanged { sync });
        }
        events
    }
}

#[derive(Debug, Default)]
struct TranscriptionCoordinator {
    segments: HashMap<String, SubtitleSegment>,
    seen_event_ids: RecentEventIds,
    next_segment: u64,
}

impl TranscriptionCoordinator {
    fn accept(&mut self, event_id: &Option<String>) -> bool {
        match event_id {
            Some(id) if !id.is_empty() => self.seen_event_ids.insert(id),
            _ => true,
        }
    }

    fn segment_mut(&mut self, item_id: &str, capture_clock_ms: u64) -> &mut SubtitleSegment {
        self.segments.entry(item_id.to_owned()).or_insert_with(|| {
            self.next_segment += 1;
            SubtitleSegment {
                id: format!("live-original-{}", self.next_segment),
                origin: SessionMode::Live,
                start_ms: capture_clock_ms,
                end_ms: capture_clock_ms.saturating_add(250),
                source_text: String::new(),
                translation_text: None,
                ambiguity_note: None,
                speaker_id: Some("live-audio".into()),
                is_provisional: true,
                transcription_status: SegmentStatus::Pending,
                translation_status: SegmentStatus::Skipped,
            }
        })
    }

    fn sync(segment_id: String) -> SessionEvent {
        SessionEvent::LiveSyncChanged {
            sync: LiveSyncState {
                target_delay_ms: 0,
                observed_lag_ms: 0,
                status: LiveSyncStatus::Steady,
                visible_segment_id: Some(segment_id),
            },
        }
    }

    fn on_delta(
        &mut self,
        item_id: String,
        event_id: Option<String>,
        text: String,
        capture_clock_ms: u64,
    ) -> Vec<SessionEvent> {
        if text.is_empty() || !self.accept(&event_id) {
            return Vec::new();
        }
        let segment = self.segment_mut(&item_id, capture_clock_ms);
        segment.source_text.push_str(&text);
        segment.end_ms = capture_clock_ms.max(segment.start_ms.saturating_add(250));
        vec![
            SessionEvent::CaptionUpserted {
                segment: segment.clone(),
            },
            Self::sync(segment.id.clone()),
        ]
    }

    fn on_done(
        &mut self,
        item_id: String,
        event_id: Option<String>,
        text: String,
        capture_clock_ms: u64,
    ) -> Vec<SessionEvent> {
        if !self.accept(&event_id) {
            return Vec::new();
        }
        let segment = self.segment_mut(&item_id, capture_clock_ms);
        if !text.trim().is_empty() {
            segment.source_text = text;
        }
        if segment.source_text.trim().is_empty() {
            self.segments.remove(&item_id);
            return Vec::new();
        }
        segment.end_ms = capture_clock_ms.max(segment.start_ms.saturating_add(250));
        segment.is_provisional = false;
        segment.transcription_status = SegmentStatus::Complete;
        let finalized = segment.clone();
        let sync_id = finalized.id.clone();
        self.segments.remove(&item_id);
        vec![
            SessionEvent::TranscriptFinalized {
                segment: finalized,
            },
            Self::sync(sync_id),
        ]
    }

    fn finish(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        for segment in self
            .segments
            .values_mut()
            .filter(|segment| segment.is_provisional)
        {
            if segment.source_text.trim().is_empty() {
                continue;
            }
            segment.end_ms = capture_clock_ms.max(segment.start_ms.saturating_add(250));
            segment.is_provisional = false;
            segment.transcription_status = SegmentStatus::Complete;
            events.push(SessionEvent::TranscriptFinalized {
                segment: segment.clone(),
            });
        }
        events
    }
}

#[derive(Debug)]
enum TranscriptionAudioAction {
    Append(CapturedPcm),
    Commit,
}

#[derive(Debug, Clone)]
struct CapturedPcm {
    samples: Vec<i16>,
    capture_start_sample: u64,
    capture_end_sample: u64,
}

#[derive(Debug)]
struct SpeechCommitter {
    preroll: VecDeque<CapturedPcm>,
    active: bool,
    speech_frames: usize,
    quiet_frames: usize,
    buffered_frames: usize,
    noise_floor: f64,
}

impl Default for SpeechCommitter {
    fn default() -> Self {
        Self {
            preroll: VecDeque::new(),
            active: false,
            speech_frames: 0,
            quiet_frames: 0,
            buffered_frames: 0,
            noise_floor: 120.0,
        }
    }
}

impl SpeechCommitter {
    fn push(&mut self, frame: CapturedPcm) -> Vec<TranscriptionAudioAction> {
        let rms = rms(&frame.samples);
        let threshold = (self.noise_floor * 3.0).max(500.0);
        let speech = rms >= threshold;
        let mut actions = Vec::new();

        if !self.active {
            if !speech {
                self.noise_floor = self.noise_floor * 0.96 + rms * 0.04;
            }
            self.preroll.push_back(frame);
            while self.preroll.len() > 3 {
                self.preroll.pop_front();
            }
            self.speech_frames = if speech { self.speech_frames + 1 } else { 0 };
            if self.speech_frames >= 2 {
                self.active = true;
                self.quiet_frames = 0;
                self.buffered_frames = self.preroll.len();
                for frame in self.preroll.drain(..) {
                    actions.push(TranscriptionAudioAction::Append(frame));
                }
            }
            return actions;
        }

        self.buffered_frames += 1;
        self.quiet_frames = if speech { 0 } else { self.quiet_frames + 1 };
        actions.push(TranscriptionAudioAction::Append(frame));
        if (self.quiet_frames >= 4 && self.buffered_frames >= 6) || self.buffered_frames >= 30 {
            actions.push(TranscriptionAudioAction::Commit);
            self.active = false;
            self.speech_frames = 0;
            self.quiet_frames = 0;
            self.buffered_frames = 0;
            self.preroll.clear();
        }
        actions
    }

    fn finish(&mut self) -> bool {
        let should_commit = self.active && self.buffered_frames > 0;
        *self = Self::default();
        should_commit
    }
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean = samples
        .iter()
        .map(|sample| {
            let sample = f64::from(*sample);
            sample * sample
        })
        .sum::<f64>()
        / samples.len() as f64;
    mean.sqrt()
}

fn terminal(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '?' | '!' | '。' | '？' | '！' | '…'))
}

fn split_for_grapheme_budget(text: &str, budget: usize) -> (String, String) {
    let graphemes: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    if graphemes.len() <= budget {
        return (text.to_owned(), String::new());
    }

    let exact_end = graphemes
        .get(budget)
        .map(|(offset, _)| *offset)
        .unwrap_or(text.len());
    let minimum_boundary = budget.saturating_mul(3) / 5;
    let semantic_end = graphemes
        .iter()
        .take(budget)
        .enumerate()
        .rev()
        .find(|(index, (_, grapheme))| *index >= minimum_boundary && grapheme_boundary(grapheme))
        .map(|(_, (offset, grapheme))| offset + grapheme.len());
    let split_at = semantic_end.unwrap_or(exact_end).max(1);
    (text[..split_at].to_owned(), text[split_at..].to_owned())
}

fn grapheme_boundary(grapheme: &str) -> bool {
    grapheme.chars().last().is_some_and(|character| {
        character.is_whitespace()
            || matches!(
                character,
                '.' | '?'
                    | '!'
                    | ','
                    | ';'
                    | ':'
                    | '。'
                    | '？'
                    | '！'
                    | '、'
                    | '，'
                    | '；'
                    | '：'
                    | '…'
            )
    })
}

fn emit_events(app: &tauri::AppHandle, generation: u64, events: Vec<SessionEvent>) {
    for event in events {
        let _ = record_event_for_generation(app, generation, event);
    }
}

fn safe_source_label(value: &str, fallback: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned.chars().take(120).collect()
    }
}

fn eligible_window(title: Option<&str>, width: f64, height: f64, layer: i32) -> bool {
    layer == 0
        && width >= 160.0
        && height >= 90.0
        && title.is_some_and(|title| !title.trim().is_empty())
}

pub async fn list_capture_sources() -> Result<LiveCaptureSources, ApiError> {
    let content = AsyncSCShareableContent::create()
        .with_exclude_desktop_windows(true)
        .with_on_screen_windows_only(true)
        .get()
        .await
        .map_err(|error| {
            capture_error(&format!(
                "NonoSub could not list shareable apps and windows. Check Screen & System Audio Recording permission: {error}"
            ))
        })?;
    Ok(capture_sources_from_content(&content))
}

fn capture_sources_from_content(content: &SCShareableContent) -> LiveCaptureSources {
    let current_process = i32::try_from(std::process::id()).unwrap_or_default();
    let windows = content.windows();
    let mut visible_window_counts = HashMap::<i32, usize>::new();
    let mut window_sources = Vec::new();

    for window in &windows {
        let frame = window.frame();
        let owner = window.owning_application();
        let process_id = owner.as_ref().map_or(0, |app| app.process_id());
        let raw_title = window.title();
        if process_id == current_process
            || !eligible_window(
                raw_title.as_deref(),
                frame.size.width,
                frame.size.height,
                window.window_layer(),
            )
        {
            continue;
        }
        *visible_window_counts.entry(process_id).or_default() += 1;
        let app_name = owner
            .as_ref()
            .map(|app| safe_source_label(&app.application_name(), "Application"));
        let title = safe_source_label(raw_title.as_deref().unwrap_or_default(), "Untitled window");
        window_sources.push(LiveCaptureSource {
            id: format!("window:{}", window.window_id()),
            kind: LiveCaptureSourceKind::Window,
            title,
            detail: format!(
                "{} · {}×{}",
                app_name.as_deref().unwrap_or("Application"),
                frame.size.width.round() as i64,
                frame.size.height.round() as i64
            ),
            application_name: app_name,
            bundle_identifier: owner.as_ref().map(|app| app.bundle_identifier()),
            process_id: Some(process_id),
            window_id: Some(window.window_id()),
            display_id: None,
        });
    }

    let mut application_sources = content
        .applications()
        .into_iter()
        .filter(|application| {
            application.process_id() != current_process
                && visible_window_counts
                    .get(&application.process_id())
                    .copied()
                    .unwrap_or_default()
                    > 0
        })
        .map(|application| {
            let count = visible_window_counts
                .get(&application.process_id())
                .copied()
                .unwrap_or_default();
            let name = safe_source_label(&application.application_name(), "Application");
            LiveCaptureSource {
                id: format!("application:{}", application.process_id()),
                kind: LiveCaptureSourceKind::Application,
                title: name.clone(),
                detail: format!("{count} visible window{}", if count == 1 { "" } else { "s" }),
                application_name: Some(name),
                bundle_identifier: Some(application.bundle_identifier()),
                process_id: Some(application.process_id()),
                window_id: None,
                display_id: None,
            }
        })
        .collect::<Vec<_>>();

    let mut display_sources = content
        .displays()
        .into_iter()
        .enumerate()
        .map(|(index, display)| LiveCaptureSource {
            id: format!("display:{}", display.display_id()),
            kind: LiveCaptureSourceKind::Display,
            title: format!("Display {}", index + 1),
            detail: format!("{}×{}", display.width(), display.height()),
            application_name: None,
            bundle_identifier: None,
            process_id: None,
            window_id: None,
            display_id: Some(display.display_id()),
        })
        .collect::<Vec<_>>();

    application_sources.sort_by_key(|source| source.title.to_ascii_lowercase());
    window_sources.sort_by_key(|source| source.title.to_ascii_lowercase());
    display_sources.sort_by_key(|source| source.title.to_ascii_lowercase());
    LiveCaptureSources {
        applications: application_sources,
        windows: window_sources,
        displays: display_sources,
    }
}

fn intersection_area(
    first: screencapturekit::cg::CGRect,
    second: screencapturekit::cg::CGRect,
) -> f64 {
    let left = first.origin.x.max(second.origin.x);
    let top = first.origin.y.max(second.origin.y);
    let right = (first.origin.x + first.size.width).min(second.origin.x + second.size.width);
    let bottom = (first.origin.y + first.size.height).min(second.origin.y + second.size.height);
    (right - left).max(0.0) * (bottom - top).max(0.0)
}

fn capture_filter_for_selection(
    content: &SCShareableContent,
    selection: &LiveCaptureSourceSelection,
) -> Result<SCContentFilter, ApiError> {
    match selection.kind {
        LiveCaptureSourceKind::Window => {
            let window_id = selection
                .window_id
                .ok_or_else(|| capture_error("The selected window is invalid."))?;
            let windows = content.windows();
            let window = windows
                .iter()
                .find(|window| window.window_id() == window_id && window.is_on_screen())
                .ok_or_else(|| {
                    capture_error("That window is no longer available. Refresh the source list.")
                })?;
            Ok(SCContentFilter::create().with_window(window).build())
        }
        LiveCaptureSourceKind::Display => {
            let display_id = selection
                .display_id
                .ok_or_else(|| capture_error("The selected display is invalid."))?;
            let displays = content.displays();
            let display = displays
                .iter()
                .find(|display| display.display_id() == display_id)
                .ok_or_else(|| {
                    capture_error("That display is no longer available. Refresh the source list.")
                })?;
            Ok(SCContentFilter::create()
                .with_display(display)
                .with_excluding_windows(&[])
                .build())
        }
        LiveCaptureSourceKind::Application => {
            let process_id = selection
                .process_id
                .ok_or_else(|| capture_error("The selected application is invalid."))?;
            let applications = content.applications();
            let application = applications
                .iter()
                .find(|application| application.process_id() == process_id)
                .ok_or_else(|| {
                    capture_error("That application is no longer available. Refresh the source list.")
                })?;
            let app_windows = content
                .windows()
                .into_iter()
                .filter(|window| {
                    window
                        .owning_application()
                        .is_some_and(|owner| owner.process_id() == process_id)
                })
                .collect::<Vec<_>>();
            let displays = content.displays();
            let display = displays
                .iter()
                .max_by(|left, right| {
                    let left_area = app_windows
                        .iter()
                        .map(|window| intersection_area(left.frame(), window.frame()))
                        .sum::<f64>();
                    let right_area = app_windows
                        .iter()
                        .map(|window| intersection_area(right.frame(), window.frame()))
                        .sum::<f64>();
                    left_area.total_cmp(&right_area)
                })
                .ok_or_else(|| capture_error("No display is available for that application."))?;
            Ok(SCContentFilter::create()
                .with_display(display)
                .with_including_applications(&[application], &[])
                .build())
        }
    }
}

pub async fn start(
    app: tauri::AppHandle,
    state: &LiveState,
    options: LiveStartOptions,
    generation: u64,
) -> Result<(), ApiError> {
    let LiveStartOptions {
        api_key,
        languages,
        sync_mode,
        processing_mode,
        source,
    } = options;
    let start_lease = state.start_sequence.fetch_add(1, Ordering::Relaxed) + 1;
    abort_previous(state).await;
    let _start_guard = state.start_lock.lock().await;
    ensure_live_start_current(state, start_lease)?;
    state.cancelled.store(false, Ordering::Relaxed);

    let content = AsyncSCShareableContent::create()
        .with_exclude_desktop_windows(true)
        .with_on_screen_windows_only(true)
        .get()
        .await
        .map_err(|error| {
            capture_error(&format!(
                "NonoSub could not access the selected source. Check Screen & System Audio Recording permission: {error}"
            ))
        })?;
    ensure_live_start_current(state, start_lease)?;
    let filter = capture_filter_for_selection(&content, &source)?;

    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(1)
        .with_excludes_current_process_audio(true);
    let stream = AsyncSCStream::new(&filter, &config, 8, SCStreamOutputType::Audio);

    let (mut writer, mut reader) =
        connect_realtime(&api_key, &languages, &processing_mode, generation, 0).await?;
    ensure_live_start_current(state, start_lease)?;

    stream.start_capture().await.map_err(|error| {
        capture_error(&format!("System audio capture could not start: {error}"))
    })?;
    ensure_live_start_current(state, start_lease)?;
    let _ = record_event_for_generation(
        &app,
        generation,
        SessionEvent::PhaseChanged {
            phase: "buffering".into(),
        },
    );

    let cancelled = state.cancelled.clone();
    let task = tauri::async_runtime::spawn(async move {
        let original_only = processing_mode == CaptionProcessingMode::OriginalOnly;
        let mut pcm = Vec::<i16>::with_capacity(SEND_SAMPLES * 2);
        let mut resampler = Pcm24Resampler::default();
        let mut captured_input_samples_48k = 0_u64;
        let mut pcm_front_capture_sample = 0_u64;
        let mut transmission = TransmissionTimeline::default();
        transmission.begin_epoch(0);
        let mut coordinator = CaptionCoordinator::new(sync_mode);
        let mut transcription = TranscriptionCoordinator::default();
        let mut speech_committer = SpeechCommitter::default();
        let mut ready = false;
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let mut closing = false;
        let mut close_started = None;
        let mut closed = false;
        let mut reconnect = ReconnectAllowance::default();

        'capture: loop {
            if cancelled.load(Ordering::Relaxed) && !closing {
                closing = true;
                close_started = Some(Instant::now());
                let close_event = if original_only {
                    speech_committer
                        .finish()
                        .then(|| json!({ "type": "input_audio_buffer.commit" }))
                } else {
                    Some(json!({ "type": "session.close" }))
                };
                if let Some(close_event) = close_event {
                    let _ = send_realtime(&mut writer, close_event).await;
                }
            }
            if closed
                || close_started.is_some_and(|started| started.elapsed() > Duration::from_secs(3))
            {
                break;
            }

            tokio::select! {
                sample = stream.next(), if !closing => {
                    match sample {
                        Some(sample) => {
                            if let Some(list) = sample.audio_buffer_list() {
                                for buffer in list.iter() {
                                    captured_input_samples_48k = captured_input_samples_48k
                                        .saturating_add(resampler.append_f32_48k(buffer.data(), &mut pcm));
                                }
                            } else {
                                emit_recoverable(&app, generation, "capture_buffer", "A system-audio buffer could not be read.");
                            }
                            // The CoreMedia buffer is dropped before the socket await; it is not Send.
                            drop(sample);
                            {
                                while pcm.len() >= SEND_SAMPLES {
                                    let samples: Vec<i16> = pcm.drain(..SEND_SAMPLES).collect();
                                    let frame = CapturedPcm {
                                        capture_start_sample: pcm_front_capture_sample,
                                        capture_end_sample: pcm_front_capture_sample
                                            .saturating_add(samples.len() as u64),
                                        samples,
                                    };
                                    pcm_front_capture_sample = frame.capture_end_sample;
                                    let actions = if original_only {
                                        speech_committer.push(frame)
                                    } else {
                                        vec![TranscriptionAudioAction::Append(frame)]
                                    };
                                    for action in actions {
                                        let (event, frame) = match action {
                                            TranscriptionAudioAction::Append(frame) => {
                                                let mut bytes = Vec::with_capacity(frame.samples.len() * 2);
                                                for sample in &frame.samples { bytes.extend_from_slice(&sample.to_le_bytes()); }
                                                (json!({
                                                    "type": if original_only { "input_audio_buffer.append" } else { "session.input_audio_buffer.append" },
                                                    "audio": BASE64.encode(bytes)
                                                }), Some(frame))
                                            }
                                            TranscriptionAudioAction::Commit => (json!({ "type": "input_audio_buffer.commit" }), None),
                                        };
                                        let sent = send_realtime(&mut writer, event).await;
                                        if sent {
                                            if let Some(frame) = frame.as_ref() {
                                                transmission.record(frame);
                                            }
                                        }
                                        if !sent {
                                            let captured_samples_24k = captured_input_samples_48k / 2;
                                            let capture_ms = capture_clock_ms(captured_samples_24k);
                                            close_for_transmission_gap(original_only, &app, generation, &mut transcription, &mut coordinator, capture_ms);
                                            if reconnect.claim(closing) {
                                                let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                                pcm.clear();
                                                resampler = Pcm24Resampler::default();
                                                pcm_front_capture_sample = captured_samples_24k;
                                                if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode, generation, 1).await {
                                                    writer = next_writer;
                                                    reader = next_reader;
                                                    transmission.begin_epoch(captured_samples_24k);
                                                    speech_committer = SpeechCommitter::default();
                                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                                    continue 'capture;
                                                }
                                            }
                                            emit_recoverable(&app, generation, "live_disconnected", "Live captions disconnected and the automatic reconnect did not succeed.");
                                            closing = true;
                                            break;
                                        }
                                    }
                                    if closing {
                                        break;
                                    }
                                }
                            }
                        }
                        None => {
                            if !closing {
                                emit_recoverable(
                                    &app,
                                    generation,
                                    "live_source_ended",
                                    "The selected audio source ended or became unavailable.",
                                );
                            }
                            break;
                        }
                    }
                }
                message = reader.next() => {
                    let Some(message) = message else {
                        if closing {
                            closed = true;
                            continue;
                        }
                        if reconnect.claim(closing) {
                            let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                            let captured_samples_24k = captured_input_samples_48k / 2;
                            let capture_ms = capture_clock_ms(captured_samples_24k);
                            close_for_transmission_gap(original_only, &app, generation, &mut transcription, &mut coordinator, capture_ms);
                            pcm.clear();
                            resampler = Pcm24Resampler::default();
                            pcm_front_capture_sample = captured_samples_24k;
                            if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode, generation, 1).await {
                                writer = next_writer;
                                reader = next_reader;
                                transmission.begin_epoch(captured_samples_24k);
                                speech_committer = SpeechCommitter::default();
                                let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                continue 'capture;
                            }
                        }
                        emit_recoverable(&app, generation, "live_disconnected", "Live captions disconnected and the automatic reconnect did not succeed.");
                        break;
                    };
                    match message {
                        Ok(Message::Text(text)) => match parse_realtime_event(&text) {
                            RealtimeEvent::SourceDelta(delta) => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                let capture_ms = capture_clock_ms(captured_input_samples_48k / 2);
                                emit_events(&app, generation, coordinator.on_source(align_timed_text(delta, &transmission), capture_ms));
                            }
                            RealtimeEvent::TranslationDelta(delta) => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                let capture_ms = capture_clock_ms(captured_input_samples_48k / 2);
                                emit_events(&app, generation, coordinator.on_translation(align_timed_text(delta, &transmission), capture_ms));
                            }
                            RealtimeEvent::TranscriptionDelta { item_id, event_id, text } => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                emit_events(&app, generation, transcription.on_delta(item_id, event_id, text, capture_clock_ms(captured_input_samples_48k / 2)));
                            }
                            RealtimeEvent::TranscriptionDone { item_id, event_id, text } => {
                                emit_events(&app, generation, transcription.on_done(item_id, event_id, text, capture_clock_ms(captured_input_samples_48k / 2)));
                            }
                            RealtimeEvent::Closed if closing => closed = true,
                            RealtimeEvent::Closed => {
                                if reconnect.claim(closing) {
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                    let captured_samples_24k = captured_input_samples_48k / 2;
                                    let capture_ms = capture_clock_ms(captured_samples_24k);
                                    close_for_transmission_gap(original_only, &app, generation, &mut transcription, &mut coordinator, capture_ms);
                                    pcm.clear();
                                    resampler = Pcm24Resampler::default();
                                    pcm_front_capture_sample = captured_samples_24k;
                                    if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode, generation, 1).await {
                                        writer = next_writer;
                                        reader = next_reader;
                                        transmission.begin_epoch(captured_samples_24k);
                                        speech_committer = SpeechCommitter::default();
                                        let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                        continue 'capture;
                                    }
                                }
                                emit_recoverable(&app, generation, "live_disconnected", "Live captions closed unexpectedly and the automatic reconnect did not succeed.");
                                break 'capture;
                            }
                            RealtimeEvent::Error(error) => emit_recoverable(&app, generation, "realtime_error", &error.message),
                            RealtimeEvent::SessionCreated(_)
                            | RealtimeEvent::SessionUpdated(_)
                            | RealtimeEvent::Ignored => {}
                        },
                        Ok(Message::Close(_)) if closing => closed = true,
                        Ok(Message::Close(_)) => {
                            if reconnect.claim(closing) {
                                let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                let captured_samples_24k = captured_input_samples_48k / 2;
                                let capture_ms = capture_clock_ms(captured_samples_24k);
                                close_for_transmission_gap(original_only, &app, generation, &mut transcription, &mut coordinator, capture_ms);
                                pcm.clear();
                                resampler = Pcm24Resampler::default();
                                pcm_front_capture_sample = captured_samples_24k;
                                if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode, generation, 1).await {
                                    writer = next_writer;
                                    reader = next_reader;
                                    transmission.begin_epoch(captured_samples_24k);
                                    speech_committer = SpeechCommitter::default();
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                    continue 'capture;
                                }
                            }
                            emit_recoverable(&app, generation, "live_disconnected", "Live captions closed unexpectedly and the automatic reconnect did not succeed.");
                            break;
                        }
                        Ok(Message::Ping(payload)) => {
                            let _ = send_realtime_message(&mut writer, Message::Pong(payload)).await;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            if closing {
                                closed = true;
                                continue;
                            }
                            if reconnect.claim(closing) {
                                let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                let captured_samples_24k = captured_input_samples_48k / 2;
                                let capture_ms = capture_clock_ms(captured_samples_24k);
                                close_for_transmission_gap(original_only, &app, generation, &mut transcription, &mut coordinator, capture_ms);
                                pcm.clear();
                                resampler = Pcm24Resampler::default();
                                pcm_front_capture_sample = captured_samples_24k;
                                if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode, generation, 1).await {
                                    writer = next_writer;
                                    reader = next_reader;
                                    transmission.begin_epoch(captured_samples_24k);
                                    speech_committer = SpeechCommitter::default();
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                    continue 'capture;
                                }
                            }
                            emit_recoverable(&app, generation, "live_disconnected", &format!("Live caption connection ended: {error}"));
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    if !original_only {
                        emit_events(&app, generation, coordinator.tick(capture_clock_ms(captured_input_samples_48k / 2)));
                    }
                }
            }
        }

        if original_only {
            emit_events(
                &app,
                generation,
                transcription.finish(capture_clock_ms(captured_input_samples_48k / 2)),
            );
        } else {
            emit_events(
                &app,
                generation,
                coordinator.finish(capture_clock_ms(captured_input_samples_48k / 2)),
            );
        }
        let _ = stream.stop_capture().await;
        let _ = record_event_for_generation(&app, generation, SessionEvent::Complete);
    });
    *state
        .task
        .lock()
        .map_err(|_| capture_error("Live task state is unavailable."))? = Some(task);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct TransmissionSpan {
    transmitted_start: u64,
    transmitted_end: u64,
    capture_start: u64,
    capture_end: u64,
}

#[derive(Debug, Default)]
struct TransmissionTimeline {
    epoch_capture_start: u64,
    transmitted_samples: u64,
    spans: VecDeque<TransmissionSpan>,
}

impl TransmissionTimeline {
    fn begin_epoch(&mut self, capture_sample: u64) {
        self.epoch_capture_start = capture_sample;
        self.transmitted_samples = 0;
        self.spans.clear();
    }

    fn record(&mut self, frame: &CapturedPcm) {
        let length = frame.samples.len() as u64;
        let span = TransmissionSpan {
            transmitted_start: self.transmitted_samples,
            transmitted_end: self.transmitted_samples.saturating_add(length),
            capture_start: frame.capture_start_sample,
            capture_end: frame.capture_end_sample,
        };
        self.transmitted_samples = span.transmitted_end;
        self.spans.push_back(span);
        while self.spans.len() > MAX_RETAINED_LIVE_UNITS {
            self.spans.pop_front();
        }
    }

    fn align_elapsed_ms(&self, elapsed_ms: u64) -> u64 {
        let transmitted_sample = elapsed_ms.saturating_mul(24);
        let span = self
            .spans
            .iter()
            .find(|span| transmitted_sample < span.transmitted_end)
            .or_else(|| self.spans.back());
        let capture_sample = span.map_or(self.epoch_capture_start, |span| {
            let offset = transmitted_sample
                .saturating_sub(span.transmitted_start)
                .min(span.transmitted_end.saturating_sub(span.transmitted_start));
            span.capture_start
                .saturating_add(offset)
                .min(span.capture_end)
        });
        samples_24k_to_ms(capture_sample.saturating_sub(self.epoch_capture_start))
    }
}

fn samples_24k_to_ms(samples: u64) -> u64 {
    samples.saturating_mul(1_000) / 24_000
}

fn align_timed_text(mut delta: TimedText, timeline: &TransmissionTimeline) -> TimedText {
    delta.elapsed_ms = delta
        .elapsed_ms
        .map(|elapsed| timeline.align_elapsed_ms(elapsed));
    delta
}

fn capture_clock_ms(captured_samples_24k: u64) -> u64 {
    samples_24k_to_ms(captured_samples_24k)
}

async fn send_realtime(writer: &mut RealtimeWriter, event: Value) -> bool {
    tokio::time::timeout(
        Duration::from_secs(5),
        writer.send(Message::Text(event.to_string().into())),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

async fn send_realtime_message(writer: &mut RealtimeWriter, message: Message) -> bool {
    tokio::time::timeout(Duration::from_secs(5), writer.send(message))
        .await
        .is_ok_and(|result| result.is_ok())
}

fn close_for_transmission_gap(
    original_only: bool,
    app: &tauri::AppHandle,
    generation: u64,
    transcription: &mut TranscriptionCoordinator,
    coordinator: &mut CaptionCoordinator,
    capture_clock_ms: u64,
) {
    if original_only {
        emit_events(app, generation, transcription.finish(capture_clock_ms));
    } else {
        emit_events(app, generation, coordinator.begin_epoch(capture_clock_ms));
    }
}

pub fn stop(state: &LiveState) {
    state.start_sequence.fetch_add(1, Ordering::Relaxed);
    state.cancelled.store(true, Ordering::Relaxed);
}

fn ensure_live_start_current(state: &LiveState, lease: u64) -> Result<(), ApiError> {
    if state.start_sequence.load(Ordering::Relaxed) == lease {
        Ok(())
    } else {
        Err(ApiError {
            kind: ApiErrorKind::Cancelled,
            message: "The previous Live Captions start was replaced.".into(),
            retryable: false,
        })
    }
}

async fn abort_previous(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
    let previous = state.task.lock().ok().and_then(|mut task| task.take());
    if let Some(task) = previous {
        task.abort();
        let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
    }
}

async fn connect_realtime(
    api_key: &str,
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
    generation: u64,
    attempt: u8,
) -> Result<(RealtimeWriter, RealtimeReader), ApiError> {
    let url = if processing_mode == &CaptionProcessingMode::OriginalOnly {
        REALTIME_TRANSCRIPTION_URL
    } else {
        REALTIME_TRANSLATION_URL
    };
    let mut request = url.into_client_request().map_err(|error| {
        network_error(&format!("Could not prepare realtime connection: {error}"))
    })?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {api_key}")
            .parse()
            .map_err(|_| network_error("Could not authorize the realtime connection."))?,
    );
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(15), connect_async(request))
        .await
        .map_err(|_| network_error("Realtime connection timed out."))?
        .map_err(realtime_connect_error)?;
    wait_for_session_created(&mut socket).await?;

    let configuration_event_id = format!("nonosub-config-{generation}-{attempt}");
    let update =
        realtime_session_update(languages, processing_mode, configuration_event_id.as_str());
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(update.to_string().into())),
    )
    .await
    .map_err(|_| network_error("Realtime caption configuration timed out."))?
    .map_err(|error| network_error(&format!("Could not configure realtime captions: {error}")))?;
    wait_for_session_updated(
        &mut socket,
        languages,
        processing_mode,
        configuration_event_id.as_str(),
    )
    .await?;

    let (writer, reader) = socket.split();
    Ok((writer, reader))
}

async fn wait_for_session_created(socket: &mut RealtimeSocket) -> Result<(), ApiError> {
    tokio::time::timeout(REALTIME_CONFIGURATION_TIMEOUT, async {
        loop {
            let message = socket
                .next()
                .await
                .ok_or_else(|| network_error("Realtime captions closed before session.created."))?
                .map_err(|error| network_error(&format!("Realtime captions failed before session.created: {error}")))?;
            match message {
                Message::Ping(payload) => {
                    socket.send(Message::Pong(payload)).await.map_err(|error| {
                        network_error(&format!("Realtime captions could not answer a ping: {error}"))
                    })?;
                }
                Message::Text(text) => return validate_session_created_event(parse_realtime_event(&text)),
                Message::Close(_) => return Err(network_error("Realtime captions closed before session.created.")),
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| configuration_error("Realtime captions timed out before session.created."))?
}

fn validate_session_created_event(event: RealtimeEvent) -> Result<(), ApiError> {
    match event {
        RealtimeEvent::SessionCreated(session) if session.is_object() => Ok(()),
        RealtimeEvent::Error(error) => Err(server_configuration_error(&error)),
        _ => Err(configuration_error(
            "Realtime captions did not send session.created as the first event.",
        )),
    }
}

async fn wait_for_session_updated(
    socket: &mut RealtimeSocket,
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
    configuration_event_id: &str,
) -> Result<(), ApiError> {
    tokio::time::timeout(REALTIME_CONFIGURATION_TIMEOUT, async {
        loop {
            let message = socket
                .next()
                .await
                .ok_or_else(|| network_error("Realtime captions closed before session.updated."))?
                .map_err(|error| {
                    network_error(&format!(
                        "Realtime captions failed before session.updated: {error}"
                    ))
                })?;
            let text = match message {
                Message::Ping(payload) => {
                    socket.send(Message::Pong(payload)).await.map_err(|error| {
                        network_error(&format!("Realtime captions could not answer a ping: {error}"))
                    })?;
                    continue;
                }
                Message::Text(text) => text,
                Message::Close(_) => return Err(network_error("Realtime captions closed before session.updated.")),
                _ => continue,
            };
            if validate_session_update_event(
                parse_realtime_event(&text),
                languages,
                processing_mode,
                configuration_event_id,
            )? {
                return Ok(());
            }
        }
    })
    .await
    .map_err(|_| configuration_error("Realtime captions timed out before session.updated."))?
}

fn validate_session_update_event(
    event: RealtimeEvent,
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
    configuration_event_id: &str,
) -> Result<bool, ApiError> {
    match event {
        RealtimeEvent::SessionUpdated(session) => {
            validate_session_updated(&session, languages, processing_mode)?;
            Ok(true)
        }
        RealtimeEvent::Error(error)
            if error.client_event_id.as_deref() == Some(configuration_event_id) =>
        {
            Err(server_configuration_error(&error))
        }
        RealtimeEvent::Error(error) => Err(server_configuration_error(&error)),
        RealtimeEvent::Closed => Err(network_error(
            "Realtime captions closed before session.updated.",
        )),
        RealtimeEvent::SessionCreated(_) => Err(configuration_error(
            "Realtime captions repeated session.created before acknowledging configuration.",
        )),
        _ => Ok(false),
    }
}

fn validate_session_updated(
    session: &Value,
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
) -> Result<(), ApiError> {
    let expect = |pointer: &str, expected: &str| {
        (session.pointer(pointer).and_then(Value::as_str) == Some(expected)).then_some(())
    };

    if processing_mode == &CaptionProcessingMode::OriginalOnly {
        expect("/type", "transcription")
            .ok_or_else(|| configuration_mismatch("transcription session type"))?;
        expect("/audio/input/format/type", "audio/pcm")
            .ok_or_else(|| configuration_mismatch("24 kHz PCM input format"))?;
        if session
            .pointer("/audio/input/format/rate")
            .and_then(Value::as_u64)
            != Some(24_000)
        {
            return Err(configuration_mismatch("24 kHz PCM input rate"));
        }
        expect("/audio/input/transcription/model", "gpt-realtime-whisper")
            .ok_or_else(|| configuration_mismatch("realtime transcription model"))?;
        expect("/audio/input/transcription/delay", "low")
            .ok_or_else(|| configuration_mismatch("low transcription delay"))?;
        if languages.source != "auto" {
            expect(
                "/audio/input/transcription/language",
                languages.source.as_str(),
            )
            .ok_or_else(|| configuration_mismatch("source-language hint"))?;
        }
    } else {
        expect("/type", "translation")
            .ok_or_else(|| configuration_mismatch("translation session type"))?;
        expect("/audio/input/transcription/model", "gpt-realtime-whisper")
            .ok_or_else(|| configuration_mismatch("source transcription model"))?;
        expect("/audio/output/language", languages.target.as_str())
            .ok_or_else(|| configuration_mismatch("target language"))?;
    }
    Ok(())
}

fn realtime_session_update(
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
    configuration_event_id: &str,
) -> Value {
    if processing_mode == &CaptionProcessingMode::OriginalOnly {
        let mut transcription = json!({
            "model": "gpt-realtime-whisper",
            "delay": "low"
        });
        if languages.source != "auto" {
            transcription["language"] = Value::String(languages.source.clone());
        }
        json!({
            "event_id": configuration_event_id,
            "type": "session.update",
            "session": {
                "type": "transcription",
                "audio": {
                    "input": {
                        "format": { "type": "audio/pcm", "rate": 24_000 },
                        "transcription": transcription,
                        "turn_detection": null
                    }
                }
            }
        })
    } else {
        json!({
            "event_id": configuration_event_id,
            "type": "session.update",
            "session": {
                "audio": {
                    "input": { "transcription": { "model": "gpt-realtime-whisper" } },
                    "output": { "language": languages.target }
                }
            }
        })
    }
}

fn realtime_connect_error(error: WebSocketError) -> ApiError {
    if let WebSocketError::Http(response) = &error {
        return match response.status().as_u16() {
            401 => ApiError {
                kind: ApiErrorKind::Authentication,
                message: "OpenAI rejected the API key for Live Captions.".into(),
                retryable: false,
            },
            403 | 404 => ApiError {
                kind: ApiErrorKind::ModelUnavailable,
                message: "The selected OpenAI realtime model is unavailable for this API key."
                    .into(),
                retryable: false,
            },
            429 => ApiError {
                kind: ApiErrorKind::RateLimited,
                message: "OpenAI rate-limited the Live Captions connection.".into(),
                retryable: true,
            },
            _ => network_error(&format!("Could not connect realtime captions: {error}")),
        };
    }
    network_error(&format!("Could not connect realtime captions: {error}"))
}

fn configuration_error(message: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::MalformedResponse,
        message: message.into(),
        retryable: false,
    }
}

fn configuration_mismatch(field: &str) -> ApiError {
    configuration_error(&format!(
        "OpenAI did not acknowledge the requested realtime {field}."
    ))
}

fn server_configuration_error(error: &RealtimeServerError) -> ApiError {
    let detail = error
        .code
        .as_deref()
        .or(error.kind.as_deref())
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();
    ApiError {
        kind: ApiErrorKind::Service,
        message: format!(
            "OpenAI rejected the Live Captions configuration{detail}: {}",
            error.message
        ),
        retryable: false,
    }
}

#[derive(Debug, Default)]
struct Pcm24Resampler {
    pending: Option<f32>,
}

impl Pcm24Resampler {
    fn append_f32_48k(&mut self, bytes: &[u8], output: &mut Vec<i16>) -> u64 {
        let mut input_samples = 0_u64;
        for frame in bytes.chunks_exact(4) {
            input_samples = input_samples.saturating_add(1);
            let sample = f32::from_ne_bytes(frame.try_into().expect("four-byte float"));
            if let Some(first) = self.pending.take() {
                let averaged = ((first + sample) * 0.5).clamp(-1.0, 1.0);
                output.push((averaged * i16::MAX as f32).round() as i16);
            } else {
                self.pending = Some(sample);
            }
        }
        input_samples
    }
}

fn parse_realtime_event(text: &str) -> RealtimeEvent {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return RealtimeEvent::Ignored;
    };
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let timed_delta = || {
        let event_id = value.get("event_id").and_then(Value::as_str)?;
        let delta = value.get("delta").and_then(Value::as_str)?;
        if event_id.is_empty() || delta.is_empty() {
            return None;
        }
        Some(TimedText {
            event_id: Some(event_id.to_owned()),
            text: delta.to_owned(),
            elapsed_ms: value.get("elapsed_ms").and_then(Value::as_u64),
        })
    };
    match event_type {
        "session.created" => {
            RealtimeEvent::SessionCreated(value.get("session").cloned().unwrap_or(Value::Null))
        }
        "session.updated" => {
            RealtimeEvent::SessionUpdated(value.get("session").cloned().unwrap_or(Value::Null))
        }
        "session.input_transcript.delta" => timed_delta()
            .map(RealtimeEvent::SourceDelta)
            .unwrap_or(RealtimeEvent::Ignored),
        "session.output_transcript.delta" => timed_delta()
            .map(RealtimeEvent::TranslationDelta)
            .unwrap_or(RealtimeEvent::Ignored),
        "conversation.item.input_audio_transcription.delta" => RealtimeEvent::TranscriptionDelta {
            item_id: value
                .get("item_id")
                .and_then(Value::as_str)
                .unwrap_or("transcription-current")
                .to_owned(),
            event_id: value
                .get("event_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            text: value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        },
        "conversation.item.input_audio_transcription.completed" => {
            RealtimeEvent::TranscriptionDone {
                item_id: value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or("transcription-current")
                    .to_owned(),
                event_id: value
                    .get("event_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                text: value
                    .get("transcript")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            }
        }
        "session.closed" => RealtimeEvent::Closed,
        "error" => RealtimeEvent::Error(RealtimeServerError {
            message: value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Realtime captions reported an error.")
                .to_owned(),
            kind: value
                .pointer("/error/type")
                .and_then(Value::as_str)
                .map(str::to_owned),
            code: value
                .pointer("/error/code")
                .and_then(Value::as_str)
                .map(str::to_owned),
            client_event_id: value
                .pointer("/error/event_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }),
        _ => RealtimeEvent::Ignored,
    }
}

fn emit_recoverable(app: &tauri::AppHandle, generation: u64, code: &str, message: &str) {
    let _ = record_event_for_generation(
        app,
        generation,
        SessionEvent::RecoverableError {
            error: RecoverableError {
                code: code.into(),
                message: message.into(),
                segment_id: None,
            },
        },
    );
}

fn capture_error(message: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::Service,
        message: message.into(),
        retryable: true,
    }
}

fn network_error(message: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::Network,
        message: message.into(),
        retryable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_48k_float_audio_to_continuous_24k_pcm16() {
        let input: Vec<u8> = [0.5_f32, 0.5, -2.0, -2.0]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        let mut output = Vec::new();
        let mut resampler = Pcm24Resampler::default();
        assert_eq!(resampler.append_f32_48k(&input, &mut output), 4);
        assert_eq!(output.len(), 2);
        assert!((output[0] - 16_384).abs() <= 1);
        assert_eq!(output[1], -32_767);
    }

    #[test]
    fn capture_clock_and_transmission_timeline_preserve_gaps() {
        assert_eq!(capture_clock_ms(4_800), 200);
        let mut timeline = TransmissionTimeline::default();
        timeline.begin_epoch(0);
        timeline.record(&CapturedPcm {
            samples: vec![0; 2_400],
            capture_start_sample: 0,
            capture_end_sample: 2_400,
        });
        timeline.record(&CapturedPcm {
            samples: vec![0; 2_400],
            capture_start_sample: 4_800,
            capture_end_sample: 7_200,
        });
        assert_eq!(timeline.align_elapsed_ms(50), 50);
        assert_eq!(timeline.align_elapsed_ms(150), 250);
    }

    #[test]
    fn odd_input_sample_is_preserved_across_resampler_calls() {
        let first: Vec<u8> = [0.25_f32].into_iter().flat_map(f32::to_ne_bytes).collect();
        let second: Vec<u8> = [0.75_f32].into_iter().flat_map(f32::to_ne_bytes).collect();
        let mut resampler = Pcm24Resampler::default();
        let mut output = Vec::new();
        resampler.append_f32_48k(&first, &mut output);
        assert!(output.is_empty());
        resampler.append_f32_48k(&second, &mut output);
        assert_eq!(output, vec![16_384]);
    }

    #[test]
    fn reconnect_allowance_is_single_use_and_ignores_graceful_close() {
        let mut graceful = ReconnectAllowance::default();
        assert!(!graceful.claim(true));
        assert!(graceful.claim(false));
        assert!(!graceful.claim(false));

        let mut unexpected = ReconnectAllowance::default();
        assert!(unexpected.claim(false));
        assert!(!unexpected.claim(false));
    }

    #[test]
    fn newer_live_start_invalidates_an_older_picker_lease() {
        let state = LiveState::default();
        let first = state.start_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        assert!(ensure_live_start_current(&state, first).is_ok());
        let second = state.start_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        assert_eq!(
            ensure_live_start_current(&state, first).unwrap_err().kind,
            ApiErrorKind::Cancelled
        );
        assert!(ensure_live_start_current(&state, second).is_ok());
    }

    #[test]
    fn parses_realtime_translation_deltas_and_errors() {
        assert_eq!(
            parse_realtime_event(
                r#"{"event_id":"source-1","type":"session.input_transcript.delta","delta":"今日は","elapsed_ms":1200}"#
            ),
            RealtimeEvent::SourceDelta(TimedText {
                event_id: Some("source-1".into()),
                text: "今日は".into(),
                elapsed_ms: Some(1_200)
            })
        );
        assert_eq!(
            parse_realtime_event(
                r#"{"event_id":"target-1","type":"session.output_transcript.delta","delta":"Today","elapsed_ms":1200}"#
            ),
            RealtimeEvent::TranslationDelta(TimedText {
                event_id: Some("target-1".into()),
                text: "Today".into(),
                elapsed_ms: Some(1_200)
            })
        );
        assert_eq!(
            parse_realtime_event(r#"{"type":"error","error":{"message":"bad audio"}}"#),
            RealtimeEvent::Error(RealtimeServerError {
                message: "bad audio".into(),
                kind: None,
                code: None,
                client_event_id: None,
            })
        );
        assert_eq!(
            parse_realtime_event(
                r#"{"type":"session.output_transcript.delta","transcript":"cumulative text","elapsed_ms":1200}"#
            ),
            RealtimeEvent::Ignored
        );
        assert_eq!(
            parse_realtime_event(
                r#"{"event_id":"target-2","type":"session.output_transcript.delta","text":"not a delta"}"#
            ),
            RealtimeEvent::Ignored
        );
        assert_eq!(
            parse_realtime_event(
                r#"{"event_id":"target-done","type":"session.output_transcript.done","transcript":"done"}"#
            ),
            RealtimeEvent::Ignored
        );
    }

    fn timed(id: &str, text: &str, elapsed_ms: Option<u64>) -> TimedText {
        TimedText {
            event_id: Some(id.into()),
            text: text.into(),
            elapsed_ms,
        }
    }

    #[test]
    fn keeps_append_only_deltas_with_shared_alignment_times_and_samples_lag_once() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "何", Some(200)), 1_000);
        coordinator.on_source(timed("s2", "ですか？", Some(200)), 1_100);
        coordinator.on_source(timed("s2", "duplicate", Some(200)), 1_100);
        coordinator.on_translation(timed("t1", "What", Some(200)), 1_200);
        coordinator.on_translation(timed("t2", " is it?", Some(200)), 1_300);
        coordinator.on_translation(timed("t2", "duplicate", Some(200)), 1_300);
        assert_eq!(coordinator.drafts[0].source, "何ですか？");
        assert_eq!(coordinator.drafts[0].translation, "What is it?");
        assert_eq!(coordinator.lag.samples.len(), 1);
        assert_eq!(coordinator.lag.samples[0].lag_ms, 1_100);
    }

    #[test]
    fn grows_delay_immediately_and_reduces_it_slowly() {
        let mut estimator = LagEstimator::default();
        estimator.observe(0, 5_000, 1_000, 1_000);
        assert_eq!(estimator.target_delay_ms, 4_600);
        estimator.observe(0, 40_000, 39_000, 39_000);
        assert_eq!(estimator.target_delay_ms, 4_400);
    }

    #[test]
    fn delay_is_clamped_and_display_cursor_never_rewinds() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.tick(17_500);
        assert_eq!(coordinator.display_cursor_ms, 15_000);
        coordinator.on_translation(timed("slow", "Slow", Some(0)), 20_000);
        assert_eq!(coordinator.lag.target_delay_ms, 6_000);
        coordinator.tick(6_000);
        assert_eq!(coordinator.display_cursor_ms, 15_000);
    }

    #[test]
    fn coordinated_mode_waits_for_translation_and_fast_source_does_not() {
        let source = timed("s1", "今日はちょっと…。", Some(200));
        let mut coordinated = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinated.on_source(source.clone(), 1_000);
        coordinated.tick(2_300);
        assert_eq!(coordinated.visible_segment_id, None);
        coordinated.on_translation(timed("t1", "Today is difficult.", Some(200)), 2_400);
        coordinated.tick(3_700);
        assert_eq!(coordinated.visible_segment_id.as_deref(), Some("live-1"));

        let mut fast = CaptionCoordinator::new(LiveSyncMode::FastSource);
        fast.on_source(source, 1_000);
        assert_eq!(fast.visible_segment_id.as_deref(), Some("live-1"));
    }

    #[test]
    fn coordinated_mode_holds_previous_caption_while_replacement_is_preparing() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "最初です。", Some(200)), 1_000);
        coordinator.tick(2_300);
        coordinator.on_translation(timed("t1", "This is first.", Some(200)), 2_400);
        coordinator.tick(3_700);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-1"));

        coordinator.on_source(timed("s2", "次です。", Some(5_000)), 5_000);
        coordinator.tick(6_300);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-1"));

        coordinator.on_translation(timed("t2", "This is next.", Some(5_000)), 6_400);
        coordinator.tick(7_800);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-2"));
    }

    #[test]
    fn coordinated_mode_falls_back_to_source_after_six_seconds() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "長い話です。", Some(200)), 1_000);
        coordinator.tick(2_300);
        coordinator.tick(8_300);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-1"));
        assert_eq!(
            coordinator
                .last_emitted_sync
                .as_ref()
                .map(|sync| &sync.status),
            Some(&LiveSyncStatus::Degraded)
        );
        coordinator.on_translation(timed("t1", "This is a long story.", Some(200)), 8_500);
        coordinator.tick(9_800);
        assert_eq!(coordinator.drafts.len(), 1);
        assert_eq!(coordinator.drafts[0].id, "live-1");
        assert_eq!(coordinator.drafts[0].translation, "This is a long story.");

        coordinator.on_translation(timed("t2", "This must be separate.", Some(400)), 10_000);
        coordinator.tick(11_300);
        assert_eq!(coordinator.drafts[0].translation, "This is a long story.");
        assert_eq!(coordinator.translation_clauses.len(), 2);
        assert_eq!(coordinator.translation_clauses[1].unit_index, Some(1));
        assert_eq!(coordinator.drafts[1].translation, "This must be separate.");

        coordinator.on_source(timed("s2", "次の話です。", Some(5_000)), 12_000);
        coordinator.tick(12_400);
        coordinator.on_translation(timed("t3", "This is the next story.", Some(5_000)), 12_500);
        coordinator.tick(12_900);
        assert_eq!(coordinator.drafts[2].source, "次の話です。");
        assert_eq!(coordinator.drafts[2].translation, "This is the next story.");
    }

    #[test]
    fn target_first_output_pairs_with_the_next_source_clause() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_translation(timed("t1", "Target first.", Some(200)), 1_000);
        assert!(coordinator.drafts.is_empty());
        coordinator.on_source(timed("s1", "先に翻訳。", Some(200)), 1_100);
        assert_eq!(coordinator.drafts.len(), 1);
        assert_eq!(coordinator.drafts[0].translation, "Target first.");
    }

    #[test]
    fn translation_clauses_never_reopen_when_source_stalls() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "話し続けています。", Some(0)), 1_000);
        coordinator.on_translation(timed("t1", "First target.", Some(0)), 1_100);
        coordinator.tick(1_500);
        coordinator.on_translation(timed("t2", "Second target.", Some(400)), 1_600);
        coordinator.tick(2_000);
        coordinator.on_translation(timed("t3", "Third target.", Some(800)), 2_100);
        coordinator.tick(2_500);

        assert_eq!(coordinator.translation_clauses.len(), 3);
        assert_eq!(coordinator.drafts[0].translation, "First target.");
        assert!(coordinator
            .translation_clauses
            .iter()
            .all(|clause| clause.closed));
        assert_eq!(
            coordinator
                .translation_clauses
                .iter()
                .map(|clause| clause.text.as_str())
                .collect::<Vec<_>>(),
            vec!["First target.", "Second target.", "Third target."]
        );
    }

    #[test]
    fn hard_grapheme_guard_bounds_continuous_target_output_without_text_loss() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "source", Some(0)), 1_000);
        let family = "👨‍👩‍👧‍👦";
        let continuous = family.repeat(441);
        let events = coordinator.on_translation(timed("t1", &continuous, Some(0)), 1_100);

        assert_eq!(coordinator.translation_clauses.len(), 3);
        assert!(coordinator
            .translation_clauses
            .iter()
            .all(|clause| { clause.text.graphemes(true).count() <= MAX_CLAUSE_GRAPHEMES }));
        assert_eq!(
            coordinator
                .translation_clauses
                .iter()
                .map(|clause| clause.text.as_str())
                .collect::<String>(),
            continuous
        );
        assert_eq!(coordinator.drafts.len(), 3);
        let emitted_ids: HashSet<String> = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::CaptionUpserted { segment } if segment.translation_text.is_some() => {
                    Some(segment.id.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(emitted_ids.len(), 3);
        assert_eq!(
            coordinator
                .drafts
                .iter()
                .map(|draft| draft.translation.as_str())
                .collect::<String>(),
            continuous
        );

        coordinator.tick(2_300);
        coordinator.on_source(timed("s2", "next source", Some(5_000)), 6_000);
        coordinator.on_translation(timed("t2", "next target", Some(5_000)), 6_100);
        assert_eq!(coordinator.drafts[3].source, "next source");
        assert_eq!(coordinator.drafts[3].translation, "next target");
    }

    #[test]
    fn hard_source_splits_do_not_consume_future_translation_slots() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        let long_source = "語".repeat(441);
        coordinator.on_source(timed("s1", &long_source, Some(0)), 1_000);
        coordinator.on_translation(timed("t1", "first translation", Some(0)), 1_100);
        coordinator.tick(2_300);
        coordinator.on_source(timed("s2", "次の発言", Some(5_000)), 6_000);
        coordinator.on_translation(timed("t2", "next translation", Some(5_000)), 6_100);

        assert_eq!(coordinator.drafts.len(), 4);
        assert_eq!(coordinator.drafts[0].translation, "first translation");
        assert!(coordinator.drafts[1].translation.is_empty());
        assert!(coordinator.drafts[2].translation.is_empty());
        assert_eq!(coordinator.drafts[3].source, "次の発言");
        assert_eq!(coordinator.drafts[3].translation, "next translation");
    }

    #[test]
    fn elapsed_resets_do_not_reassign_finalized_history() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "一。", Some(100)), 1_000);
        coordinator.tick(1_400);
        coordinator.on_translation(timed("t1", "One.", Some(100)), 1_500);
        coordinator.tick(1_900);
        coordinator.on_source(timed("s2", "二。", Some(200)), 2_000);
        coordinator.tick(2_400);
        coordinator.on_translation(timed("t2", "Two.", Some(50)), 2_500);
        coordinator.tick(2_900);

        assert_eq!(coordinator.drafts[0].translation, "One.");
        assert_eq!(coordinator.drafts[1].translation, "Two.");
    }

    #[test]
    fn reconnect_closes_tracks_and_starts_a_new_alignment_epoch() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("old-s", "before", Some(100)), 1_000);
        coordinator.on_translation(timed("old-t", "before target", Some(100)), 1_100);
        coordinator.begin_epoch(12_000);
        assert!(coordinator.source_clauses[0].closed);
        assert!(coordinator.translation_clauses[0].closed);
        coordinator.on_source(timed("new-s", "再接続", Some(400)), 12_500);
        coordinator.on_translation(timed("new-t", "reconnected", Some(400)), 12_600);
        assert_eq!(coordinator.drafts.len(), 2);
        assert_eq!(coordinator.drafts[1].start_ms, 12_400);
        assert_eq!(coordinator.drafts[0].translation, "before target");
        assert_eq!(coordinator.drafts[1].translation, "reconnected");
    }

    #[test]
    fn missing_alignment_is_kept_out_of_lag_estimation() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "source", None), 3_900);
        coordinator.on_translation(timed("t1", "Untimed", None), 4_000);
        assert_eq!(coordinator.lag.observed_lag_ms, 0);
        assert!(coordinator.lag.samples.is_empty());
        assert_eq!(coordinator.drafts[0].translation, "Untimed");
    }

    #[test]
    fn aligned_audio_span_forces_a_new_source_clause() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(timed("s1", "First clause", Some(0)), 1_000);
        coordinator.on_source(timed("s2", "Second clause", Some(8_200)), 9_200);
        assert_eq!(coordinator.drafts.len(), 2);
        assert!(coordinator.drafts[0].source_closed);
        assert_eq!(coordinator.drafts[1].source, "Second clause");
    }

    #[test]
    fn punctuation_idle_and_capture_age_close_source_clauses() {
        let mut punctuation = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        punctuation.on_source(timed("p1", "Finished.", Some(0)), 1_000);
        punctuation.tick(1_349);
        assert!(!punctuation.drafts[0].source_closed);
        punctuation.tick(1_350);
        assert!(punctuation.drafts[0].source_closed);

        let mut idle = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        idle.on_source(timed("i1", "still open", Some(0)), 1_000);
        idle.tick(2_199);
        assert!(!idle.drafts[0].source_closed);
        idle.tick(2_200);
        assert!(idle.drafts[0].source_closed);

        let mut aged = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        aged.on_source(timed("a1", "age bound", Some(0)), 1_000);
        aged.source_clauses[0].last_at_ms = 10_999;
        aged.tick(10_999);
        assert!(!aged.drafts[0].source_closed);
        aged.source_clauses[0].last_at_ms = 11_000;
        aged.tick(11_000);
        assert!(aged.drafts[0].source_closed);
    }

    #[test]
    fn incoming_deltas_honor_quiet_boundaries_without_waiting_for_tick() {
        let mut source = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        source.on_source(timed("s1", "Finished.", Some(0)), 1_000);
        source.on_source(timed("s2", "Next sentence", Some(400)), 1_400);
        assert_eq!(source.drafts.len(), 2);
        assert_eq!(source.drafts[0].source, "Finished.");
        assert_eq!(source.drafts[1].source, "Next sentence");

        let mut translation = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        translation.on_source(timed("source", "Source one. Source two.", Some(0)), 900);
        translation.on_translation(timed("t1", "Finished.", Some(0)), 1_000);
        translation.on_translation(timed("t2", "Next sentence", Some(400)), 1_400);
        assert_eq!(translation.translation_clauses.len(), 2);
        assert_eq!(translation.translation_clauses[0].text, "Finished.");
        assert_eq!(translation.translation_clauses[1].text, "Next sentence");

        let mut idle = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        idle.on_source(timed("i1", "first", Some(0)), 1_000);
        idle.on_source(timed("i2", "second", Some(1_200)), 2_200);
        assert_eq!(idle.drafts.len(), 2);
    }

    #[test]
    fn visible_caption_holds_until_replacement_and_clears_on_stop() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::FastSource);
        coordinator.on_source(timed("s1", "brief caption", Some(0)), 1_000);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-1"));
        coordinator.tick(20_000);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-1"));
        coordinator.on_source(timed("s2", "replacement caption", Some(20_000)), 21_000);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-2"));
        coordinator.finish(21_100);
        assert_eq!(coordinator.visible_segment_id, None);
        assert_eq!(
            coordinator
                .last_emitted_sync
                .as_ref()
                .and_then(|sync| sync.visible_segment_id.as_deref()),
            None
        );

        let mut stopped = CaptionCoordinator::new(LiveSyncMode::FastSource);
        stopped.on_source(timed("stop", "clear on stop", Some(0)), 1_000);
        assert!(stopped.visible_segment_id.is_some());
        stopped.finish(1_100);
        assert_eq!(stopped.visible_segment_id, None);
    }

    #[test]
    fn requests_source_transcripts_for_bilingual_live_captions() {
        let languages = LanguageSettings {
            source: "en".into(),
            target: "ja".into(),
            explanation: "ja".into(),
        };
        let event = realtime_session_update(
            &languages,
            &CaptionProcessingMode::Translated,
            "config-translated",
        );
        assert_eq!(event["event_id"], "config-translated");
        assert_eq!(
            event["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-realtime-whisper"
        );
        assert_eq!(event["session"]["audio"]["output"]["language"], "ja");
        assert!(event["session"]["audio"]["input"]["transcription"]
            .get("language")
            .is_none());
    }

    #[test]
    fn configures_original_only_as_a_low_delay_transcription_session() {
        let languages = LanguageSettings {
            source: "ja".into(),
            target: "en".into(),
            explanation: "en".into(),
        };
        let event = realtime_session_update(
            &languages,
            &CaptionProcessingMode::OriginalOnly,
            "config-original",
        );
        assert_eq!(event["event_id"], "config-original");
        assert_eq!(event["session"]["type"], "transcription");
        assert_eq!(
            event["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-realtime-whisper"
        );
        assert_eq!(
            event["session"]["audio"]["input"]["transcription"]["delay"],
            "low"
        );
        assert_eq!(
            event["session"]["audio"]["input"]["transcription"]["language"],
            "ja"
        );
        assert!(event["session"]["audio"].get("output").is_none());
    }

    #[test]
    fn auto_source_omits_the_transcription_hint() {
        let languages = LanguageSettings {
            source: "auto".into(),
            target: "en".into(),
            explanation: "en".into(),
        };
        let event = realtime_session_update(
            &languages,
            &CaptionProcessingMode::OriginalOnly,
            "config-auto",
        );
        assert!(event["session"]["audio"]["input"]["transcription"]
            .get("language")
            .is_none());
    }

    #[test]
    fn parses_lifecycle_events_and_correlates_configuration_errors() {
        assert!(matches!(
            parse_realtime_event(
                r#"{"type":"session.created","event_id":"server-1","session":{"type":"translation"}}"#
            ),
            RealtimeEvent::SessionCreated(session) if session["type"] == "translation"
        ));
        assert!(matches!(
            parse_realtime_event(
                r#"{"type":"session.updated","event_id":"server-2","session":{"type":"translation"}}"#
            ),
            RealtimeEvent::SessionUpdated(session) if session["type"] == "translation"
        ));
        assert!(matches!(
            parse_realtime_event(
                r#"{"type":"error","event_id":"server-3","error":{"type":"invalid_request_error","code":"invalid_value","message":"bad target","event_id":"nonosub-config-7-0"}}"#
            ),
            RealtimeEvent::Error(RealtimeServerError {
                client_event_id: Some(event_id),
                code: Some(code),
                ..
            }) if event_id == "nonosub-config-7-0" && code == "invalid_value"
        ));
    }

    #[test]
    fn rejects_out_of_order_or_repeated_configuration_lifecycle_events() {
        let languages = LanguageSettings {
            source: "auto".into(),
            target: "en".into(),
            explanation: "en".into(),
        };
        assert!(
            validate_session_created_event(RealtimeEvent::SessionUpdated(
                json!({ "type": "translation" })
            ))
            .is_err()
        );
        assert!(
            validate_session_created_event(RealtimeEvent::SessionCreated(Value::Null)).is_err()
        );
        assert!(validate_session_update_event(
            RealtimeEvent::SessionCreated(json!({ "type": "translation" })),
            &languages,
            &CaptionProcessingMode::Translated,
            "config-1",
        )
        .is_err());
        let correlated_error = validate_session_update_event(
            RealtimeEvent::Error(RealtimeServerError {
                message: "bad target".into(),
                kind: Some("invalid_request_error".into()),
                code: Some("invalid_value".into()),
                client_event_id: Some("config-1".into()),
            }),
            &languages,
            &CaptionProcessingMode::Translated,
            "config-1",
        )
        .expect_err("correlated configuration errors must abort startup");
        assert!(!correlated_error.retryable);
    }

    #[test]
    fn validates_acknowledged_translation_configuration() {
        let languages = LanguageSettings {
            source: "ja".into(),
            target: "en".into(),
            explanation: "en".into(),
        };
        let valid = json!({
            "type": "translation",
            "audio": {
                "input": { "transcription": { "model": "gpt-realtime-whisper" } },
                "output": { "language": "en" }
            }
        });
        assert!(
            validate_session_updated(&valid, &languages, &CaptionProcessingMode::Translated)
                .is_ok()
        );

        let mut wrong_target = valid.clone();
        wrong_target["audio"]["output"]["language"] = Value::String("ja".into());
        let error = validate_session_updated(
            &wrong_target,
            &languages,
            &CaptionProcessingMode::Translated,
        )
        .expect_err("wrong target must be rejected");
        assert_eq!(error.kind, ApiErrorKind::MalformedResponse);
        assert!(!error.retryable);
    }

    #[test]
    fn validates_acknowledged_transcription_configuration_and_source_hint() {
        let languages = LanguageSettings {
            source: "ja".into(),
            target: "en".into(),
            explanation: "en".into(),
        };
        let valid = json!({
            "type": "transcription",
            "audio": { "input": {
                "format": { "type": "audio/pcm", "rate": 24_000 },
                "transcription": {
                    "model": "gpt-realtime-whisper",
                    "delay": "low",
                    "language": "ja"
                }
            }}
        });
        assert!(
            validate_session_updated(&valid, &languages, &CaptionProcessingMode::OriginalOnly)
                .is_ok()
        );

        let mut missing_hint = valid;
        missing_hint["audio"]["input"]["transcription"]
            .as_object_mut()
            .expect("transcription object")
            .remove("language");
        assert!(validate_session_updated(
            &missing_hint,
            &languages,
            &CaptionProcessingMode::OriginalOnly
        )
        .is_err());
    }

    #[test]
    fn parses_and_finalizes_transcription_only_items() {
        let delta = parse_realtime_event(
            r#"{"type":"conversation.item.input_audio_transcription.delta","event_id":"evt-1","item_id":"item-1","delta":"今日は"}"#,
        );
        assert!(
            matches!(delta, RealtimeEvent::TranscriptionDelta { item_id, text, .. } if item_id == "item-1" && text == "今日は")
        );
        let mut coordinator = TranscriptionCoordinator::default();
        let events = coordinator.on_delta(
            "item-1".into(),
            Some("evt-1".into()),
            "今日は".into(),
            1_000,
        );
        assert!(
            matches!(events.first(), Some(SessionEvent::CaptionUpserted { segment }) if segment.is_provisional && segment.translation_status == SegmentStatus::Skipped)
        );
        let events = coordinator.on_done(
            "item-1".into(),
            Some("evt-2".into()),
            "今日はちょっと。".into(),
            1_800,
        );
        assert!(
            matches!(events.first(), Some(SessionEvent::TranscriptFinalized { segment }) if !segment.is_provisional && segment.source_text == "今日はちょっと。")
        );
    }

    #[test]
    fn speech_committer_ignores_silence_and_commits_after_quiet() {
        let mut committer = SpeechCommitter::default();
        let frame = |samples: Vec<i16>| CapturedPcm {
            capture_start_sample: 0,
            capture_end_sample: samples.len() as u64,
            samples,
        };
        for _ in 0..6 {
            assert!(committer.push(frame(vec![0; SEND_SAMPLES])).is_empty());
        }
        let loud = vec![4_000; SEND_SAMPLES];
        assert!(committer.push(frame(loud.clone())).is_empty());
        assert!(committer
            .push(frame(loud))
            .iter()
            .any(|action| matches!(action, TranscriptionAudioAction::Append(_))));
        let mut committed = false;
        for _ in 0..4 {
            committed |= committer
                .push(frame(vec![0; SEND_SAMPLES]))
                .iter()
                .any(|action| matches!(action, TranscriptionAudioAction::Commit));
        }
        assert!(committed);
    }

    #[test]
    fn recent_event_identity_is_bounded() {
        let mut ids = RecentEventIds {
            capacity: 3,
            ..RecentEventIds::default()
        };
        assert!(ids.insert("one"));
        assert!(ids.insert("two"));
        assert!(ids.insert("three"));
        assert!(!ids.insert("three"));
        assert!(ids.insert("four"));
        assert_eq!(ids.order.iter().cloned().collect::<Vec<_>>(), ["two", "three", "four"]);
        assert!(ids.insert("one"), "an evicted event can be accepted in a later epoch window");
    }

    #[test]
    fn source_chooser_hides_tiny_untitled_and_non_content_windows() {
        assert!(eligible_window(Some("Japanese livestream"), 1280.0, 720.0, 0));
        assert!(!eligible_window(Some(""), 1280.0, 720.0, 0));
        assert!(!eligible_window(Some("Menu"), 80.0, 40.0, 0));
        assert!(!eligible_window(Some("Overlay"), 1280.0, 720.0, 3));
    }

    #[test]
    fn application_source_prefers_the_display_with_the_largest_visible_area() {
        let first = screencapturekit::cg::CGRect::new(0.0, 0.0, 1000.0, 800.0);
        let second = screencapturekit::cg::CGRect::new(1000.0, 0.0, 1000.0, 800.0);
        let mostly_second = screencapturekit::cg::CGRect::new(900.0, 100.0, 900.0, 600.0);
        assert!(intersection_area(second, mostly_second) > intersection_area(first, mostly_second));
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires a logged-in macOS session and Screen & System Audio Recording permission"]
    async fn native_capture_source_enumeration_returns_a_selectable_source() {
        let sources = list_capture_sources()
            .await
            .expect("ScreenCaptureKit should enumerate visible content");
        assert!(
            !sources.applications.is_empty()
                || !sources.windows.is_empty()
                || !sources.displays.is_empty(),
            "at least one capture source should be available"
        );
        assert!(
            sources
                .applications
                .iter()
                .chain(&sources.windows)
                .all(|source| source.process_id != Some(std::process::id() as i32)),
            "NonoSub must not offer itself as a capture source"
        );
    }

    #[test]
    fn live_clause_pairing_history_stays_within_retention_window() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        for index in 0..320_u64 {
            let clock = index * 1_000;
            coordinator.on_source(
                TimedText {
                    event_id: Some(format!("source-{index}")),
                    text: format!("Source {index}."),
                    elapsed_ms: Some(clock),
                },
                clock,
            );
            coordinator.on_translation(
                TimedText {
                    event_id: Some(format!("target-{index}")),
                    text: format!("Target {index}."),
                    elapsed_ms: Some(clock),
                },
                clock,
            );
            coordinator.tick(clock + TERMINAL_QUIET_MS);
        }
        coordinator.tick(320_000);
        assert!(coordinator.drafts.len() <= MAX_RETAINED_LIVE_UNITS);
        assert!(coordinator.source_clauses.len() <= MAX_RETAINED_LIVE_UNITS);
        assert!(coordinator.translation_clauses.len() <= MAX_RETAINED_LIVE_UNITS);
    }
}
