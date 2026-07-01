//! Real storage backend over nedb-engine — the v2 content-addressed DAG engine,
//! already Rust, used directly (no FFI). Persists headers, blocks, and L2 receipts
//! into NEDB collections so the node resumes instantly on the next boot.
//!
//! - `headers` collection: id = block-hash hex, data = {hdr: <80-byte hex>, height},
//!   `caused_by = [parent hash]` — the header is a DAG node caused by its parent.
//! - `blocks`  collection: id = block-hash hex, data = {raw: <block hex>}.
//!
//! Boot resume (v2.5.44+): the chain tip is NOT a synthetic marker document — it is
//! read directly from the engine's own durable per-collection tip,
//! `db.tip_collection("headers")`. That primitive is kept current on every write and
//! survives a warm restart with no scan (persisted in MANIFEST) — see
//! <https://github.com/Eth-Interchained/nedb/blob/master/docs/REPLICATION.md>.
//! Because a header document's id IS `to_internal_hex(header.block_hash())`, the tip
//! hash is the node's id directly; no header bytes need decoding to resume.
//!
//! Durability: `flush()` wraps the engine's `flush_all()` (WAL + segment sync +
//! MANIFEST, including the tip). Callers checkpoint on their own cadence — this
//! module is a persistence primitive, not a policy: it does not decide *when* to
//! flush, only *how*. See `sync::sync_headers`/`sync_blocks` (L1, every ~2000) and
//! `sequencer::produce_block` (L2, every 500), plus the exit handler in `main.rs`.

use std::io;
use std::path::Path;
use std::sync::Arc;

use nedb_engine::Db;
use serde_json::json;

use itc_proto::block::BlockHeader;
use itc_proto::consensus::Reader;
use itc_proto::hashes::to_internal_hex;

pub const COLL_HEADERS: &str = "headers";
const COLL_BLOCKS: &str = "blocks";

type PutOp = (String, String, serde_json::Value, Vec<String>, Option<String>, Option<String>);

pub struct Store {
    pub db: Arc<Db>,
}

fn err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

impl Store {
    /// Wrap an already-open NEDB instance (e.g. for background threads).
    pub fn from_arc_db(db: Arc<Db>) -> Store {
        Store { db }
    }

    /// Open (or create) the NEDB-backed store at `path`.
    ///
    /// On cold start (no MANIFEST on disk), NEDB rebuilds the index from the WAL
    /// in a background thread. `head()` returns empty until the scan completes.
    /// We wait here so that `tip_header()` and other reads see the full indexed state.
    pub fn open(path: &str) -> io::Result<Store> {
        let db = Db::open(Path::new(path), None).map_err(err)?;
        let db = Arc::new(db);
        Db::start_cold_scan(Arc::clone(&db));
        // Wait for the cold scan to complete (indicated by a non-empty head).
        // On warm start the head is immediately available; on cold start we wait.
        // Timeout: 300s (5 minutes) for very large databases.
        let store = Store { db };
        if store.head().is_empty() {
            println!("itc-node[store]: cold start — waiting for NEDB scan to complete...");
            for _ in 0..30_000u32 { // 300s at 10ms intervals
                std::thread::sleep(std::time::Duration::from_millis(10));
                if !store.head().is_empty() { break; }
            }
            println!("itc-node[store]: NEDB scan complete (head={})", &store.head()[..16.min(store.head().len())]);
        }
        Ok(store)
    }

    /// The engine's tamper-evident Merkle head (for logging / proofs).
    pub fn head(&self) -> String {
        self.db.head()
    }

    /// Make buffered writes durable now (id-index WAL + segment sync + MANIFEST,
    /// including the tip). Callers decide the cadence — see module docs.
    pub fn flush(&self) {
        self.db.flush_all();
    }

    /// Persist a single header, linked causally to its parent.
    #[allow(dead_code)]
    pub fn put_header(&self, header: &BlockHeader, height: i32) -> io::Result<()> {
        let id = to_internal_hex(&header.block_hash());
        let parent = to_internal_hex(&header.prev_blockhash);
        let data = json!({ "hdr": hex_encode(&header.encode()), "height": height });
        self.db
            .put(COLL_HEADERS, &id, data, vec![parent], None, None)
            .map(|_| ())
            .map_err(err)
    }

    /// Persist a batch of headers in one engine call (parallel, monotonic seq).
    pub fn put_headers_batch(&self, items: &[(BlockHeader, i32)]) -> io::Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let ops: Vec<PutOp> = items
            .iter()
            .map(|(h, height)| {
                let id = to_internal_hex(&h.block_hash());
                let parent = to_internal_hex(&h.prev_blockhash);
                let data = json!({ "hdr": hex_encode(&h.encode()), "height": *height });
                (COLL_HEADERS.to_string(), id, data, vec![parent], None, None)
            })
            .collect();
        self.db.put_batch(ops).map(|_| ()).map_err(err)
    }

    /// Load a header (and its height) by block-hash hex id.
    #[allow(dead_code)]
    pub fn get_header(&self, id: &str) -> Option<(BlockHeader, i32)> {
        let node = self.db.get(COLL_HEADERS, id)?;
        let hdr_hex = node.data.get("hdr")?.as_str()?;
        let height = node.data.get("height")?.as_i64()? as i32;
        let bytes = hex_decode(hdr_hex)?;
        let mut r = Reader::new(&bytes);
        let header = BlockHeader::decode(&mut r).ok()?;
        Some((header, height))
    }

    /// Persist a full block body (raw consensus bytes).
    pub fn put_block(&self, block_hash_hex: &str, raw: &[u8]) -> io::Result<()> {
        let data = json!({ "raw": hex_encode(raw) });
        self.db
            .put(COLL_BLOCKS, block_hash_hex, data, vec![], None, None)
            .map(|_| ())
            .map_err(err)
    }

    /// Load a full block body's raw bytes by block-hash hex id.
    pub fn get_block(&self, id: &str) -> Option<Vec<u8>> {
        let node = self.db.get(COLL_BLOCKS, id)?;
        hex_decode(node.data.get("raw")?.as_str()?)
    }

    /// Return true if we already have this block body persisted.
    pub fn has_block(&self, id: &str) -> bool {
        self.db.get(COLL_BLOCKS, id).is_some()
    }

    /// The chain tip — (height, block hash) of the most recently connected header —
    /// or `None` if nothing has been synced yet. The durable boot-resume primitive:
    /// backed by `db.tip_collection("headers")`, which is kept current on every
    /// header write and survives a warm restart with no scan (see module docs).
    ///
    /// No header bytes need decoding: a header document's id IS
    /// `to_internal_hex(header.block_hash())`, so the tip hash is the node's id
    /// directly. `height` is read straight from the stored `{hdr, height}` payload.
    pub fn tip_header(&self) -> Option<(i32, [u8; 32])> {
        let node = self.db.tip_collection(COLL_HEADERS)?;
        let height = node.data.get("height")?.as_i64()? as i32;
        let bytes = hex_decode(&node.id)?;
        if bytes.len() != 32 {
            return None;
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Some((height, hash))
    }
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        out.push((hexval(bytes[i])? << 4) | hexval(bytes[i + 1])?);
        i += 2;
    }
    Some(out)
}

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
