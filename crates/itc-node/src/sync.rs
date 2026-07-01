//! Sync — header sync and full block body download from the anchor peer.
//!
//! Durability cadence (v2.5.44+): flush on natural batch boundaries, not per-put.
//! `store.flush()` is a real `flush_all()` (WAL + segment sync + MANIFEST) — cheap
//! enough to call every round/batch, not so cheap it belongs on the hot per-header
//! or per-block path. Headers checkpoint every round (`getheaders` returns up to
//! 2000 at a time); blocks checkpoint every ~2000 downloaded. On a hard exit
//! (Ctrl+C / SIGTERM) `main.rs`'s handler calls `store.flush()` directly — no
//! shadow "last known tip" state needs tracking here anymore: `store.tip_header()`
//! (backed by `nedb_engine::Db::tip_collection`) always reflects the real
//! last-connected header, kept current synchronously on every write.

use std::io;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use itc_proto::block::BlockHeader;
use itc_proto::hashes::to_internal_hex;

use crate::chain::{ConnectOutcome, HeaderChain};
use crate::p2p::Peer;
use crate::store::Store;

/// Batch size for block body downloads. 16 is conservative — peers typically
/// allow up to 128 outstanding block requests, but we start small.
const BLOCK_BATCH: i32 = 16;

/// L1 block-download checkpoint cadence — flush every N downloaded bodies.
const BLOCK_FLUSH_EVERY: u64 = 2000;

// ── Header sync ──────────────────────────────────────────────────────────────

/// Sync headers forward from `peer` into `chain`, persisting into `store`.
/// Checkpoints (flushes) at the end of every round — `getheaders` returns up to
/// 2000 headers per round, so this is a natural ~2000-header cadence, not a
/// per-put flush.
pub fn sync_headers(
    peer: &mut Peer,
    chain: &mut HeaderChain,
    store: &Store,
    shutdown: &AtomicBool,
) -> io::Result<()> {
    let target = peer.peer_height;
    let mut rounds = 0u32;
    loop {
        if shutdown.load(Ordering::Relaxed) { return Ok(()); }
        rounds += 1;
        let locator = chain.block_locator();
        let batch = peer.get_headers(locator)?;
        if batch.is_empty() {
            break;
        }
        let before = chain.tip_height();
        let mut to_persist: Vec<(BlockHeader, i32)> = Vec::new();
        for h in batch.iter() {
            match chain.connect(h.clone()) {
                ConnectOutcome::Extended(height) => to_persist.push((h.clone(), height)),
                ConnectOutcome::HeavierFork(ht) => {
                    println!(
                        "itc-node[sync]: heavier competing chain at height {ht} — Proof-of-Prefix MISMATCH flagged"
                    );
                }
                _ => {}
            }
        }
        store.put_headers_batch(&to_persist)?;
        // Checkpoint: durable tip resume no longer needs a manually-tracked shadow
        // copy — tip_header() reads the real last-connected header directly.
        store.flush();

        let after = chain.tip_height();
        // Single-line overwriting progress bar
        {
            let pct = if target > 0 { (after as f64 / target as f64 * 100.0) as u32 } else { 0 };
            let filled = ((pct as usize) * 20 / 100).min(20);
            let bar = "█".repeat(filled) + &"░".repeat(20 - filled);
            eprint!("\r  [headers] {after:>7}/{target} [{bar}] {pct:>3}%   ");
        }
        if after == before {
            break;
        }
        if batch.len() < 2000 {
            eprintln!(); // end progress bar line
            break;
        }
        if target > 0 && after >= target {
            eprintln!(); // end progress bar line
            break;
        }
        if rounds > 100_000 {
            eprintln!(); // end progress bar line
            break;
        }
    }
    Ok(())
}

// ── Block body download ───────────────────────────────────────────────────────

/// Download full block bodies for every block we have a header for but no body.
/// Walks the active chain from height 1 to tip, batching getdata requests.
/// Already-stored blocks are skipped (idempotent, resume-safe). Checkpoints
/// (flushes) every `BLOCK_FLUSH_EVERY` downloaded bodies, plus once more at the
/// end so a short final stretch isn't left unflushed.
///
/// Two real bugs fixed here (not just a missing progress bar):
///
/// 1. The old progress print gated on `downloaded % 100 == 0`. `downloaded`
///    advances in chunks of up to `BLOCK_BATCH` (16) per round, not by 1 —
///    and 16 does not divide evenly into 100 (their LCM is 400). The instant
///    any round downloads fewer than a full batch (which happens constantly:
///    resuming, or any already-owned block in range), the running total steps
///    PAST 100 instead of landing on it, and the print can go silent for the
///    rest of a multi-hundred-thousand-block run — even though downloading is
///    proceeding completely normally underneath. Fixed by printing every round
///    on a live line, never gated by an exact-count match.
///
/// 2. `Peer::request_blocks` can time out or the peer can simply not answer
///    for part of a range, and returns `Ok(partial_or_empty_vec)` — NOT an
///    error. The old code never checked `received` against `requested`, so a
///    peer that silently refuses to serve old historical blocks would let
///    this function "complete successfully" having downloaded nothing, with
///    zero indication anything was wrong. Now logged loudly per short batch,
///    with a final summary if any occurred.
///
/// Returns (downloaded, skipped) counts.
pub fn sync_blocks(peer: &mut Peer, chain: &HeaderChain, store: &Store, start_height: i32, shutdown: &AtomicBool) -> io::Result<(u64, u64)> {
    let tip = chain.tip_height();
    if tip < 1 {
        return Ok((0, 0));
    }
    let range_start = start_height.max(1);
    let total: u64 = (tip - range_start + 1).max(0) as u64;
    let mut downloaded = 0u64;
    let mut skipped = 0u64;
    let mut since_flush = 0u64;
    let mut short_batches = 0u64;
    let mut height = range_start;

    const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let cyan = "\x1b[36m";
    let bcyan = "\x1b[1;36m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let yellow = "\x1b[33m";
    let reset = "\x1b[0m";

    let start = Instant::now();
    let mut last_sample_t = start;
    let mut last_sample_n = 0u64;
    let mut rate = 0.0f64;
    let mut frame = 0usize;

    while height <= tip {
        if shutdown.load(Ordering::Relaxed) { break; }
        let batch_end = (height + BLOCK_BATCH - 1).min(tip);
        let hashes = chain.active_range(height, batch_end);

        // Filter to blocks we don't have yet.
        let needed: Vec<[u8; 32]> = hashes
            .iter()
            .filter(|h| !store.has_block(&to_internal_hex(*h)))
            .cloned()
            .collect();

        skipped += (hashes.len() - needed.len()) as u64;

        if !needed.is_empty() {
            let requested = needed.len();
            let blocks = peer.request_blocks(&needed)?;
            let received = blocks.len();
            for block in blocks {
                let hash_hex = to_internal_hex(&block.block_hash());
                store.put_block(&hash_hex, &block.raw)?;
                downloaded += 1;
                since_flush += 1;
            }
            if received < requested {
                short_batches += 1;
                eprintln!(
                    "\n  {yellow}⚠ itc-node[blocks]: peer returned {received}/{requested} for height {height}..{batch_end} \
                     — {} block(s) missing from this range and will remain a gap until re-synced{reset}",
                    requested - received
                );
            }
            if since_flush >= BLOCK_FLUSH_EVERY {
                store.flush();
                since_flush = 0;
            }
        }

        // Live progress — every round, unconditionally. Rate is sampled on a
        // wall-clock window (not a block-count window) so it stays honest even
        // when rounds are almost entirely skips (already-owned blocks).
        {
            let now = Instant::now();
            if now.duration_since(last_sample_t).as_millis() >= 500 {
                let dt = now.duration_since(last_sample_t).as_secs_f64();
                let dn = downloaded.saturating_sub(last_sample_n) as f64;
                if dt > 0.0 {
                    rate = dn / dt;
                }
                last_sample_t = now;
                last_sample_n = downloaded;
            }
            let done = (height - range_start).max(0) as u64;
            let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u32 } else { 100 };
            let elapsed = now.duration_since(start).as_secs_f64();
            let eta = if rate > 0.0 { total.saturating_sub(done) as f64 / rate } else { 0.0 };
            let filled = ((pct as usize) * 20 / 100).min(20);
            let bar = "█".repeat(filled) + &"░".repeat(20 - filled);
            let warn = if short_batches > 0 {
                format!("  {yellow}⚠ {short_batches} short batch(es){reset}")
            } else {
                String::new()
            };
            eprint!(
                "\r  {cyan}{spin}{reset} [blocks] {bcyan}[{bar}]{reset} {bold}{pct:>3}%{reset}  \
                 height {height:>7}/{tip}  {dim}({downloaded} dl · {rate:>6.0}/s · eta {eta:>4.0}s · {elapsed:>4.0}s elapsed){reset}{warn}   ",
                spin = SPINNER[frame % SPINNER.len()],
            );
            let _ = std::io::stderr().flush();
            frame += 1;
        }

        height = batch_end + 1;
    }
    eprintln!(); // end the live progress line
    if since_flush > 0 {
        store.flush(); // final partial stretch — don't leave it unflushed
    }
    if short_batches > 0 {
        eprintln!(
            "itc-node[blocks]: WARNING — {short_batches} batch(es) returned fewer blocks than \
             requested; some ranges may have gaps. Re-run sync to retry them."
        );
    }
    Ok((downloaded, skipped))
}
