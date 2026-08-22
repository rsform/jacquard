//! `.jss` sealed-segment decoding for Jetstream v2 archives.
//!
//! Wire format (from the upstream [README](https://github.com/bluesky-social/jetstream/blob/main/docs/README.md), sections 3.1.2 and 3.2):
//!
//! - 256-byte little-endian fixed header: magic `jss0`, xxh3 checksum
//!   (over `header[12..] || footer`, zero = active/unsealed), version,
//!   counts, seq/time bounds, and five offsets.
//! - Block frames from offset 256: `[u64 compressed_len][zstd frame]`.
//!   Each frame decompresses to a columnar event body.
//! - Columnar body: `u32 count`, nine fixed-size columns, then five
//!   concatenated variable-length regions.
//! - Footer block index: 52-byte entries describing each block.
//!
//! Rows decode directly into the generated
//! [`network_bsky::jetstream::subscribe_events`] message shapes so archive
//! replay and live streams share one representation.

use alloc::vec::Vec;
use core::fmt;

/// Magic bytes at offset 0 of every segment file.
pub const SEGMENT_MAGIC: [u8; 4] = *b"jss0";

/// Fixed header length in bytes.
pub const HEADER_LEN: usize = 256;

/// The only on-disk header version produced or accepted.
pub const CURRENT_VERSION: u16 = 1;

/// Size of one footer block-index entry in bytes.
pub const BLOCK_INDEX_ENTRY_LEN: usize = 52;

/// Maximum accepted decompressed block size (guards hostile inputs).
pub const MAX_BLOCK_UNCOMPRESSED: usize = 1024 * 1024 * 1024;

/// Parse errors surfaced by the decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentError {
    /// Leading magic is not `jss0`.
    BadMagic,
    /// Header version is not supported.
    WrongVersion(u16),
    /// A declared offset or size exceeds the file, or regions overlap.
    InvalidOffset,
    /// File ends mid-header, mid-block, or mid-footer without a clean
    /// boundary. For active segments this is expected at the tail.
    Truncated,
    /// Sealed-header checksum does not match the computed value.
    ChecksumMismatch {
        /// The checksum recorded in the header.
        expected: u64,
        /// The recomputed checksum over header and footer bytes.
        actual: u64,
    },
    /// A columnar row carries an unknown kind discriminator.
    UnknownKind(u8),
    /// A variable-length region does not consume exactly its declared bytes,
    /// or trailing data remains after the last payload.
    MalformedBody(&'static str),
    /// Block decompression failed (zstd checksum mismatch or corrupt frame).
    Decompress,
}

impl fmt::Display for SegmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "bad segment magic (expected \"jss0\")"),
            Self::WrongVersion(v) => {
                write!(
                    f,
                    "unsupported segment version {v} (expected {CURRENT_VERSION})"
                )
            }
            Self::InvalidOffset => write!(f, "declared offset/size out of bounds"),
            Self::Truncated => write!(f, "file truncated mid-structure"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(
                    f,
                    "header checksum mismatch: expected {expected:#x}, got {actual:#x}"
                )
            }
            Self::UnknownKind(k) => write!(f, "unknown event kind {k}"),
            Self::MalformedBody(what) => write!(f, "malformed block body: {what}"),
            Self::Decompress => write!(f, "block decompression failed"),
        }
    }
}

impl std::error::Error for SegmentError {}

/// Parsed 256-byte segment header.
///
/// Offsets are absolute file positions. `did_bloom_offset`,
/// `block_did_bloom_offset`, and `collection_index_offset` are retained but
/// not parsed: they hold server-side acceleration metadata (gloom bloom
/// filters, collection index) that clients do not consume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentHeader {
    /// On-disk format version (always [`CURRENT_VERSION`] after decode).
    pub version: u16,
    /// Number of blocks in the sealed file.
    pub block_count: u32,
    /// Total events across all blocks.
    pub event_count: u32,
    /// Distinct repository DIDs referenced.
    pub unique_did_count: u32,
    /// Lowest sequence number in the file.
    pub min_seq: u64,
    /// Highest sequence number in the file.
    pub max_seq: u64,
    /// Unix microseconds.
    pub min_witnessed_at: i64,
    /// Unix microseconds.
    pub max_witnessed_at: i64,
    /// Start of the variable-length footer (blooms, indexes, block index).
    pub footer_offset: u64,
    /// Start of the segment-wide DID bloom filter (unparsed).
    pub did_bloom_offset: u64,
    /// Start of the per-block DID bloom filters (unparsed).
    pub block_did_bloom_offset: u64,
    /// Start of the collection block index (unparsed).
    pub collection_index_offset: u64,
    /// Start of the block index; equals `footer_offset` in v1 files.
    pub block_index_offset: u64,
    /// Zero when the segment is still active/unsealed.
    pub checksum: u64,
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().expect("4-byte slice"))
}

fn read_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().expect("8-byte slice"))
}

fn read_i64(buf: &[u8], off: usize) -> i64 {
    read_u64(buf, off) as i64
}

impl SegmentHeader {
    /// Parse and validate a 256-byte header against a file of `file_len`
    /// bytes. Does not verify the checksum; see [`SegmentHeader::verify_checksum`].
    pub fn decode(buf: &[u8], file_len: usize) -> Result<Self, SegmentError> {
        if buf.len() < HEADER_LEN || file_len < HEADER_LEN {
            return Err(SegmentError::Truncated);
        }
        if buf[0..4] != SEGMENT_MAGIC {
            return Err(SegmentError::BadMagic);
        }
        let version = read_u16(buf, 12);
        if version != CURRENT_VERSION {
            return Err(SegmentError::WrongVersion(version));
        }

        let header = Self {
            version,
            block_count: read_u32(buf, 14),
            event_count: read_u32(buf, 18),
            unique_did_count: read_u32(buf, 22),
            min_seq: read_u64(buf, 26),
            max_seq: read_u64(buf, 34),
            min_witnessed_at: read_i64(buf, 42),
            max_witnessed_at: read_i64(buf, 50),
            footer_offset: read_u64(buf, 58),
            did_bloom_offset: read_u64(buf, 66),
            block_did_bloom_offset: read_u64(buf, 74),
            collection_index_offset: read_u64(buf, 82),
            block_index_offset: read_u64(buf, 90),
            checksum: read_u64(buf, 4),
        };

        // Sealed layout: blocks end at footer_offset, the block index
        // starts there (block_index_offset == footer_offset), followed by
        // the did bloom, block did bloom, and collection index, in order,
        // without overlap. Active files carry zero offsets and are exempt.
        let len = file_len as u64;
        if !header.is_active()
            && (header.footer_offset < HEADER_LEN as u64
                || header.footer_offset != header.block_index_offset
                || header.did_bloom_offset < header.footer_offset
                || header.block_did_bloom_offset < header.did_bloom_offset
                || header.collection_index_offset < header.block_did_bloom_offset
                || header.collection_index_offset > len)
        {
            return Err(SegmentError::InvalidOffset);
        }

        Ok(header)
    }

    /// Whether this header describes an active (unsealed) segment.
    pub fn is_active(&self) -> bool {
        self.checksum == 0
    }

    /// Verify the seal-time checksum: xxh3 over `header[12..256] || footer`.
    ///
    /// `footer_bytes` must be the exact tail of the file starting at
    /// [`Self::block_index_offset`]. Only meaningful for sealed headers;
    /// active headers have a zero checksum by definition.
    pub fn verify_checksum(
        &self,
        header_bytes: &[u8],
        footer_bytes: &[u8],
    ) -> Result<(), SegmentError> {
        if header_bytes.len() < HEADER_LEN {
            return Err(SegmentError::Truncated);
        }
        let mut hasher = xxhash_rust::xxh3::Xxh3::default();
        hasher.update(&header_bytes[12..HEADER_LEN]);
        hasher.update(footer_bytes);
        let actual = hasher.digest();
        if actual != self.checksum {
            return Err(SegmentError::ChecksumMismatch {
                expected: self.checksum,
                actual,
            });
        }
        Ok(())
    }
}

/// One entry of the sealed footer's block index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInfo {
    /// Absolute file offset of the block's 8-byte length prefix.
    pub offset: u64,
    /// Compressed zstd frame length, excluding the length prefix.
    pub compressed_size: u32,
    /// Uncompressed columnar body size.
    pub uncompressed_size: u32,
    /// Events stored in this block.
    pub event_count: u32,
    /// Lowest sequence number in the block.
    pub min_seq: u64,
    /// Highest sequence number in the block.
    pub max_seq: u64,
    /// Unix microseconds.
    pub min_witnessed_at: i64,
    /// Unix microseconds.
    pub max_witnessed_at: i64,
}

impl BlockInfo {
    /// Decode one 52-byte entry.
    fn decode(buf: &[u8]) -> Self {
        Self {
            offset: read_u64(buf, 0),
            compressed_size: read_u32(buf, 8),
            uncompressed_size: read_u32(buf, 12),
            event_count: read_u32(buf, 16),
            min_seq: read_u64(buf, 20),
            max_seq: read_u64(buf, 28),
            min_witnessed_at: read_i64(buf, 36),
            max_witnessed_at: read_i64(buf, 44),
        }
    }
}

/// Decode the sealed footer's block index.
///
/// `footer_bytes` starts at `header.block_index_offset`; the entry count is
/// `header.block_count`.
pub fn decode_block_index(
    header: &SegmentHeader,
    footer_bytes: &[u8],
    file_len: usize,
) -> Result<Vec<BlockInfo>, SegmentError> {
    if header.is_active() {
        return Err(SegmentError::InvalidOffset);
    }
    let want = (header.block_count as usize)
        .checked_mul(BLOCK_INDEX_ENTRY_LEN)
        .ok_or(SegmentError::InvalidOffset)?;
    if footer_bytes.len() < want {
        return Err(SegmentError::Truncated);
    }
    let footer_offset = header.block_index_offset;
    let index_end = (footer_offset as usize)
        .checked_add(want)
        .ok_or(SegmentError::InvalidOffset)?;
    // The block index occupies [block_index_offset, did_bloom_offset);
    // an index claiming to extend into the bloom section is corrupt.
    if index_end > header.did_bloom_offset as usize {
        return Err(SegmentError::InvalidOffset);
    }
    let mut prev_end = None;
    let mut entries = Vec::with_capacity(header.block_count as usize);
    for i in 0..header.block_count as usize {
        let info = BlockInfo::decode(&footer_bytes[i * BLOCK_INDEX_ENTRY_LEN..]);
        let end = (info.offset as usize)
            .checked_add(info.compressed_size as usize)
            .and_then(|e| e.checked_add(8))
            .ok_or(SegmentError::InvalidOffset)?;
        // Blocks live between the header and the footer, in order, without
        // overlap.
        if info.offset < HEADER_LEN as u64
            || end > file_len
            || end > footer_offset as usize
            || prev_end.is_some_and(|p| info.offset < p)
        {
            return Err(SegmentError::InvalidOffset);
        }
        prev_end = Some(end as u64);
        entries.push(info);
    }
    Ok(entries)
}

/// Decompress one zstd frame with the output bounded at
/// [`MAX_BLOCK_UNCOMPRESSED`]. No zip bombs allowed.
///
/// Reading one byte past the limit detects oversize without buffering
/// it.
#[cfg(feature = "zstd")]
pub fn decompress_bounded(frame: &[u8]) -> Result<Vec<u8>, SegmentError> {
    let mut out = Vec::new();
    let mut decoder = zstd::Decoder::new(frame).map_err(|_| SegmentError::Decompress)?;
    let mut limited = std::io::Read::take(&mut decoder, MAX_BLOCK_UNCOMPRESSED as u64 + 1);
    std::io::Read::read_to_end(&mut limited, &mut out).map_err(|_| SegmentError::Decompress)?;
    drop(limited);
    let _ = decoder.finish();
    if out.len() > MAX_BLOCK_UNCOMPRESSED {
        return Err(SegmentError::Decompress);
    }
    Ok(out)
}

/// Decompress one block frame: `[u64 compressed_len][zstd frame]` at `pos`.
///
/// Returns the decompressed columnar body and the offset just past the
/// block. `Ok(None)` is the clean end of file. On an active segment
/// (`active_tail = true`) a truncated tail (the writer's normal state
/// while sealing) also counts as clean. On a sealed segment, that's data corruption, baby.
///
/// `.jss` block frames are always zstd (upstream writer has no uncompressed
/// mode), so this is feature-gated; header/index parsing works without it.
#[cfg(feature = "zstd")]
pub fn read_block_frame(
    file: &[u8],
    pos: usize,
    active_tail: bool,
) -> Result<Option<(Vec<u8>, usize)>, SegmentError> {
    if pos == file.len() {
        return Ok(None);
    }
    let start = pos.checked_add(8).ok_or(SegmentError::InvalidOffset)?;
    if start > file.len() {
        return tail_result(active_tail);
    }
    let compressed_len = read_u64(file, pos) as usize;
    let end = start
        .checked_add(compressed_len)
        .ok_or(SegmentError::InvalidOffset)?;
    if end > file.len() {
        return tail_result(active_tail);
    }
    Ok(Some((decompress_bounded(&file[start..end])?, end)))
}

#[cfg(feature = "zstd")]
fn tail_result(active_tail: bool) -> Result<Option<(Vec<u8>, usize)>, SegmentError> {
    if active_tail {
        Ok(None)
    } else {
        Err(SegmentError::Truncated)
    }
}

// Columnar event-body decoding: zstd-decompressed block payload to rows.
//
// Layout: `u32 count`, then nine fixed-size columns, then five concatenated variable-length regions
// (collections, dids, rkeys, revs, payloads).
//
// Regions are columnar, not row-major, so their boundaries depend on all
// length columns. The decoder totals those columns before slicing the five
// regions, then assembles rows from the column slices.

use crate::bos::{Bos, BosStr, FromBosSlice};

/// Firehose event kind discriminator from the on-disk `kind` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A new record was created.
    Create,
    /// An existing record was updated.
    Update,
    /// A record was deleted (no payload).
    Delete,
    /// Handle or DID-document change.
    Identity,
    /// Account status change (active/deactivated/deleted/...).
    Account,
    /// Broken commit chain; consumers should resync the repo.
    Sync,
    /// Create semantics after a resync gap; commit-shaped on the wire.
    CreateResync,
}

impl TryFrom<u8> for Kind {
    type Error = SegmentError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Ok(match v {
            1 => Self::Create,
            2 => Self::Update,
            3 => Self::Delete,
            4 => Self::Identity,
            5 => Self::Account,
            6 => Self::Sync,
            7 => Self::CreateResync,
            other => return Err(SegmentError::UnknownKind(other)),
        })
    }
}

/// One decoded event row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<S: BosStr, B: Bos<[u8]>> {
    /// Jetstream's monotonic sequence number; the stream cursor.
    pub seq: u64,
    /// Unix microseconds when the instance first saw the event.
    pub witnessed_at: i64,
    /// Unix microseconds of the operator timestamp import; 0 = unset.
    /// Display time falls back to [`Self::witnessed_at`].
    pub indexed_at: i64,
    /// Firehose event kind discriminator.
    pub kind: Kind,
    /// Collection NSID (commit kinds; empty otherwise).
    pub collection: S,
    /// Repository DID.
    pub did: S,
    /// Record key (commit kinds; empty otherwise).
    pub rkey: S,
    /// Repo revision (commit kinds; empty otherwise).
    pub rev: S,
    /// Raw DAG-CBOR record/event bytes (create/update/CreateResync carry
    /// the record; identity/account/sync carry the upstream event).
    pub payload: B,
}

impl<S: BosStr, B: Bos<[u8]>> Row<S, B> {
    /// The timestamp Jetstream shows subscribers (`time_us`): the imported
    /// `indexed_at` when set, otherwise `witnessed_at`.
    pub fn display_time_us(&self) -> i64 {
        if self.indexed_at != 0 {
            self.indexed_at
        } else {
            self.witnessed_at
        }
    }
}

/// Decode a decompressed columnar body into rows with the caller's chosen
/// backing types.
pub fn decode_block_body<'i, S, B>(body: &'i [u8]) -> Result<Vec<Row<S, B>>, SegmentError>
where
    S: BosStr + From<&'i str>,
    B: Bos<[u8]> + FromBosSlice<'i>,
{
    if body.len() < 4 {
        return Err(SegmentError::Truncated);
    }
    let n = u32::from_le_bytes(body[0..4].try_into().expect("4-byte slice")) as usize;

    // Column slices are taken left to right; widths use checked math so a
    // hostile event count errors instead of wrapping (wrapping on 32-bit
    // targets would yield short slices and later index panics).
    let mut pos: usize = 4;
    let mut take = |count: usize, width: usize| -> Result<&[u8], SegmentError> {
        let total = count
            .checked_mul(width)
            .ok_or(SegmentError::InvalidOffset)?;
        let end = pos.checked_add(total).ok_or(SegmentError::InvalidOffset)?;
        if end > body.len() {
            return Err(SegmentError::Truncated);
        }
        let slice = &body[pos..end];
        pos = end;
        Ok(slice)
    };

    let seq_col = take(n, 8)?;
    let witnessed_col = take(n, 8)?;
    let indexed_col = take(n, 8)?;
    let kind_col = take(n, 1)?;
    let collection_len_col = take(n, 1)?;
    let did_len_col = take(n, 2)?;
    let rkey_len_col = take(n, 1)?;
    let rev_len_col = take(n, 1)?;
    let payload_len_col = take(n, 4)?;

    // Region boundaries depend on every row's lengths, so the five length
    // columns are summed before any region bytes can be claimed.
    fn le16(c: &[u8]) -> Result<u16, SegmentError> {
        let a: [u8; 2] = c
            .try_into()
            .map_err(|_| SegmentError::MalformedBody("short u16 column chunk"))?;
        Ok(u16::from_le_bytes(a))
    }
    fn le32(c: &[u8]) -> Result<u32, SegmentError> {
        let a: [u8; 4] = c
            .try_into()
            .map_err(|_| SegmentError::MalformedBody("short u32 column chunk"))?;
        Ok(u32::from_le_bytes(a))
    }
    fn le64(c: &[u8]) -> Result<u64, SegmentError> {
        let a: [u8; 8] = c
            .try_into()
            .map_err(|_| SegmentError::MalformedBody("short u64 column chunk"))?;
        Ok(u64::from_le_bytes(a))
    }

    let mut collections_total: usize = 0;
    let mut dids_total: usize = 0;
    let mut rkeys_total: usize = 0;
    let mut revs_total: usize = 0;
    let mut payloads_total: usize = 0;
    let mut did_lens = did_len_col.chunks_exact(2);
    let mut payload_lens = payload_len_col.chunks_exact(4);
    for ((&c_len, &r_len), &v_len) in collection_len_col.iter().zip(rkey_len_col).zip(rev_len_col) {
        collections_total = collections_total
            .checked_add(c_len as usize)
            .ok_or(SegmentError::InvalidOffset)?;
        dids_total = dids_total
            .checked_add(le16(did_lens.next().ok_or(SegmentError::Truncated)?)? as usize)
            .ok_or(SegmentError::InvalidOffset)?;
        rkeys_total = rkeys_total
            .checked_add(r_len as usize)
            .ok_or(SegmentError::InvalidOffset)?;
        revs_total = revs_total
            .checked_add(v_len as usize)
            .ok_or(SegmentError::InvalidOffset)?;
        payloads_total = payloads_total
            .checked_add(le32(payload_lens.next().ok_or(SegmentError::Truncated)?)? as usize)
            .ok_or(SegmentError::InvalidOffset)?;
    }

    let mut take_region = |total: usize| -> Result<&'i [u8], SegmentError> {
        let end = pos.checked_add(total).ok_or(SegmentError::InvalidOffset)?;
        if end > body.len() {
            return Err(SegmentError::Truncated);
        }
        let slice = &body[pos..end];
        pos = end;
        Ok(slice)
    };

    let collections = take_region(collections_total)?;
    let dids = take_region(dids_total)?;
    let rkeys = take_region(rkeys_total)?;
    let revs = take_region(revs_total)?;
    let payloads = take_region(payloads_total)?;

    if pos != body.len() {
        return Err(SegmentError::MalformedBody(
            "trailing bytes after payload region",
        ));
    }

    fn utf8(head: &[u8]) -> Result<&str, SegmentError> {
        core::str::from_utf8(head)
            .map_err(|_| SegmentError::MalformedBody("string region is not UTF-8"))
    }

    // Takes `len` bytes off the front of `rest`; errors
    // when the region is shorter than the length column claims.
    fn take_head<'r>(rest: &mut &'r [u8], len: usize) -> Result<&'r [u8], SegmentError> {
        if rest.len() < len {
            return Err(SegmentError::MalformedBody(
                "length column exceeds its region",
            ));
        }
        let (head, tail) = rest.split_at(len);
        *rest = tail;
        Ok(head)
    }

    let mut c_rest = collections;
    let mut d_rest = dids;
    let mut r_rest = rkeys;
    let mut v_rest = revs;
    let mut p_rest = payloads;

    let mut rows = Vec::with_capacity(n);
    let mut seqs = seq_col.chunks_exact(8);
    let mut witnessed_vals = witnessed_col.chunks_exact(8);
    let mut indexed_vals = indexed_col.chunks_exact(8);
    let mut kinds = kind_col.iter();
    let mut collection_lens = collection_len_col.iter();
    let mut did_lens = did_len_col.chunks_exact(2);
    let mut rkey_lens = rkey_len_col.iter();
    let mut rev_lens = rev_len_col.iter();
    let mut payload_lens = payload_len_col.chunks_exact(4);

    for _ in 0..n {
        let seq = le64(seqs.next().ok_or(SegmentError::Truncated)?)?;
        let witnessed = le64(witnessed_vals.next().ok_or(SegmentError::Truncated)?)? as i64;
        let indexed = le64(indexed_vals.next().ok_or(SegmentError::Truncated)?)? as i64;
        let kind = Kind::try_from(*kinds.next().ok_or(SegmentError::Truncated)?)?;

        let c_len = *collection_lens.next().ok_or(SegmentError::Truncated)? as usize;
        let collection = S::from(utf8(take_head(&mut c_rest, c_len)?)?);

        let d_len = le16(did_lens.next().ok_or(SegmentError::Truncated)?)? as usize;
        let did = S::from(utf8(take_head(&mut d_rest, d_len)?)?);

        let r_len = *rkey_lens.next().ok_or(SegmentError::Truncated)? as usize;
        let rkey = S::from(utf8(take_head(&mut r_rest, r_len)?)?);

        let v_len = *rev_lens.next().ok_or(SegmentError::Truncated)? as usize;
        let rev = S::from(utf8(take_head(&mut v_rest, v_len)?)?);

        let p_len = le32(payload_lens.next().ok_or(SegmentError::Truncated)?)? as usize;
        let payload = B::from_bos_slice(take_head(&mut p_rest, p_len)?);

        rows.push(Row {
            seq,
            witnessed_at: witnessed,
            indexed_at: indexed,
            kind,
            collection,
            did,
            rkey,
            rev,
            payload,
        });
    }

    Ok(rows)
}

#[cfg(all(test, feature = "zstd"))]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/jss/testdata/segments.jss"
        ))
        .expect("fixture present")
    }

    fn decode_borrowed(body: &[u8]) -> Result<Vec<Row<&str, &[u8]>>, SegmentError> {
        decode_block_body(body)
    }

    #[test]
    fn decode_fixed_header_fixture() {
        let file = fixture();
        let header = SegmentHeader::decode(&file, file.len()).expect("valid fixture");

        assert_eq!(header.version, 1);
        assert_eq!(header.block_count, 1);
        assert_eq!(header.event_count, 40);
        assert_eq!(header.unique_did_count, 5);
        assert_eq!(header.min_seq, 1);
        assert_eq!(header.max_seq, 40);
        assert_eq!(header.footer_offset, 1069);
        assert_eq!(header.block_index_offset, header.footer_offset);
        // Sealed: nonzero checksum, verified against the real server bytes.
        assert!(!header.is_active());
        header
            .verify_checksum(&file, &file[header.block_index_offset as usize..])
            .expect("upstream-computed checksum must verify");
    }

    #[test]
    fn decode_fixed_block_fixture() {
        let file = fixture();
        let header = SegmentHeader::decode(&file, file.len()).unwrap();
        let footer = &file[header.block_index_offset as usize..];
        let index = decode_block_index(&header, footer, file.len()).expect("index");
        assert_eq!(index.len(), 1);

        let (body, end) = read_block_frame(&file, 256, false)
            .expect("frame")
            .expect("present");
        // The block ends at footer_offset; the rest is footer metadata.
        assert_eq!(end, header.footer_offset as usize);
        let rows = decode_borrowed(&body).expect("rows");
        assert_eq!(rows.len(), 40);
        assert_eq!(rows[0].seq, 1);
        assert_eq!(rows[39].seq, 40);
        assert!(rows.iter().all(|r| r.kind == Kind::Identity));
        assert!(
            rows.iter()
                .all(|r| r.collection.is_empty() && r.rkey.is_empty())
        );
        assert!(rows.iter().all(|r| r.did.starts_with("did:plc:")));
        assert!(!rows[0].payload.is_empty());
    }

    #[test]
    fn decode_block_index_fixture() {
        let file = fixture();
        let header = SegmentHeader::decode(&file, file.len()).unwrap();
        let footer_start = header.block_index_offset as usize;
        let entries =
            decode_block_index(&header, &file[footer_start..], file.len()).expect("entries");

        assert_eq!(entries.len(), header.block_count as usize);
        let e = &entries[0];
        assert_eq!(e.offset, 256);
        assert_eq!(e.compressed_size, 805);
        assert_eq!(e.uncompressed_size, 5937);
        assert_eq!(e.event_count, 40);
        assert_eq!(e.min_seq, 1);
        assert_eq!(e.max_seq, 40);
        assert!(e.max_witnessed_at >= e.min_witnessed_at);
        assert!(footer_start + entries.len() * BLOCK_INDEX_ENTRY_LEN < file.len());
    }

    #[test]
    fn decode_frame_only_getblock_payload() {
        let file = fixture();
        let frame = &file[256 + 8..256 + 8 + 805];
        let mut out = Vec::new();
        zstd::stream::copy_decode(frame, &mut out).expect("decompress");
        let rows = decode_borrowed(&out).expect("rows");
        assert_eq!(rows.len(), 40);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut file = fixture();
        file[0] = b'X';
        assert_eq!(
            SegmentHeader::decode(&file, file.len()),
            Err(SegmentError::BadMagic)
        );
    }

    #[test]
    fn rejects_wrong_version() {
        let mut file = fixture();
        file[12] = 2;
        match SegmentHeader::decode(&file, file.len()) {
            Err(SegmentError::WrongVersion(2)) => {}
            other => panic!("expected WrongVersion(2), got {other:?}"),
        }
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let mut file = fixture();
        file[200] ^= 0xFF;
        let header = SegmentHeader::decode(&file, file.len()).unwrap();
        assert!(matches!(
            header.verify_checksum(&file, &file[header.block_index_offset as usize..]),
            Err(SegmentError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn rejects_truncated_columns() {
        let body = {
            let mut b = vec![0u8; 8];
            b[0..4].copy_from_slice(&100u32.to_le_bytes());
            b
        };
        assert_eq!(decode_borrowed(&body), Err(SegmentError::Truncated));
    }

    #[test]
    fn rejects_count_overflow() {
        let mut body = Vec::new();
        body.extend_from_slice(&u32::MAX.to_le_bytes());
        body.extend_from_slice(&[0u8; 24]);
        body.push(1);
        body.push(1);
        body.extend_from_slice(&[0u8; 2]);
        body.push(1);
        body.push(1);
        body.extend_from_slice(&[0u8; 4]);
        match decode_borrowed(&body) {
            Err(SegmentError::Truncated) | Err(SegmentError::InvalidOffset) => {}
            other => panic!("expected overflow-safe error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_length_overflow() {
        let body = {
            let mut b = Vec::new();
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&u64::MAX.to_le_bytes());
            b.extend_from_slice(&0i64.to_le_bytes());
            b.extend_from_slice(&0i64.to_le_bytes());
            b.push(4);
            b.push(0);
            b.extend_from_slice(&(u16::MAX).to_le_bytes());
            b.push(0);
            b.push(0);
            b.extend_from_slice(&0u32.to_le_bytes());
            b
        };
        assert_eq!(decode_borrowed(&body), Err(SegmentError::Truncated));
    }

    #[test]
    fn rejects_unknown_kind() {
        let body = {
            let mut b = Vec::new();
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&1u64.to_le_bytes());
            b.extend_from_slice(&0i64.to_le_bytes());
            b.extend_from_slice(&0i64.to_le_bytes());
            b.push(99);
            b.push(0);
            b.extend_from_slice(&0u16.to_le_bytes());
            b.push(0);
            b.push(0);
            b.extend_from_slice(&0u32.to_le_bytes());
            b
        };
        assert_eq!(decode_borrowed(&body), Err(SegmentError::UnknownKind(99)));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut body = {
            let mut b = Vec::new();
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&1u64.to_le_bytes());
            b.extend_from_slice(&0i64.to_le_bytes());
            b.extend_from_slice(&0i64.to_le_bytes());
            b.push(3);
            b.push(0);
            b.extend_from_slice(&0u16.to_le_bytes());
            b.push(0);
            b.push(0);
            b.extend_from_slice(&0u32.to_le_bytes());
            b
        };
        body.push(0xAB);
        assert_eq!(
            decode_borrowed(&body),
            Err(SegmentError::MalformedBody(
                "trailing bytes after payload region"
            ))
        );
    }

    #[test]
    fn rejects_invalid_block_index_offset() {
        let mut file = fixture();
        let bogus = (file.len() as u64) * 4;
        file[90..98].copy_from_slice(&bogus.to_le_bytes());
        match SegmentHeader::decode(&file, file.len()) {
            Ok(header) => {
                assert!(matches!(
                    decode_block_index(
                        &header,
                        &file[header.block_index_offset as usize..],
                        file.len()
                    ),
                    Err(SegmentError::InvalidOffset)
                ));
            }
            Err(SegmentError::InvalidOffset) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn accepts_unsealed_active_segment() {
        let mut file = fixture();
        file[0..HEADER_LEN].fill(0);
        file[0..4].copy_from_slice(b"jss0");
        file[12..14].copy_from_slice(&1u16.to_le_bytes());

        let header = SegmentHeader::decode(&file, file.len()).expect("active header");
        assert!(header.is_active());
        assert_eq!(header.block_count, 0);
        assert_eq!(
            decode_block_index(
                &SegmentHeader {
                    checksum: 0,
                    ..header
                },
                &[],
                file.len()
            ),
            Err(SegmentError::InvalidOffset)
        );
    }

    #[test]
    fn zero_event_block_decodes() {
        let rows = decode_borrowed(&[0u8; 4]).expect("zero-event body");
        assert!(rows.is_empty());
    }

    fn patch_header_u64(file: &mut [u8], offset: usize, value: u64) {
        file[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_footer_block_index_mismatch() {
        let mut file = fixture();
        patch_header_u64(&mut file, 90, 9999);
        assert_eq!(
            SegmentHeader::decode(&file, file.len()).unwrap_err(),
            SegmentError::InvalidOffset
        );
    }

    #[test]
    fn rejects_overlapping_section_layout() {
        let mut file = fixture();
        patch_header_u64(&mut file, 66, 1000);
        assert_eq!(
            SegmentHeader::decode(&file, file.len()).unwrap_err(),
            SegmentError::InvalidOffset
        );
    }

    #[test]
    fn rejects_offset_past_eof() {
        let mut file = fixture();
        let past_eof = file.len() as u64 + 1;
        patch_header_u64(&mut file, 82, past_eof);
        assert_eq!(
            SegmentHeader::decode(&file, file.len()).unwrap_err(),
            SegmentError::InvalidOffset
        );
    }

    #[test]
    fn rejects_block_index_entry_past_footer() {
        let file = fixture();
        let header = SegmentHeader::decode(&file, file.len()).expect("valid fixture");
        let mut footer = file[header.block_index_offset as usize..].to_vec();
        footer[0..8].copy_from_slice(&1000u64.to_le_bytes());
        footer[8..12].copy_from_slice(&(header.block_index_offset as u32 + 1).to_le_bytes());
        assert_eq!(
            decode_block_index(&header, &footer, file.len()).unwrap_err(),
            SegmentError::InvalidOffset
        );
    }

    #[test]
    fn rejects_unordered_block_index_entries() {
        let file = fixture();
        let header = SegmentHeader::decode(&file, file.len()).expect("valid fixture");
        assert_eq!(header.block_count, 1);
        let mut footer = file[header.block_index_offset as usize..].to_vec();
        footer[0..8].copy_from_slice(&0u64.to_le_bytes());
        footer[8..12].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(
            decode_block_index(&header, &footer, file.len()).unwrap_err(),
            SegmentError::InvalidOffset
        );
    }
}
