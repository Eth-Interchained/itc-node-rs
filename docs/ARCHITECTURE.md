# ITC-L2 — Sidechain Architecture

> © Interchained LLC × Claude Sonnet 4.6  
> Status: **design-locked** (2026-06-28)

---

## What It Is

**ITC-L2** is a sovereign EVM sidechain anchored to the ITC mainnet (L1).
It runs full Solidity smart contracts, has its own block production and transaction fees,
and uses **NEDB** as its verifiable, bi-temporal state store.
The chain is secured by ITC L1 via periodic `OP_RETURN` anchor posts containing the
sidechain's NEDB Merkle state root.

ITC is the root of trust. The sidechain inherits ITC's finality, not Ethereum's.

---

## System Layers

```
┌─────────────────────────────────────────────────────────────┐
│                       Applications                          │
│          (Solidity contracts, ERC-20/721/1155, DeFi)        │
├─────────────────────────────────────────────────────────────┤
│                    ITC-L2 EVM Runtime                       │
│   revm execution engine  ·  itc-evm crate                   │
│   NEDB state backend (accounts / storage / code)            │
├─────────────────────────────────────────────────────────────┤
│                   ITC-L2 Node (itc-node-rs)                 │
│   Block production  ·  tx pool  ·  eth_* RPC                │
│   Peer-to-peer header relay (existing slice 1-4 work)       │
├─────────────────────────────────────────────────────────────┤
│               Federated Bridge (Sentinel Pair)              │
│   Lock/Mint  ·  Burn/Release  ·  Always-on processing       │
│   Fee in native currency on each side                       │
├─────────────────────────────────────────────────────────────┤
│                    ITC L1 Mainnet (itcd)                     │
│   SHA-256 PoW  ·  UTXO model  ·  OP_RETURN anchors          │
│   ITC lock address (bridge P2SH multisig)                  │
└─────────────────────────────────────────────────────────────┘
```

---

## Chain Parameters

| Parameter            | Value                                      |
|---------------------|--------------------------------------------|
| Chain ID            | `17101` *(pending registration on chainlist.org)* |
| Consensus           | Authority / PoA (operator-run, v1)         |
| Block time target   | 5 seconds                                  |
| Native token        | `ITC` (bridged from ITC mainnet 1:1 — see Bridge)        |
| EVM spec            | London (revm default; upgradeable)         |
| State backend       | NEDB v2 (content-addressed DAG, AES-256-GCM at rest) |
| L1 anchor interval  | Every 100 L2 blocks                        |
| L1 anchor method    | `OP_RETURN` carrying `[NEDB_HEAD_32 || L2_BLOCK_HASH_32]` |

---

## Native Token: ITC

**ITC starts at zero supply on genesis.** No pre-mint, no allocation.
The only way ITC enters circulation on the sidechain is via the bridge:
ITC locked on L1 → equal ITC minted on L2. 1:1, always.

ITC is an ERC-20 deployed at a well-known address on the sidechain
(`itc-l2/contracts/ITC.sol`). The bridge sentinel is the sole authorized minter/burner.
Gas on the sidechain is denominated in ITC, so the bridge is the entry point
for all economic activity.

---

## EVM State in NEDB

The sidechain's EVM state lives entirely in NEDB, structured as three collections:

| Collection   | Key                   | Value                                      | DAG edge          |
|-------------|----------------------|--------------------------------------------|-------------------|
| `accounts`  | `address` (hex)      | `{balance, nonce, code_hash}`              | `caused_by: [tx_hash]` |
| `storage`   | `address:slot` (hex) | `{value}`                                  | `caused_by: [tx_hash]` |
| `code`      | `code_hash` (hex)    | `{bytecode}`                               | immutable, no edge |

Every state write carries `caused_by: [tx_hash]`, so the full causal graph of
every balance change, storage slot mutation, and contract deployment is
TRACE-queryable and AS OF time-travel readable. This is a property unique to
ITC-L2 among EVM chains.

NEDB's Merkle head after each L2 block IS the state root posted to ITC L1.

---

## L1 Anchor Flow

```
Every 100 L2 blocks:
  1. Finalize the L2 block → compute NEDB Merkle head (db.head())
  2. Build an ITC L1 transaction:
       output: OP_RETURN <0x49544332> <nedb_head_32_bytes> <l2_block_hash_32_bytes>
       (prefix 0x49544332 = "ITC2" — marks anchor posts)
  3. Broadcast to ITC L1 via the L1 sentinel (has a funded ITC address for fees)
  4. On L1 confirmation: log [ANCHOR] height=<l1_height> l2_block=<n> root=<hex>
```

Any third party can verify the sidechain's integrity by:
1. Reading `OP_RETURN` outputs from ITC L1 with prefix `0x49544332`
2. Running `db.verify()` on the sidechain's NEDB store
3. Confirming the Merkle head matches the on-chain anchor

---

## NEDB Provenance Properties

Because the EVM state backend is NEDB, every L2 transaction produces:

- **Tamper-evidence** — BLAKE2b hash chain across all writes; `verify()` detects any mutation
- **Time-travel** — `SELECT * FROM accounts AS OF seq 1000` replays any historical state
- **Causal graph** — `TRACE caused_by` walks from any account balance back to the tx tree that produced it
- **L1-vouched epochs** — the anchor `OP_RETURN` commits a NEDB Merkle head at a known L2 height,
  giving external verifiers an epoch commitment without downloading all L2 blocks

---

## Relationship to ITC Mainnet

The sidechain is **not** a rollup — it does not post all transactions to L1.
It is a **sidechain** with an optional exit mechanism (the bridge).

ITC L1 provides:
- Root of trust (anchor / OP_RETURN epoch commitments)
- The asset backing for ITC (locked ITC on L1 = ITC on L2)
- A censorship fallback (future: forced exit via L1 tx)

The sidechain provides:
- EVM execution (Solidity, ERC-20/721/1155, DeFi)
- Fast finality (5s blocks vs ITC's ~DGW block time)
- Low-fee programmable transactions
- Smart contract platform with NEDB-native state provenance
