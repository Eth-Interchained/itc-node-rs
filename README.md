# itc-node-rs — ITC-L2 EVM Sidechain

> **Proof of Sovereignty.** An EVM sidechain anchored to ITC mainnet,
> with NEDB-backed verifiable state and a federated bridge for native ITC.
>
> © Interchained LLC × Claude Sonnet 4.6 — MIT License

---

## What it is

**ITC-L2** is a sovereign EVM sidechain on top of ITC mainnet.

- Run any Solidity contract. Deploy ERC-20s, ERC-721s, DeFi protocols.
- Every state transition is stored in **NEDB** — content-addressed, tamper-evident, AS OF time-travel queryable, causally traceable.
- Anchored to **ITC L1** every 100 blocks via `OP_RETURN` (NEDB Merkle head).
- Bridged via a **federated sentinel bridge** — ITC locked on L1 mints ITC on L2, 1:1. Zero ITC at genesis.

---

## Status

| Slice | What | Status |
|-------|------|--------|
| 1 | Workspace + ITC protocol constants | ✅ merged |
| 2 | Real ITC P2P protocol + anchor handshake | ✅ merged |
| 3 | Forward header sync + seeding server | ✅ merged |
| 4 | NEDB-backed header/block/tip persistence | ✅ merged |
| 5 | EVM execution engine (revm + NEDB state) | 🔜 next |
| 6 | ITC ERC-20 + sentinel bridge daemon | 🔜 |
| 7 | L1 anchor OP_RETURN poster | 🔜 |
| 8 | eth_* JSON-RPC server | 🔜 |

---

## Quick Start

```bash
# Build
cargo build --release

# Run (connects to ITC mainnet anchor, syncs headers)
ITC_NODE_DATADIR=./data ./target/release/itc-node [LISTEN_PORT]
```

Requirements: Rust 1.75+, internet access to `seed.interchained.org:17101`.

---

## Architecture

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full system design:
chain parameters, EVM state model, NEDB provenance properties, L1 anchor flow.

See [`docs/BRIDGE.md`](docs/BRIDGE.md) for the federated bridge spec:
sentinel architecture, deposit/withdrawal flows, fee model, ITC ERC-20 spec,
security model and v2 upgrade path.

---

## Key Properties

**ITC starts at zero.** No pre-mint. The only way ITC enters circulation is
via the bridge: lock ITC on L1, receive ITC on L2. Supply on L2 always equals
ITC locked in the bridge address on L1.

**NEDB state = auditable EVM.** Every account balance and storage slot write
carries `caused_by: [tx_hash]` — the full causal chain from any state value
back to the transaction that produced it. AS OF queries replay any historical
state. NEDB's Merkle head IS the state root posted to ITC L1.

**Instant boot.** The node resumes from persisted headers in NEDB on startup —
no full rescan. Same instant-boot trust-anchor architecture from the header relay
slices, now extended to EVM state.

---

## Crate Layout

```
itc-node-rs/
├── crates/
│   ├── itc-proto/        # ITC P2P wire protocol, consensus, hashes
│   ├── itc-node/         # Node binary: anchor, sync, serve, NEDB store
│   ├── itc-evm/          # (slice 5) revm + NedbState EVM backend    [coming]
│   └── itc-sentinel/     # (slice 6) federated bridge sentinel daemon [coming]
└── docs/
    ├── ARCHITECTURE.md
    └── BRIDGE.md
```

---

## License

MIT — see [LICENSE](LICENSE).
