# ITC-L2 Node — Storage Contract v1.0

**Network**: ITC-L2 (chain ID 17101)  
**Engine**: NEDB v2 (content-addressed, append-only, bi-temporal)  
**Locked**: 2026-06-29 — v1.0.0

---

## Directory Layout

```
$ITC_NODE_DATADIR/
├── MANIFEST           ← NEDB engine manifest (internal — never touch)
│
├── headers/           ← L1 IMMUTABLE: 648k+ ITC mainnet block headers
├── blocks/            ← L1 IMMUTABLE: full block bodies (raw consensus bytes)
├── index/             ← L1 IMMUTABLE: chain tip index {height, hash}
│
├── evm_accounts/      ← L2 DERIVED: aITC balances + nonces + code hashes
├── evm_storage/       ← L2 DERIVED: EVM contract storage slots
├── evm_code/          ← L2 DERIVED: deployed contract bytecode
├── l2_receipts/       ← L2 DERIVED: transaction receipts {gas, status, block}
│
├── oracle_minted/     ← L2 DERIVED: bridge deposit idempotency guards
├── oracle_pending/    ← L2 DERIVED: deposits awaiting L1 confirmation
└── oracle_state/      ← L2 DERIVED: oracle tip height + pending index
```

---

## Permanence Tiers

| Tier | Collections | Rule |
|------|-------------|------|
| **L1 Immutable** | `headers`, `blocks`, `index` | Never wipe. Re-downloading costs hours. These are the ground truth. |
| **L2 Derived** | `evm_accounts`, `evm_storage`, `evm_code`, `l2_receipts`, `oracle_minted`, `oracle_pending`, `oracle_state` | Fully reconstructable from L1 in minutes via `--replay`. |
| **Engine Internal** | `MANIFEST` | Managed by NEDB. Do not modify. |

---

## Replay Protocol

L2 derived state is **fully deterministic** from L1 block data.
To wipe and re-derive:

```bash
./itc-node --replay
# or
ITC_ORACLE_REPLAY=1 ./itc-node
```

This will:
1. Delete all L2 DERIVED collection directories
2. Preserve all L1 IMMUTABLE collections (no re-download needed)
3. Re-derive L2 state by scanning blocks from `ITC_ORACLE_START_HEIGHT`

---

## NEDB Schema

### `headers/` — L1 block headers
```json
{ "_id": "<block_hash_hex>", "hdr": "<80-byte-hex>", "height": 648643,
  "caused_by": ["<prev_block_hash_hex>"] }
```

### `blocks/` — L1 block bodies
```json
{ "_id": "<block_hash_hex>", "raw": "<consensus_bytes_hex>" }
```

### `index/` — chain tip
```json
{ "_id": "tip", "height": 648786, "hash": "<block_hash_hex>" }
```

### `evm_accounts/` — aITC balances
```json
{ "_id": "<eth_addr_hex>", "balance": "<u256_hex>", "nonce": 0,
  "code_hash": "<keccak_hex>", "origin": "bridge_deposit",
  "l1_txid": "<txid_hex>", "caused_by": ["<l1_txid_hex>"] }
```

### `oracle_minted/` — idempotency guards
```json
{ "_id": "minted:<l1_txid_hex>", "l1_txid": "...", "net_sats": 99502634,
  "gross_sats": 105266600, "aitc_address": "<eth_addr_hex>",
  "minted_at_l1": 648643, "balance_written": true,
  "caused_by": ["<l1_txid_hex>"] }
```

### `oracle_pending/` — confirmation queue
```json
{ "_id": "pending:<l1_txid_hex>", "txid": "...", "amount_sats": 105266600,
  "aitc_address": "<eth_addr_hex>", "l1_height": 648641,
  "required_height": 648643 }
```

### `oracle_state/` — oracle metadata
```json
{ "_id": "tip", "height": 648786 }
{ "_id": "pending_index", "ids": ["pending:<txid>", ...] }
```

---

## Causality & Provenance

Every write carries `caused_by: [parent_id]` — NEDB's DAG provenance hook.
This enables:
- `AS OF` time-travel queries on any collection
- `TRACE` — walk back from any balance to the L1 tx that created it
- Tamper-evident Merkle root anchored to ITC L1 via OP_RETURN

---

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `ITC_NODE_DATADIR` | `./itc-node-data` | Root storage directory |
| `ITC_ORACLE_START_HEIGHT` | `1` | Block height to start oracle scan |
| `ITC_ORACLE_REPLAY` | unset | Set any value to trigger replay on boot |

---

## Versioning

| Version | Date | Notes |
|---------|------|-------|
| v1.0.0 | 2026-06-29 | Initial storage contract — ITC-L2 genesis day |

*© Interchained LLC 2026 — by Mark × Vex*
