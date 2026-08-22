import re

p = "crates/jacquard/src/jetstream/replay.rs"
s = open(p).read()

# Trim the header to a maintainer-sized summary.
old_head = s[: s.index("use std::collections::VecDeque;")]
new_head = """//! Replay orchestration: archive backfill with cutover to the live
//! `subscribeEvents` stream.
//!
//! Composes the plan loop, archive transport, `.jss` decoder, and the
//! low-level live stream, and owns the cross-cutting behaviour none of
//! them can: seq-floor dedupe at the archive->live seam, `CursorTooOld`
//! recovery by replanning from the last delivered seq, per-row filter
//! re-application (plans over-include), and `429` backoff.
//!
//! Delivery is at-least-once; handlers should be idempotent, keyed on the
//! at:// URI (and rev for commits). Cursor persistence is caller-owned:
//! `ReplayItem::last_seq` is the checkpoint to store.

"""
s = new_head + s[s.index("use std::collections::VecDeque;"):]

# Restructure next(): move phases out during dispatch (avoids &mut self
# conflicts), restore before continues/returns.
start = s.index("    /// Receive the next event")
end = s.index("    async fn run_plan")
new_next = """    /// Receive the next event, or `None` when the run is complete.
    pub async fn next(&mut self) -> Result<Option<ReplayItem>, ReplayError<C::Error>> {
        loop {
            // Phases move out during dispatch: several arms need `&mut
            // self` (network calls) while holding the phase's contents.
            let phase = core::mem::replace(&mut self.phase, Phase::Done);
            self.phase = match phase {
                Phase::Plan => {
                    self.run_plan().await?;
                    continue;
                }
                Phase::Archive {
                    mut segments,
                    sealed_tip_seq,
                    mut rows,
                } => {
                    if let Some(row) = rows.pop_front() {
                        self.phase = Phase::Archive {
                            segments,
                            sealed_tip_seq,
                            rows,
                        };
                        if let Some(item) = self.row_to_item(row)? {
                            return Ok(Some(item));
                        }
                        continue;
                    }
                    if segments.is_empty() {
                        self.cutover(sealed_tip_seq).await?;
                        continue;
                    }
                    let take = self.options.download_concurrency.min(segments.len());
                    let batch: Vec<Segment<SmolStr>> = segments.drain(..take).collect();
                    for segment in batch.iter() {
                        rows.extend(self.download_segment(segment).await?);
                    }
                    self.phase = Phase::Archive {
                        segments,
                        sealed_tip_seq,
                        rows,
                    };
                    continue;
                }
                Phase::Live { mut stream } => {
                    let message = match stream.next().await {
                        Ok(m) => m,
                        Err(LiveError::Closed) => {
                            // The low-level stream is single-connection by
                            // design; a close during replay is recoverable
                            // by replanning from the floor. Pure-live runs
                            // treat closure as the end of the stream.
                            if matches!(self.mode, ReplayMode::Replay { .. }) {
                                self.replans += 1;
                                if self.replans > self.options.max_replans {
                                    return Err(ReplayError::ReplanExhausted);
                                }
                                Phase::Plan
                            } else {
                                return Ok(None);
                            }
                        }
                        Err(e) => return Err(e.into()),
                    };
                    let seq = message_seq(message);
                    if let Some(seq) = seq {
                        if seq <= self.floor {
                            self.phase = Phase::Live { stream };
                            continue;
                        }
                        self.floor = self.floor.max(seq);
                    }
                    self.phase = Phase::Live { stream };
                    return Ok(Some(ReplayItem {
                        message,
                        last_seq: seq,
                    }));
                }
                Phase::Done => return Ok(None),
            };
        }
    }

"""
s = s[:start] + new_next + s[end:]

# Rework cutover: inline CursorTooOld replan, terminal otherwise.
start = s.index("    /// Establish the live stream at the seam.")
end = s.index("    async fn establish_live")
new_cutover = """    /// Establish the live stream at the seam. `CursorTooOld` re-enters
    /// the plan loop from the last delivered seq; other handshake
    /// rejections are terminal.
    async fn cutover(&mut self, cursor: i64) -> Result<(), ReplayError<C::Error>> {
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
                        // backfill from the last delivered seq.
                        self.replans += 1;
                        if self.replans > self.options.max_replans {
                            return Err(ReplayError::ReplanExhausted);
                        }
                        let after = self.checkpoint();
                        let plan = plan_snapshot(&self.client, &self.filters, after).await?;
                        self.phase = Phase::Archive {
                            segments: plan.segments.into(),
                            sealed_tip_seq: plan.sealed_tip_seq,
                            rows: VecDeque::new(),
                        };
                        Ok(())
                    }
                    error => Err(LiveError::Handshake(super::live::HandshakeError {
                        error,
                        ..h
                    })
                    .into()),
                }
            }
            Err(e) => Err(e.into()),
        }
    }

"""
s = s[:start] + new_cutover + s[end:]

# The old connect_live helper is gone; delete it if still present.
start = s.find("    async fn connect_live(")
if start != -1:
    end = s.index("    async fn establish_live")
    s = s[:start] + s[end:]

open(p, "w").write(s)
print("patched")
