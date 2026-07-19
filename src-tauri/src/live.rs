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
    async_api::{AsyncSCContentSharingPicker, AsyncSCStream},
    cm::CMSampleBufferExt,
    content_sharing_picker::{
        SCContentSharingPickerConfiguration, SCContentSharingPickerMode, SCPickerOutcome,
    },
    stream::{configuration::SCStreamConfiguration, output_type::SCStreamOutputType},
};
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
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};
use unicode_segmentation::UnicodeSegmentation;

const REALTIME_TRANSLATION_URL: &str =
    "wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate";
const REALTIME_TRANSCRIPTION_URL: &str =
    "wss://api.openai.com/v1/realtime?model=gpt-realtime-whisper";
const SEND_SAMPLES: usize = 2_400;
const TERMINAL_QUIET_MS: u64 = 350;
const IDLE_CLAUSE_MS: u64 = 1_200;
const MAX_ALIGNED_CLAUSE_MS: u64 = 8_000;
const MAX_CAPTURE_CLAUSE_MS: u64 = 10_000;
const MAX_CLAUSE_GRAPHEMES: usize = 220;
const CLAUSE_PAIR_TOLERANCE_MS: u64 = 1_000;
const SOURCE_FALLBACK_MS: u64 = 6_000;
type RealtimeSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type RealtimeWriter = SplitSink<RealtimeSocket, Message>;
type RealtimeReader = SplitStream<RealtimeSocket>;

#[derive(Debug, Default)]
pub struct LiveState {
    cancelled: Arc<AtomicBool>,
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
    Error(String),
    Ignored,
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
    seen_event_ids: HashSet<String>,
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
            seen_event_ids: HashSet::new(),
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
            Some(id) if !id.is_empty() => self.seen_event_ids.insert(id.clone()),
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
        if self.last_emitted_sync.as_ref() == Some(&sync) {
            Vec::new()
        } else {
            self.last_emitted_sync = Some(sync.clone());
            vec![SessionEvent::LiveSyncChanged { sync }]
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
    seen_event_ids: HashSet<String>,
    next_segment: u64,
}

impl TranscriptionCoordinator {
    fn accept(&mut self, event_id: &Option<String>) -> bool {
        match event_id {
            Some(id) if !id.is_empty() => self.seen_event_ids.insert(id.clone()),
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
        vec![
            SessionEvent::TranscriptFinalized {
                segment: segment.clone(),
            },
            Self::sync(segment.id.clone()),
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
    Append(Vec<i16>),
    Commit,
}

#[derive(Debug)]
struct SpeechCommitter {
    preroll: VecDeque<Vec<i16>>,
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
    fn push(&mut self, samples: Vec<i16>) -> Vec<TranscriptionAudioAction> {
        let rms = rms(&samples);
        let threshold = (self.noise_floor * 3.0).max(500.0);
        let speech = rms >= threshold;
        let mut actions = Vec::new();

        if !self.active {
            if !speech {
                self.noise_floor = self.noise_floor * 0.96 + rms * 0.04;
            }
            self.preroll.push_back(samples);
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
        actions.push(TranscriptionAudioAction::Append(samples));
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

pub async fn start(
    app: tauri::AppHandle,
    state: &LiveState,
    api_key: String,
    languages: LanguageSettings,
    sync_mode: LiveSyncMode,
    processing_mode: CaptionProcessingMode,
    generation: u64,
) -> Result<(), ApiError> {
    abort_previous(state);
    state.cancelled.store(false, Ordering::Relaxed);

    let mut picker_config = SCContentSharingPickerConfiguration::new();
    picker_config.set_allows_changing_selected_content(false);
    picker_config.set_allowed_picker_modes(&[
        SCContentSharingPickerMode::SingleDisplay,
        SCContentSharingPickerMode::SingleApplication,
        SCContentSharingPickerMode::SingleWindow,
    ]);
    let picked = AsyncSCContentSharingPicker::show(&picker_config).await;
    let filter = match picked {
        SCPickerOutcome::Picked(result) => result.filter(),
        SCPickerOutcome::Cancelled => {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "Live caption selection was cancelled.".into(),
                retryable: true,
            })
        }
        SCPickerOutcome::Error(error) => {
            return Err(capture_error(&format!(
                "Apple's capture picker failed: {error}"
            )))
        }
    };

    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(1)
        .with_excludes_current_process_audio(true);
    let stream = AsyncSCStream::new(&filter, &config, 8, SCStreamOutputType::Audio);

    let (mut writer, mut reader) = connect_realtime(&api_key, &languages, &processing_mode).await?;

    stream.start_capture().await.map_err(|error| {
        capture_error(&format!("System audio capture could not start: {error}"))
    })?;
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
        let mut coordinator = CaptionCoordinator::new(sync_mode);
        let mut transcription = TranscriptionCoordinator::default();
        let mut speech_committer = SpeechCommitter::default();
        let mut sent_samples = 0_u64;
        let mut ready = false;
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let mut closing = false;
        let mut close_started = None;
        let mut closed = false;
        let mut reconnect_used = false;

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
                    let _ = writer
                        .send(Message::Text(close_event.to_string().into()))
                        .await;
                }
            }
            if closed
                || close_started.is_some_and(|started| started.elapsed() > Duration::from_secs(2))
            {
                break;
            }

            tokio::select! {
                sample = stream.next(), if !closing => {
                    match sample {
                        Some(sample) => {
                            if let Some(list) = sample.audio_buffer_list() {
                                for buffer in list.iter() { append_f32_48k_as_pcm16_24k(buffer.data(), &mut pcm); }
                            } else {
                                emit_recoverable(&app, generation, "capture_buffer", "A system-audio buffer could not be read.");
                            }
                            // The CoreMedia buffer is dropped before the socket await; it is not Send.
                            drop(sample);
                            {
                                while pcm.len() >= SEND_SAMPLES {
                                    let samples: Vec<i16> = pcm.drain(..SEND_SAMPLES).collect();
                                    sent_samples = sent_samples.saturating_add(SEND_SAMPLES as u64);
                                    let actions = if original_only {
                                        speech_committer.push(samples)
                                    } else {
                                        vec![TranscriptionAudioAction::Append(samples)]
                                    };
                                    for action in actions {
                                        let event = match action {
                                            TranscriptionAudioAction::Append(samples) => {
                                                let mut bytes = Vec::with_capacity(samples.len() * 2);
                                                for sample in samples { bytes.extend_from_slice(&sample.to_le_bytes()); }
                                                json!({
                                                    "type": if original_only { "input_audio_buffer.append" } else { "session.input_audio_buffer.append" },
                                                    "audio": BASE64.encode(bytes)
                                                })
                                            }
                                            TranscriptionAudioAction::Commit => json!({ "type": "input_audio_buffer.commit" }),
                                        };
                                        if writer.send(Message::Text(event.to_string().into())).await.is_err() {
                                            if !reconnect_used {
                                                reconnect_used = true;
                                                let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                                if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode).await {
                                                    writer = next_writer;
                                                    reader = next_reader;
                                                    if original_only {
                                                        emit_events(&app, generation, transcription.finish(capture_clock_ms(sent_samples)));
                                                        speech_committer = SpeechCommitter::default();
                                                    } else {
                                                        emit_events(&app, generation, coordinator.begin_epoch(capture_clock_ms(sent_samples)));
                                                    }
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
                        None => break,
                    }
                }
                message = reader.next() => {
                    let Some(message) = message else {
                        if !reconnect_used {
                            reconnect_used = true;
                            let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                            if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode).await {
                                writer = next_writer;
                                reader = next_reader;
                                if original_only {
                                    emit_events(&app, generation, transcription.finish(capture_clock_ms(sent_samples)));
                                    speech_committer = SpeechCommitter::default();
                                } else {
                                    emit_events(&app, generation, coordinator.begin_epoch(capture_clock_ms(sent_samples)));
                                }
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
                                emit_events(&app, generation, coordinator.on_source(delta, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::TranslationDelta(delta) => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                emit_events(&app, generation, coordinator.on_translation(delta, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::TranscriptionDelta { item_id, event_id, text } => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                emit_events(&app, generation, transcription.on_delta(item_id, event_id, text, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::TranscriptionDone { item_id, event_id, text } => {
                                emit_events(&app, generation, transcription.on_done(item_id, event_id, text, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::Closed => closed = true,
                            RealtimeEvent::Error(message) => emit_recoverable(&app, generation, "realtime_error", &message),
                            RealtimeEvent::Ignored => {}
                        },
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(error) => {
                            if !reconnect_used {
                                reconnect_used = true;
                                let _ = record_event_for_generation(&app, generation, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                if let Ok((next_writer, next_reader)) = connect_realtime(&api_key, &languages, &processing_mode).await {
                                    writer = next_writer;
                                    reader = next_reader;
                                    if original_only {
                                        emit_events(&app, generation, transcription.finish(capture_clock_ms(sent_samples)));
                                        speech_committer = SpeechCommitter::default();
                                    } else {
                                        emit_events(&app, generation, coordinator.begin_epoch(capture_clock_ms(sent_samples)));
                                    }
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
                        emit_events(&app, generation, coordinator.tick(capture_clock_ms(sent_samples)));
                    }
                }
            }
        }

        if original_only {
            emit_events(
                &app,
                generation,
                transcription.finish(capture_clock_ms(sent_samples)),
            );
        } else {
            emit_events(
                &app,
                generation,
                coordinator.finish(capture_clock_ms(sent_samples)),
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

fn capture_clock_ms(sent_samples: u64) -> u64 {
    sent_samples.saturating_mul(1_000) / 24_000
}

pub fn stop(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
}

fn abort_previous(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
    if let Ok(mut task) = state.task.lock() {
        if let Some(task) = task.take() {
            task.abort();
        }
    }
}

async fn connect_realtime(
    api_key: &str,
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
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
    let (socket, _) = connect_async(request)
        .await
        .map_err(|error| network_error(&format!("Could not connect realtime captions: {error}")))?;
    let (mut writer, reader) = socket.split();
    let update = realtime_session_update(languages, processing_mode);
    writer
        .send(Message::Text(update.to_string().into()))
        .await
        .map_err(|error| {
            network_error(&format!("Could not configure realtime captions: {error}"))
        })?;
    Ok((writer, reader))
}

fn realtime_session_update(
    languages: &LanguageSettings,
    processing_mode: &CaptionProcessingMode,
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

fn append_f32_48k_as_pcm16_24k(bytes: &[u8], output: &mut Vec<i16>) {
    let mut frames = bytes.chunks_exact(4);
    while let Some(first) = frames.next() {
        let Some(second) = frames.next() else {
            break;
        };
        let a = f32::from_ne_bytes(first.try_into().expect("four-byte float"));
        let b = f32::from_ne_bytes(second.try_into().expect("four-byte float"));
        let averaged = ((a + b) * 0.5).clamp(-1.0, 1.0);
        output.push((averaged * i16::MAX as f32).round() as i16);
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
        "error" => RealtimeEvent::Error(
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Realtime captions reported an error.")
                .to_owned(),
        ),
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
        append_f32_48k_as_pcm16_24k(&input, &mut output);
        assert_eq!(output.len(), 2);
        assert!((output[0] - 16_384).abs() <= 1);
        assert_eq!(output[1], -32_767);
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
            RealtimeEvent::Error("bad audio".into())
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
            source: "auto".into(),
            target: "ja".into(),
            explanation: "ja".into(),
        };
        let event = realtime_session_update(&languages, &CaptionProcessingMode::Translated);
        assert_eq!(
            event["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-realtime-whisper"
        );
        assert_eq!(event["session"]["audio"]["output"]["language"], "ja");
    }

    #[test]
    fn configures_original_only_as_a_low_delay_transcription_session() {
        let languages = LanguageSettings {
            source: "ja".into(),
            target: "en".into(),
            explanation: "en".into(),
        };
        let event = realtime_session_update(&languages, &CaptionProcessingMode::OriginalOnly);
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
        for _ in 0..6 {
            assert!(committer.push(vec![0; SEND_SAMPLES]).is_empty());
        }
        let loud = vec![4_000; SEND_SAMPLES];
        assert!(committer.push(loud.clone()).is_empty());
        assert!(committer
            .push(loud)
            .iter()
            .any(|action| matches!(action, TranscriptionAudioAction::Append(_))));
        let mut committed = false;
        for _ in 0..4 {
            committed |= committer
                .push(vec![0; SEND_SAMPLES])
                .iter()
                .any(|action| matches!(action, TranscriptionAudioAction::Commit));
        }
        assert!(committed);
    }
}
