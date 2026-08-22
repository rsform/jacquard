//! Replay orchestration: archive backfill with cutover to the live
//! `subscribeEvents` stream.
//!
//! Composes the plan loop, archive transport, `.jss` decoder, and the
//! low-level live stream, adding what all the connecting bits: seq-floor dedupe at the
//! cutover point, `CursorTooOld` recovery by replanning from the
//! last consumed seq, per-row filter re-application (plans over-include),
//! `429` backoff, and bounded reconnection with exponential backoff.
//!
//! Two seq markers are tracked separately:
//!
//! - [`ReplayStream::checkpoint`] — the highest seq *delivered* to the
//!   caller. Persist this between runs.
//! - the resume anchor (internal) — the highest seq *consumed* from the
//!   archive or live stream, advanced even for filtered-out rows so a
//!   replan never re-downloads the whole window behind a sparse filter.
//!   Because consumption can outrun delivery, redelivery after an error
//!   is possible; delivery is at-least-once either way.
//!
//! Handlers should be idempotent, keyed on the at:// URI and rev for
//! commits. Archive commits carry no CID column, so the idempotency key
//! must not include one. Account-deletion and `sync` events are terminal
//! for the affected repo: consumers should stop applying updates for that
//! DID until it is re-fetched.

use core::fmt;
use core::num::NonZeroUsize;
use core::time::Duration;
use std::collections::VecDeque;

use jacquard_api::network_bsky::jetstream::get_segment::GetSegment;
use jacquard_api::network_bsky::jetstream::plan_snapshot::{
    PlanSnapshotOutput, Segment, SegmentMode,
};
use jacquard_api::network_bsky::jetstream::subscribe_events::SubscribeEventsMessage;
use jacquard_common::DefaultStr;
use jacquard_common::IntoStatic as _;
use jacquard_common::deps::bytes;
use jacquard_common::http_client::{HttpClient, HttpClientExt};
use jacquard_common::websocket::WebSocketClient;
use jacquard_common::xrpc::XrpcClient as _;
use smol_str::{SmolStr, ToSmolStr, format_smolstr};

use super::archive::{JetstreamClient, JetstreamError};
use super::convert::{ConvertError, row_to_message};
use super::live::{self, LiveError, LiveOptions, LiveStream};
use super::plan::{EventKind, PlanCursor, PlanError, ReplayFilters};
use jacquard_common::jss::Kind;
use jacquard_common::jss::{self, HEADER_LEN, SegmentError};

/// Which phases a run covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayMode {
    /// Live only: no archive backfill. Terminal live errors are fatal.
    Live {
        /// Resume position (inclusive); `None` starts at the live tip.
        cursor: Option<i64>,
    },
    /// Archive backfill from `after_seq` (exclusive), then cutover to
    /// live with dedupe.
    Replay {
        /// Backfill start (exclusive); `None` plans the whole archive.
        after_seq: Option<i64>,
    },
    /// Archive only, bounded by `before_seq` (inclusive); the stream
    /// ends once the bound is passed and never connects live.
    Snapshot {
        /// Backfill start (exclusive); `None` plans the whole archive.
        after_seq: Option<i64>,
        /// Upper bound (inclusive); `None` ends at the sealed tip.
        before_seq: Option<i64>,
    },
}

impl ReplayMode {
    /// The snapshot upper bound, if this run is bounded.
    fn bound(&self) -> Option<i64> {
        match self {
            Self::Snapshot { before_seq, .. } => *before_seq,
            _ => None,
        }
    }

    /// The constructor-provided backfill start (exclusive).
    fn initial_after(&self) -> Option<i64> {
        match self {
            Self::Live { cursor } => *cursor,
            Self::Replay { after_seq } | Self::Snapshot { after_seq, .. } => *after_seq,
        }
    }

    fn cuts_over_to_live(&self) -> bool {
        !matches!(self, Self::Snapshot { .. })
    }
}

/// Tuning knobs; defaults in [`Default`].
#[derive(Debug, Clone)]
pub struct ReplayOptions {
    /// Upper bound on concurrent segment/block downloads. Event order is
    /// seq order regardless of download completion order.
    pub download_concurrency: NonZeroUsize,
    /// Cap on archive replans triggered by `CursorTooOld`. Live
    /// reconnections are counted separately and back off instead of
    /// exhausting a budget.
    pub max_replans: u32,
    /// Cap on consecutive `429` retries for a single download before the
    /// error surfaces to the caller.
    pub download_max_retries: u32,
    /// Initial delay before reconnecting a dropped live stream; doubles
    /// per consecutive drop up to [`Self::reconnect_backoff_max`].
    pub reconnect_backoff_start: Duration,
    /// Ceiling for the reconnect backoff.
    pub reconnect_backoff_max: Duration,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            download_concurrency: NonZeroUsize::new(4).expect("nonzero"),
            max_replans: 8,
            download_max_retries: 10,
            reconnect_backoff_start: Duration::from_millis(250),
            reconnect_backoff_max: Duration::from_secs(30),
        }
    }
}

/// One delivered event plus the checkpoint to persist.
#[derive(Debug, Clone)]
pub struct ReplayItem {
    /// The normalized event, identical in shape to a live message.
    pub message: SubscribeEventsMessage<SmolStr>,
    /// The highest seq delivered so far (the durable cursor). `None`
    /// for `#info` frames, which are advisory and carry no seq.
    pub last_seq: Option<i64>,
}

/// Everything the orchestration layer can fail with. `HE` is the HTTP
/// transport's error, `WE` the WebSocket transport's.
#[derive(Debug)]
pub enum ReplayError<HE, WE> {
    /// Archive transport failure.
    Archive(JetstreamError<HE>),
    /// Plan-loop failure.
    Plan(PlanError<HE>),
    /// Live stream failure.
    Live(LiveError<WE>),
    /// `.jss` decode failure.
    Segment(SegmentError),
    /// Row-to-message conversion failure.
    Convert(ConvertError),
    /// `CursorTooOld` replanning exceeded [`ReplayOptions::max_replans`].
    ReplanExhausted,
    /// Consecutive `429` retries for one download exceeded
    /// [`ReplayOptions::download_max_retries`].
    RetryExhausted,
}

impl<HE: fmt::Display, WE: fmt::Display> fmt::Display for ReplayError<HE, WE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "{e}"),
            Self::Plan(e) => write!(f, "{e}"),
            Self::Live(e) => write!(f, "{e}"),
            Self::Segment(e) => write!(f, "{e}"),
            Self::Convert(e) => write!(f, "{e}"),
            Self::ReplanExhausted => write!(f, "cursor replan limit exceeded"),
            Self::RetryExhausted => write!(f, "download retry limit exceeded"),
        }
    }
}

impl<HE: fmt::Display + fmt::Debug, WE: fmt::Display + fmt::Debug> std::error::Error
    for ReplayError<HE, WE>
{
}

impl<HE, WE> From<JetstreamError<HE>> for ReplayError<HE, WE> {
    fn from(e: JetstreamError<HE>) -> Self {
        Self::Archive(e)
    }
}
impl<HE, WE> From<PlanError<HE>> for ReplayError<HE, WE> {
    fn from(e: PlanError<HE>) -> Self {
        Self::Plan(e)
    }
}
impl<HE, WE> From<LiveError<WE>> for ReplayError<HE, WE> {
    fn from(e: LiveError<WE>) -> Self {
        Self::Live(e)
    }
}
impl<HE, WE> From<SegmentError> for ReplayError<HE, WE> {
    fn from(e: SegmentError) -> Self {
        Self::Segment(e)
    }
}
impl<HE, WE> From<ConvertError> for ReplayError<HE, WE> {
    fn from(e: ConvertError) -> Self {
        Self::Convert(e)
    }
}

enum Phase<E> {
    /// Plan (and replan) from the resume anchor.
    Plan,
    /// Establish the live stream at `cursor`; retryable on error.
    Cutover { cursor: i64 },
    /// Drain planned segments, then cutover (or finish, in snapshot
    /// mode).
    Archive {
        segments: VecDeque<Segment<SmolStr>>,
        sealed_tip_seq: i64,
        messages: VecDeque<SubscribeEventsMessage<SmolStr>>,
    },
    /// Live after the archive is consumed (or immediately in
    /// [`ReplayMode::Live`]).
    Live { stream: LiveStream<E, SmolStr> },
    /// Terminal state: snapshot bound reached or live stream ended.
    Done,
}

/// The orchestrated stream. Pull-based: [`ReplayStream::next`] yields
/// `None` when the run is over. An `Err` return leaves the stream in a
/// resumable state — a retryable caller can call `next` again and
/// progress picks up from the last consumed seq.
pub struct ReplayStream<C: HttpClient, W: WebSocketClient> {
    client: JetstreamClient<C>,
    ws: W,
    filters: ReplayFilters,
    mode: ReplayMode,
    options: ReplayOptions,
    phase: Phase<W::Error>,
    /// Highest seq delivered to the caller (the persisted checkpoint).
    delivered_floor: i64,
    /// Highest seq consumed from any source, filtered rows included.
    /// The replan/cursor anchor.
    consumed_seq: Option<i64>,
    replans: u32,
    /// Consecutive live drops; resets after a successful cutover
    /// delivers a message.
    reconnect_attempts: u32,
}

impl<C, W> ReplayStream<C, W>
where
    C: HttpClient + HttpClientExt + Sync,
    W: WebSocketClient,
{
    /// Assemble a run. Nothing hits the network until [`Self::next`].
    pub fn new(
        client: JetstreamClient<C>,
        ws: W,
        filters: ReplayFilters,
        mode: ReplayMode,
        options: ReplayOptions,
    ) -> Self {
        Self {
            client,
            ws,
            filters,
            mode,
            options,
            phase: Phase::Plan,
            delivered_floor: i64::MIN,
            consumed_seq: None,
            replans: 0,
            reconnect_attempts: 0,
        }
    }

    /// The durable checkpoint: the highest seq delivered so far.
    pub fn checkpoint(&self) -> Option<i64> {
        (self.delivered_floor > i64::MIN).then_some(self.delivered_floor)
    }

    /// Where a replan or cursor resumes from: the highest consumed seq
    /// when this run has consumed anything, else the constructor value.
    fn resume_anchor(&self) -> Option<i64> {
        self.consumed_seq.or_else(|| self.mode.initial_after())
    }

    /// Receive the next event, or `None` when the run is complete.
    ///
    /// On `Err` the internal state stays resumable: a transient failure
    /// (network drop, exhausted metering window) can be retried by
    /// calling `next` again.
    pub async fn next(&mut self) -> Result<Option<ReplayItem>, ReplayError<C::Error, W::Error>> {
        loop {
            let phase = core::mem::replace(&mut self.phase, Phase::Done);
            match phase {
                Phase::Plan => {
                    // Errors leave the stream in the plan phase, so a
                    // retry re-plans from the resume anchor.
                    self.phase = Phase::Plan;
                    let next = self.run_plan().await?;
                    self.phase = next;
                }
                Phase::Cutover { cursor } => {
                    self.phase = Phase::Cutover { cursor };
                    self.establish_at(cursor).await?;
                }
                Phase::Archive {
                    mut segments,
                    sealed_tip_seq,
                    mut messages,
                } => {
                    if let Some(message) = messages.pop_front() {
                        self.phase = Phase::Archive {
                            segments,
                            sealed_tip_seq,
                            messages,
                        };
                        if let Some(item) = self.message_to_item(message) {
                            return Ok(Some(item));
                        }
                        continue;
                    }
                    if segments.is_empty() {
                        if self.mode.cuts_over_to_live() {
                            let cursor = sealed_tip_seq.max(self.consumed_seq.unwrap_or(i64::MIN));
                            self.phase = Phase::Cutover { cursor };
                        } else {
                            self.phase = Phase::Done;
                        }
                        continue;
                    }
                    // Take a download batch out and record the pre-batch
                    // state first: a failure mid-batch leaves the stream
                    // positioned to re-download the batch.
                    let take = self.options.download_concurrency.get().min(segments.len());
                    let batch: Vec<Segment<SmolStr>> = segments.drain(..take).collect();
                    self.phase = Phase::Archive {
                        segments,
                        sealed_tip_seq,
                        messages,
                    };
                    let downloaded = self.download_batch(&batch).await?;
                    match &mut self.phase {
                        Phase::Archive { messages, .. } => messages.extend(downloaded),
                        // Only this arm can run; the phase was just set.
                        _ => unreachable!("phase set to Archive before download"),
                    }
                }
                Phase::Live { mut stream } => {
                    let next_phase = match stream.next().await {
                        Ok(message) => {
                            let seq = live_message_seq(&message);
                            if let Some(seq) = seq {
                                if seq > self.consumed_seq.unwrap_or(i64::MIN) {
                                    self.consumed_seq = Some(seq);
                                }
                            }
                            self.phase = Phase::Live { stream };
                            // A delivered message proves the connection
                            // works; reset the reconnect budget.
                            self.reconnect_attempts = 0;
                            if let Some(seq) = seq {
                                if seq <= self.delivered_floor {
                                    continue;
                                }
                                self.delivered_floor = self.delivered_floor.max(seq);
                            }
                            return Ok(Some(ReplayItem {
                                message,
                                last_seq: seq,
                            }));
                        }
                        Err(e @ (LiveError::Closed | LiveError::Stream(_))) => {
                            // The low-level stream is single-connection by
                            // design; the orchestrator reconnects with
                            // backoff in replay mode. Pure-live runs end
                            // on closure and surface mid-stream failures.
                            if matches!(self.mode, ReplayMode::Replay { .. }) {
                                sleep(self.backoff_delay()).await;
                                Phase::Cutover {
                                    cursor: self.consumed_seq.unwrap_or(i64::MIN),
                                }
                            } else if matches!(e, LiveError::Closed) {
                                return Ok(None);
                            } else {
                                return Err(e.into());
                            }
                        }
                        Err(e) => return Err(e.into()),
                    };
                    self.phase = next_phase;
                }
                Phase::Done => return Ok(None),
            }
        }
    }

    /// Exponential reconnect backoff: start doubling per consecutive
    /// drop, capped at the configured maximum.
    fn backoff_delay(&mut self) -> Duration {
        let shift = self.reconnect_attempts.min(16);
        self.reconnect_attempts += 1;
        let delay = self
            .options
            .reconnect_backoff_start
            .saturating_mul(1u32 << shift);
        delay.min(self.options.reconnect_backoff_max)
    }

    async fn run_plan(&mut self) -> Result<Phase<W::Error>, ReplayError<C::Error, W::Error>> {
        match &self.mode {
            ReplayMode::Live { cursor } => Ok(Phase::Cutover {
                cursor: cursor.unwrap_or_else(|| self.consumed_seq.unwrap_or(i64::MIN)),
            }),
            ReplayMode::Replay { .. } => {
                // The orchestrator accumulates planned segments across
                // awaits, so it materializes them with an owned backing;
                // per-page decoding still borrows from each page buffer.
                let mut cursor = PlanCursor::new(&self.filters, self.resume_anchor(), None);
                let mut segments = VecDeque::new();
                while let Some(page) = cursor.next_page(&self.client).await? {
                    let out: PlanSnapshotOutput<DefaultStr> = page.parse().map_err(|e| {
                        ReplayError::Plan(PlanError::Archive(JetstreamError::Decode(e.0)))
                    })?;
                    segments.extend(out.segments);
                }
                Ok(Phase::Archive {
                    sealed_tip_seq: cursor.sealed_tip().unwrap_or(i64::MIN),
                    segments,
                    messages: VecDeque::new(),
                })
            }
            ReplayMode::Snapshot { before_seq, .. } => {
                let mut cursor = PlanCursor::new(&self.filters, self.resume_anchor(), *before_seq);
                let mut segments = VecDeque::new();
                while let Some(page) = cursor.next_page(&self.client).await? {
                    let out: PlanSnapshotOutput<DefaultStr> = page.parse().map_err(|e| {
                        ReplayError::Plan(PlanError::Archive(JetstreamError::Decode(e.0)))
                    })?;
                    segments.extend(out.segments);
                }
                Ok(Phase::Archive {
                    sealed_tip_seq: cursor.sealed_tip().unwrap_or(i64::MIN),
                    segments,
                    messages: VecDeque::new(),
                })
            }
        }
    }

    /// Establish the live stream at `cursor`, with dictionary
    /// negotiation. `CursorTooOld` re-enters the plan loop from the
    /// resume anchor; other handshake rejections are terminal.
    async fn establish_at(&mut self, cursor: i64) -> Result<(), ReplayError<C::Error, W::Error>> {
        let options = LiveOptions {
            cursor: (cursor > i64::MIN).then_some(cursor),
            max_message_size_bytes: None,
        };
        let base = self.client.base_uri().await;
        match self.establish_live(&base, options).await {
            Ok(stream) => {
                self.phase = Phase::Live { stream };
                Ok(())
            }
            Err(LiveError::Handshake(h)) => {
                use jacquard_api::network_bsky::jetstream::subscribe_events::SubscribeEventsError;
                match h.error {
                    SubscribeEventsError::CursorTooOld(_) => {
                        // The cursor fell below the retention floor;
                        // backfill from the resume anchor.
                        self.replans += 1;
                        if self.replans > self.options.max_replans {
                            return Err(ReplayError::ReplanExhausted);
                        }
                        let mut cursor =
                            PlanCursor::new(&self.filters, self.resume_anchor(), self.mode.bound());
                        let mut segments = VecDeque::new();
                        while let Some(page) = cursor.next_page(&self.client).await? {
                            let out: PlanSnapshotOutput<DefaultStr> =
                                page.parse().map_err(|e| {
                                    ReplayError::Plan(PlanError::Archive(JetstreamError::Decode(
                                        e.0,
                                    )))
                                })?;
                            segments.extend(out.segments);
                        }
                        self.phase = Phase::Archive {
                            sealed_tip_seq: cursor.sealed_tip().unwrap_or(i64::MIN),
                            segments,
                            messages: VecDeque::new(),
                        };
                        Ok(())
                    }
                    error => {
                        Err(LiveError::Handshake(super::live::HandshakeError { error, ..h }).into())
                    }
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Connect live, preferring dictionary compression. The fallback to
    /// an uncompressed connection happens only when the dictionary fetch
    /// itself fails; handshake rejections other than
    /// `UnknownZstdDictionary` propagate to the caller.
    #[allow(unused_variables)]
    async fn establish_live(
        &self,
        base: &jacquard_common::deps::fluent_uri::Uri<String>,
        options: LiveOptions,
    ) -> Result<LiveStream<W::Error, SmolStr>, LiveError<W::Error>> {
        live::subscribe_events(&self.client, &self.ws, &self.filters, options).await
    }

    /// Download a batch of planned segments. Downloads run concurrently
    /// up to [`ReplayOptions::download_concurrency`]; decoded messages
    /// return in plan (seq) order. On wasm (single-threaded) downloads
    /// run sequentially in order.
    async fn download_batch(
        &mut self,
        batch: &[Segment<SmolStr>],
    ) -> Result<Vec<SubscribeEventsMessage<SmolStr>>, ReplayError<C::Error, W::Error>> {
        #[cfg(not(target_arch = "wasm32"))]
        let results: Vec<Result<(Vec<_>, i64), _>> = {
            let mut futures = Vec::with_capacity(batch.len());
            for segment in batch {
                futures.push(self.download_segment(segment));
            }
            n0_future::join_all(futures).await
        };
        #[cfg(target_arch = "wasm32")]
        let results: Vec<Result<(Vec<_>, i64), _>> = {
            let mut results = Vec::with_capacity(batch.len());
            for segment in batch {
                results.push(self.download_segment(segment).await);
            }
            results
        };

        let mut all = Vec::new();
        let mut consumed = self.consumed_seq.unwrap_or(i64::MIN);
        for result in results {
            let (messages, segment_max) = result?;
            consumed = consumed.max(segment_max);
            all.extend(messages);
        }
        self.consumed_seq = Some(consumed);
        Ok(all)
    }

    /// Fetch a whole segment with `429` handling: wait the server's
    /// `Retry-After` (or a 1s default) between attempts, bounded by
    /// [`ReplayOptions::download_max_retries`] consecutive rejections.
    /// Fetch a whole segment as a streaming download with byte-offset
    /// resume: bytes already received are kept, and a mid-stream failure
    /// or metering rejection resumes with `Range` from the received
    /// offset instead of re-downloading (and re-paying for) them.
    async fn fetch_segment(
        &self,
        name: &str,
    ) -> Result<bytes::Bytes, ReplayError<C::Error, W::Error>> {
        use jacquard_common::xrpc::XrpcStreamingClient as _;
        use n0_future::StreamExt as _;

        let mut buffer: Vec<u8> = Vec::new();
        let mut rejections = 0u32;
        loop {
            if buffer.is_empty() {
                // Initial attempt: stream the whole segment.
                let response = self
                    .client
                    .download(GetSegment::<DefaultStr> { name: name.into() })
                    .await
                    .map_err(|e| {
                        ReplayError::Archive(JetstreamError::Decode(format_smolstr!(
                            "segment download: {e}"
                        )))
                    })?;
                match response.status() {
                    http::StatusCode::TOO_MANY_REQUESTS => {
                        rejections += 1;
                        if rejections > self.options.download_max_retries {
                            return Err(ReplayError::RetryExhausted);
                        }
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    status if !status.is_success() => {
                        return Err(ReplayError::Archive(JetstreamError::UnexpectedStatus {
                            status,
                            body: SmolStr::new(format!("{status} bytes")),
                        }));
                    }
                    _ => {}
                }
                let mut body = response.into_parts().1.into_inner();
                let mut failed = false;
                while let Some(chunk) = body.next().await {
                    match chunk {
                        Ok(bytes) => buffer.extend_from_slice(bytes.as_ref()),
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                if !failed {
                    return Ok(buffer.into());
                }
                // Fall through to the range-resume path with the partial
                // bytes retained.
            } else {
                // Resume from the received offset.
                let offset = buffer.len() as u64;
                match self.client.get_segment_range(name, offset).await {
                    Ok(response) => {
                        // A server that ignores the Range header returns
                        // 200 with the full body; the resume contract
                        // requires 206.
                        if response.status() != http::StatusCode::PARTIAL_CONTENT {
                            return Err(ReplayError::Archive(JetstreamError::Decode(
                                "server ignored Range resume request".to_smolstr(),
                            )));
                        }
                        let (parts, body) = response.into_parts();
                        validate_content_range(&parts.headers, offset, body.len())?;
                        buffer.extend_from_slice(&body);
                        return Ok(buffer.into());
                    }
                    Err(JetstreamError::ByteLimitExceeded { retry_after }) => {
                        rejections += 1;
                        if rejections > self.options.download_max_retries {
                            return Err(ReplayError::RetryExhausted);
                        }
                        sleep(Duration::from_secs(retry_after.unwrap_or(1))).await;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    /// Fetch one block with `429` handling, as [`Self::fetch_segment`].
    /// Blocks are small (one zstd frame); resume does not apply.
    async fn fetch_block(
        &self,
        segment: &str,
        index: i64,
    ) -> Result<bytes::Bytes, ReplayError<C::Error, W::Error>> {
        let mut rejections = 0u32;
        loop {
            match self.client.get_block(segment, index).await {
                Ok(bytes) => return Ok(bytes),
                Err(JetstreamError::ByteLimitExceeded { retry_after }) => {
                    rejections += 1;
                    if rejections > self.options.download_max_retries {
                        return Err(ReplayError::RetryExhausted);
                    }
                    sleep(Duration::from_secs(retry_after.unwrap_or(1))).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Download and fully decode one planned segment (whole-file or block
    /// ranges) into seq-ordered messages, re-applying filters per row.
    /// Returns the messages plus the highest seq decoded in this segment
    /// (filtered rows included), so the caller's resume anchor advances
    /// past rows it deliberately dropped.
    async fn download_segment(
        &self,
        segment: &Segment<SmolStr>,
    ) -> Result<(Vec<SubscribeEventsMessage<SmolStr>>, i64), ReplayError<C::Error, W::Error>> {
        let bodies = match &segment.mode {
            SegmentMode::Segment => {
                let bytes = self.fetch_segment(segment.name.as_ref()).await?;
                self.decode_whole_segment(&bytes, &segment.checksum)?
            }
            SegmentMode::Blocks => {
                let ranges = segment.blocks.as_deref().unwrap_or(&[]);
                #[cfg(feature = "zstd")]
                let mut bodies = Vec::new();
                #[cfg(not(feature = "zstd"))]
                let bodies = Vec::new();
                for range in ranges {
                    for index in range.first..=range.last {
                        let frame = self.fetch_block(segment.name.as_ref(), index).await?;
                        #[cfg(feature = "zstd")]
                        bodies.push(jss::decompress_bounded(&frame)?);
                        #[cfg(not(feature = "zstd"))]
                        {
                            let _ = frame;
                            return Err(SegmentError::Decompress.into());
                        }
                    }
                }
                bodies
            }
            SegmentMode::Other(mode) => {
                return Err(ReplayError::Archive(JetstreamError::Decode(
                    format_smolstr!("unknown download mode {mode}"),
                )));
            }
        };

        let bound = self.mode.bound();
        let mut messages = Vec::new();
        let mut consumed = i64::MIN;
        for body in &bodies {
            let rows = jss::decode_block_body::<&str, &[u8]>(body)?;
            for row in rows {
                let seq = i64::try_from(row.seq).map_err(|_| ConvertError::SeqOverflow(row.seq))?;
                consumed = consumed.max(seq);
                if let Some(bound) = bound {
                    if seq > bound {
                        continue;
                    }
                }
                let kind = match row.kind {
                    Kind::Create | Kind::Update | Kind::Delete | Kind::CreateResync => {
                        EventKind::Commit
                    }
                    Kind::Identity => EventKind::Identity,
                    Kind::Account => EventKind::Account,
                    Kind::Sync => EventKind::Sync,
                };
                // Bloom-filtered plans over-include; the exact filters
                // apply again per row.
                if self
                    .filters
                    .matches(kind, row.did.as_ref(), row.collection.as_ref())
                {
                    let message = row_to_message(&row)?;
                    messages.push(message.into_static());
                }
            }
        }
        Ok((messages, consumed))
    }

    /// Whole-file path: validate the header and the plan's checksum
    /// before decoding any block.
    fn decode_whole_segment(
        &self,
        bytes: &[u8],
        plan_checksum: &str,
    ) -> Result<Vec<Vec<u8>>, ReplayError<C::Error, W::Error>> {
        let header = jss::SegmentHeader::decode(bytes, bytes.len())?;
        let expected = u64::from_str_radix(plan_checksum, 16).map_err(|_| {
            ReplayError::Archive(JetstreamError::Decode(
                "plan returned an invalid segment checksum".to_smolstr(),
            ))
        })?;
        if header.is_active() || header.checksum != expected {
            return Err(SegmentError::ChecksumMismatch {
                expected,
                actual: header.checksum,
            }
            .into());
        }
        let footer = &bytes[header.block_index_offset as usize..];
        header.verify_checksum(&bytes[..HEADER_LEN], footer)?;

        // Block frames are always zstd on the wire; without the feature
        // there is nothing to decode here. Header and checksum
        // validation above still ran.
        #[cfg(feature = "zstd")]
        {
            let mut bodies = Vec::new();
            let mut pos = HEADER_LEN;
            let stop = header.block_index_offset as usize;
            while pos < stop {
                match jss::read_block_frame(bytes, pos, false)? {
                    Some((body, next)) => {
                        bodies.push(body);
                        pos = next;
                    }
                    None => break,
                }
            }
            Ok(bodies)
        }
        #[cfg(not(feature = "zstd"))]
        {
            let _ = (bytes, header);
            Err(SegmentError::Decompress.into())
        }
    }

    /// Apply the delivered floor to one decoded message.
    fn message_to_item(&mut self, message: SubscribeEventsMessage<SmolStr>) -> Option<ReplayItem> {
        let seq = live_message_seq(&message);
        if let Some(seq) = seq {
            if seq > self.consumed_seq.unwrap_or(i64::MIN) {
                self.consumed_seq = Some(seq);
            }
            if seq <= self.delivered_floor {
                return None;
            }
            self.delivered_floor = self.delivered_floor.max(seq);
        }
        Some(ReplayItem {
            message,
            last_seq: seq,
        })
    }
}

/// The seq of a live-shaped message, if it carries one.
fn live_message_seq(message: &SubscribeEventsMessage<SmolStr>) -> Option<i64> {
    match message {
        SubscribeEventsMessage::Commit(m) => Some(m.seq),
        SubscribeEventsMessage::Identity(m) => Some(m.seq),
        SubscribeEventsMessage::Account(m) => Some(m.seq),
        SubscribeEventsMessage::Sync(m) => Some(m.seq),
        SubscribeEventsMessage::Info(_) | SubscribeEventsMessage::Unknown(_) => None,
    }
}

fn validate_content_range<E, W>(
    headers: &http::HeaderMap,
    expected_start: u64,
    body_len: usize,
) -> Result<(), ReplayError<E, W>> {
    let value = headers
        .get(http::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ReplayError::Archive(JetstreamError::Decode(
                "range response missing Content-Range".to_smolstr(),
            ))
        })?;
    let range = value.strip_prefix("bytes ").ok_or_else(|| {
        ReplayError::Archive(JetstreamError::Decode(
            "invalid Content-Range unit".to_smolstr(),
        ))
    })?;
    let (bounds, total) = range.split_once('/').ok_or_else(|| {
        ReplayError::Archive(JetstreamError::Decode(
            "invalid Content-Range syntax".to_smolstr(),
        ))
    })?;
    let (start, end) = bounds.split_once('-').ok_or_else(|| {
        ReplayError::Archive(JetstreamError::Decode(
            "invalid Content-Range bounds".to_smolstr(),
        ))
    })?;
    let start = start.parse::<u64>().map_err(|_| {
        ReplayError::Archive(JetstreamError::Decode(
            "invalid Content-Range start".to_smolstr(),
        ))
    })?;
    let end = end.parse::<u64>().map_err(|_| {
        ReplayError::Archive(JetstreamError::Decode(
            "invalid Content-Range end".to_smolstr(),
        ))
    })?;
    let total = total.parse::<u64>().map_err(|_| {
        ReplayError::Archive(JetstreamError::Decode(
            "invalid Content-Range total".to_smolstr(),
        ))
    })?;
    let body_len = u64::try_from(body_len).map_err(|_| {
        ReplayError::Archive(JetstreamError::Decode(
            "range response body length overflows u64".to_smolstr(),
        ))
    })?;
    let range_len = end
        .checked_sub(start)
        .and_then(|length| length.checked_add(1));
    if start != expected_start || range_len != Some(body_len) || end >= total {
        return Err(ReplayError::Archive(JetstreamError::Decode(
            format_smolstr!(
                "Content-Range {value:?} does not match offset {expected_start} and body length {body_len}"
            ),
        )));
    }
    Ok(())
}

/// Sleep for a duration. On wasm (no system timer) this yields instead;
/// `Retry-After` pacing is unavailable there and retries are immediate.
async fn sleep(duration: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(duration).await;
    #[cfg(target_arch = "wasm32")]
    {
        n0_future::future::yield_now().await;
        let _ = duration;
    }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NeverWs;

    impl jacquard_common::websocket::WebSocketClient for NeverWs {
        type Error = jacquard_common::websocket::WebSocketError;

        async fn connect(
            &self,
            _uri: jacquard_common::deps::fluent_uri::Uri<&str>,
        ) -> Result<jacquard_common::websocket::WebSocketConnection, Self::Error> {
            Err(jacquard_common::websocket::WebSocketError::Transport(
                "never".into(),
            ))
        }
    }

    #[test]
    fn content_range_must_match_retained_offset_and_body() {
        fn headers(value: Option<&str>) -> http::HeaderMap {
            let mut headers = http::HeaderMap::new();
            if let Some(value) = value {
                headers.insert(
                    http::header::CONTENT_RANGE,
                    http::HeaderValue::from_str(value).expect("header"),
                );
            }
            headers
        }

        assert!(validate_content_range::<(), ()>(&headers(Some("bytes 5-9/10")), 5, 5).is_ok());
        assert!(validate_content_range::<(), ()>(&headers(Some("bytes 4-8/10")), 5, 5).is_err());
        assert!(validate_content_range::<(), ()>(&headers(Some("bytes 5-8/10")), 5, 5).is_err());
        assert!(validate_content_range::<(), ()>(&headers(None), 5, 5).is_err());
        assert!(
            validate_content_range::<(), ()>(
                &headers(Some("bytes 18446744073709551615-18446744073709551615/1")),
                u64::MAX,
                1,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn replay_rejects_unsealed_segment_from_snapshot_plan() {
        let server = MockServer::start().await;
        let mut fixture = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../jacquard-common/src/jss/testdata/segments.jss"
        ))
        .expect("fixture present");
        fixture[4..12].fill(0);
        let plan_page = format!(
            r#"{{"sealedTipSeq":100,"plannedThroughSeq":100,"segments":[{{"name":"seg-0","mode":"segment","checksum":"{SEGMENT_CHECKSUM}","index":0,"minSeq":1,"maxSeq":100}}],"stats":{{"blocksMatched":1,"entries":1,"segmentsExamined":1,"segmentsMatched":1}}}}"#
        );
        server
            .register(
                Mock::given(method("POST"))
                    .and(path("/xrpc/network.bsky.jetstream.planSnapshot"))
                    .respond_with(ResponseTemplate::new(200).set_body_string(plan_page)),
            )
            .await;
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getSegment"))
                    .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture)),
            )
            .await;

        let base = jacquard_common::deps::fluent_uri::Uri::parse(server.uri()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);
        let mut stream = ReplayStream::new(
            client,
            NeverWs,
            ReplayFilters::default(),
            ReplayMode::Snapshot {
                after_seq: None,
                before_seq: None,
            },
            ReplayOptions::default(),
        );
        assert!(matches!(
            stream.next().await,
            Err(ReplayError::Segment(SegmentError::ChecksumMismatch {
                actual: 0,
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn snapshot_rejects_checksum_mismatch() {
        let server = MockServer::start().await;
        let fixture = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../jacquard-common/src/jss/testdata/segments.jss"
        ))
        .expect("fixture present");
        let plan_page = r#"{"sealedTipSeq":100,"plannedThroughSeq":100,"segments":[{"name":"seg-0","mode":"segment","checksum":"deadbeefdeadbeef","index":0,"minSeq":1,"maxSeq":100}],"stats":{"blocksMatched":1,"entries":1,"segmentsExamined":1,"segmentsMatched":1}}"#;
        server
            .register(
                Mock::given(method("POST"))
                    .and(path("/xrpc/network.bsky.jetstream.planSnapshot"))
                    .respond_with(ResponseTemplate::new(200).set_body_string(plan_page)),
            )
            .await;
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getSegment"))
                    .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture)),
            )
            .await;

        let base =
            jacquard_common::deps::fluent_uri::Uri::parse(server.uri().clone()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);

        let mut stream = ReplayStream::new(
            client,
            NeverWs,
            ReplayFilters::default(),
            ReplayMode::Snapshot {
                after_seq: None,
                before_seq: None,
            },
            ReplayOptions::default(),
        );
        match stream.next().await {
            Err(ReplayError::Segment(SegmentError::ChecksumMismatch { .. })) => {}
            other => panic!("expected checksum mismatch, got {other:?}"),
        }
    }

    // ---- scripted WebSocket client -------------------------------------

    mod scripted_ws {
        use jacquard_common::deps::fluent_uri::Uri;
        use jacquard_common::stream::StreamError;
        use jacquard_common::websocket::{
            WebSocketClient, WebSocketConnectOptions, WebSocketConnection, WebSocketError,
            WsMessage, WsSink, WsStream,
        };
        use std::collections::VecDeque;
        use std::pin::Pin;
        use std::sync::{Arc, Mutex};
        use std::task::{Context, Poll};

        /// One connection's scripted outcome.
        pub enum Script {
            /// Deliver these frames, then end the stream cleanly.
            Frames(Vec<String>),
            /// Reject the upgrade with this status and JSON body.
            Reject { status: u16, body: String },
        }

        /// A WebSocket client popping one [`Script`] per connection and
        /// recording the URIs it was asked to connect to.
        #[derive(Clone, Default)]
        pub struct ScriptedWs {
            scripts: Arc<Mutex<VecDeque<Script>>>,
            pub uris: Arc<Mutex<Vec<String>>>,
        }

        impl ScriptedWs {
            pub fn push(&self, script: Script) {
                self.scripts.lock().expect("scripts").push_back(script);
            }
        }

        struct NoopSink;

        impl n0_future::Sink<WsMessage> for NoopSink {
            type Error = StreamError;

            fn poll_ready(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn start_send(self: Pin<&mut Self>, _item: WsMessage) -> Result<(), Self::Error> {
                Ok(())
            }

            fn poll_flush(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn poll_close(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
        }

        impl WebSocketClient for ScriptedWs {
            type Error = WebSocketError;

            async fn connect(&self, _uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
                unreachable!("subscriptions use connect_with_options")
            }

            async fn connect_with_options(
                &self,
                uri: Uri<&str>,
                _options: WebSocketConnectOptions<'_>,
            ) -> Result<WebSocketConnection, Self::Error> {
                self.uris
                    .lock()
                    .expect("uris")
                    .push(uri.as_str().to_string());
                match self.scripts.lock().expect("scripts").pop_front() {
                    Some(Script::Reject { status, body }) => {
                        Err(WebSocketError::HandshakeRejected {
                            status: http::StatusCode::from_u16(status).expect("valid status"),
                            headers: Vec::new(),
                            body: jacquard_common::deps::bytes::Bytes::from(body),
                        })
                    }
                    Some(Script::Frames(frames)) => {
                        let messages = frames
                            .into_iter()
                            .map(|f| {
                                Ok(WsMessage::Text(jacquard_common::websocket::WsText::from(f)))
                            })
                            .collect::<Vec<_>>();
                        let stream = n0_future::stream::iter(messages);
                        Ok(WebSocketConnection::new(
                            WsSink::new(NoopSink),
                            WsStream::new(stream),
                        ))
                    }
                    None => {
                        let n = self.uris.lock().expect("uris").len();
                        panic!(
                            "scripted WS exhausted after {n} connects: {:?}",
                            self.uris.lock().expect("uris")
                        )
                    }
                }
            }
        }

        /// A valid `xrpc.v1.json` message frame containing `#identity` at `seq`.
        pub fn identity_frame(seq: i64) -> String {
            format!(
                r#"{{"$type":"message","payload":{{"$type":"network.bsky.jetstream.subscribeEvents#identity","did":"did:plc:test","seq":{seq},"time":"2026-01-01T00:00:00.000Z","identity":{{"did":"did:plc:test","handle":"example.com","seq":1,"time":"2026-01-01T00:00:00.000Z"}}}}}}"#
            )
        }
    }

    use jacquard_common::websocket::WebSocketConnection;
    use scripted_ws::{Script, ScriptedWs, identity_frame};

    const SEGMENT_CHECKSUM: &str = "0b5b436ea46204a1";

    fn plan_page(tip: i64, through: i64, names: &[&str]) -> String {
        let segments: Vec<String> = names
            .iter()
            .map(|n| {
                format!(
                    r#"{{"name":"{n}","mode":"segment","checksum":"{SEGMENT_CHECKSUM}","index":0,"minSeq":1,"maxSeq":40}}"#
                )
            })
            .collect();
        format!(
            r#"{{"sealedTipSeq":{tip},"plannedThroughSeq":{through},"segments":[{}],"stats":{{"blocksMatched":1,"entries":1,"segmentsExamined":1,"segmentsMatched":1}}}}"#,
            segments.join(",")
        )
    }

    /// Mock one planSnapshot page: requests whose `afterSeq` matches
    /// (`None` matches the field being absent) get `page_body` back.
    async fn mock_plan(server: &wiremock::MockServer, after_seq: Option<i64>, page_body: &str) {
        use wiremock::matchers::body_partial_json;
        let matcher = match after_seq {
            Some(seq) => serde_json::json!({ "afterSeq": seq }),
            None => serde_json::json!({}),
        };
        server
            .register(
                Mock::given(method("POST"))
                    .and(path("/xrpc/network.bsky.jetstream.planSnapshot"))
                    .and(body_partial_json(matcher))
                    .respond_with(ResponseTemplate::new(200).set_body_string(page_body))
                    .up_to_n_times(1),
            )
            .await;
    }

    async fn mock_segment(server: &wiremock::MockServer, name: &str) {
        use wiremock::matchers::query_param;
        use wiremock::{Mock, ResponseTemplate};
        let fixture = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../jacquard-common/src/jss/testdata/segments.jss"
        ))
        .expect("fixture present");
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getSegment"))
                    .and(query_param("name", name))
                    .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture)),
            )
            .await;
    }

    /// The plan-loop page fetch uses streaming download for segments, but
    /// planSnapshot itself goes through the buffered path; the dictionary
    /// fetch attempt (compression negotiation) will 404 on this server
    /// and fall back to an uncompressed connect.
    #[test]
    fn scripted_identity_frame_parses() {
        use jacquard_api::network_bsky::jetstream::subscribe_events::{
            SubscribeEventsMessage, SubscribeEventsStream,
        };
        use jacquard_common::xrpc::SubscriptionResp as _;

        let frame = identity_frame(41);
        let parsed = SubscribeEventsStream::decode_message::<smol_str::SmolStr>(frame.as_bytes())
            .expect("frame must parse");
        let SubscribeEventsMessage::Identity(inner) = parsed else {
            panic!("frame must decode to the identity variant, got {parsed:?}");
        };
        assert_eq!(inner.seq, 41);
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn replay_cutover_deduplicates_seam_event() {
        let server = wiremock::MockServer::start().await;
        mock_plan(&server, None, &plan_page(40, 40, &["seg-0"])).await;
        mock_segment(&server, "seg-0").await;

        let ws = ScriptedWs::default();
        // Live replay is inclusive from the cursor (= sealed tip 40): the
        // seam event 40 arrives again and must be dropped by the floor.
        ws.push(Script::Frames(vec![identity_frame(40), identity_frame(41)]));

        let base =
            jacquard_common::deps::fluent_uri::Uri::parse(server.uri().clone()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);
        let mut stream = ReplayStream::new(
            client,
            ws,
            ReplayFilters::default(),
            ReplayMode::Replay { after_seq: None },
            ReplayOptions::default(),
        );

        let mut seqs = Vec::new();
        while let Some(item) = stream.next().await.expect("stream") {
            if let Some(seq) = item.last_seq {
                seqs.push(seq);
                if seq == 41 {
                    break;
                }
            }
        }
        assert_eq!(seqs.len(), 41, "events 1..41 delivered exactly once");
        assert_eq!(seqs.last(), Some(&41));
        assert_eq!(seqs.iter().filter(|s| **s == 40).count(), 1);
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn cursor_too_old_replans_from_last_seq() {
        let server = wiremock::MockServer::start().await;
        mock_plan(&server, None, &plan_page(40, 40, &["seg-0"])).await;
        mock_plan(&server, Some(40), &plan_page(40, 40, &["seg-0"])).await;
        mock_segment(&server, "seg-0").await;

        let ws = ScriptedWs::default();
        // First live attempt: rejected pre-upgrade with a floor of 10.
        ws.push(Script::Reject {
            status: 400,
            body: r#"{"error":"CursorTooOld","message":"cursor 40 below lookback floor 10; re-backfill from your last seq"}"#.to_string(),
        });
        // After the replan and second cutover, delivery continues past 40.
        ws.push(Script::Frames(vec![identity_frame(41)]));

        let base =
            jacquard_common::deps::fluent_uri::Uri::parse(server.uri().clone()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);
        let mut stream = ReplayStream::new(
            client,
            ws,
            ReplayFilters::default(),
            ReplayMode::Replay { after_seq: None },
            ReplayOptions::default(),
        );

        let mut seen_41 = false;
        // The archive replan re-delivers nothing (floor) and live delivers 41.
        while let Some(item) = stream.next().await.expect("stream") {
            if item.last_seq == Some(41) {
                seen_41 = true;
                break;
            }
        }
        assert!(seen_41, "replan resumed delivery past the floor");
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn snapshot_only_ends_at_tip() {
        let server = wiremock::MockServer::start().await;
        mock_plan(&server, None, &plan_page(40, 40, &["seg-0"])).await;
        mock_segment(&server, "seg-0").await;

        // A WS client that fails the test if snapshot mode ever connects.
        #[derive(Clone, Default)]
        struct NeverConnect;

        impl jacquard_common::websocket::WebSocketClient for NeverConnect {
            type Error = jacquard_common::websocket::WebSocketError;

            async fn connect(
                &self,
                _uri: jacquard_common::deps::fluent_uri::Uri<&str>,
            ) -> Result<WebSocketConnection, jacquard_common::websocket::WebSocketError>
            {
                panic!("snapshot mode must never connect live");
            }

            async fn connect_with_options(
                &self,
                _uri: jacquard_common::deps::fluent_uri::Uri<&str>,
                _options: jacquard_common::websocket::WebSocketConnectOptions<'_>,
            ) -> Result<WebSocketConnection, jacquard_common::websocket::WebSocketError>
            {
                panic!("snapshot mode must never connect live");
            }
        }

        let base =
            jacquard_common::deps::fluent_uri::Uri::parse(server.uri().clone()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);
        let mut stream = ReplayStream::new(
            client,
            NeverConnect::default(),
            ReplayFilters::default(),
            ReplayMode::Snapshot {
                after_seq: None,
                before_seq: None,
            },
            ReplayOptions::default(),
        );

        let mut count = 0u32;
        let mut last = 0i64;
        while let Some(item) = stream.next().await.expect("stream") {
            if let Some(seq) = item.last_seq {
                count += 1;
                last = seq;
            }
        }
        assert_eq!(count, 40, "archive events 1..40 delivered");
        assert_eq!(last, 40, "ends at the sealed tip");
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn plan_loop_pages_until_sealed_tip() {
        let server = wiremock::MockServer::start().await;
        // Page 1: partial (plannedThrough 50 < tip 100) with seg-0;
        // page 2 completes at the tip with seg-1.
        mock_plan(&server, None, &plan_page(100, 50, &["seg-0"])).await;
        mock_plan(&server, Some(50), &plan_page(100, 100, &["seg-1"])).await;
        mock_segment(&server, "seg-0").await;
        mock_segment(&server, "seg-1").await;

        let ws = ScriptedWs::default();
        ws.push(Script::Frames(vec![identity_frame(101)]));

        let base =
            jacquard_common::deps::fluent_uri::Uri::parse(server.uri().clone()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);
        let mut stream = ReplayStream::new(
            client,
            ws,
            ReplayFilters::default(),
            ReplayMode::Replay { after_seq: None },
            ReplayOptions::default(),
        );

        let mut seen_101 = false;
        while let Some(item) = stream.next().await.expect("stream") {
            if item.last_seq == Some(101) {
                seen_101 = true;
                break;
            }
        }
        assert!(seen_101, "both pages planned and cutover delivered");
        server.verify().await;
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn downloads_respect_concurrency_bound() {
        let server = wiremock::MockServer::start().await;
        let names = ["seg-0", "seg-1", "seg-2", "seg-3", "seg-4"];
        mock_plan(&server, None, &plan_page(40, 40, &names)).await;
        for name in names {
            mock_segment(&server, name).await;
        }

        let base = jacquard_common::deps::fluent_uri::Uri::parse(server.uri()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);
        let mut options = ReplayOptions::default();
        options.download_concurrency = NonZeroUsize::new(2).expect("nonzero");
        let mut stream = ReplayStream::new(
            client,
            NeverWs,
            ReplayFilters::default(),
            ReplayMode::Snapshot {
                after_seq: None,
                before_seq: None,
            },
            options,
        );

        assert!(stream.next().await.expect("stream").is_some());
        let requests = server.received_requests().await.expect("request recording");
        let downloads = requests
            .iter()
            .filter(|request| request.url.path() == "/xrpc/network.bsky.jetstream.getSegment")
            .count();
        assert_eq!(
            downloads, 2,
            "the next batch must not start while messages from the bounded batch remain queued"
        );
    }
}
