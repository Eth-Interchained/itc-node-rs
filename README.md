# itc-node-rs — ITC-L2 EVM Sidechain (aITC)

> **Proof of Sovereignty.** A full EVM sidechain anchored to ITC mainnet.
> Your ITC private key is your aITC key — same secp256k1, both chains.
>
> © Interchained LLC × Claude Sonnet 4.6 — MIT License

---

## What it is

**ITC-L2** is a sovereign EVM sidechain running on top of ITC mainnet.

- Run any Solidity contract. Deploy ERC-20s, ERC-721s, DeFi protocols.
- Every state transition stored in **NEDB** — content-addressed, tamper-evident, AS OF time-travel queryable, causally traceable.
- Anchored to **ITC L1** every 100 blocks via `OP_RETURN` (NEDB Merkle state root).
- **aITC (Anchored ITC)** is the native gas coin — same secp256k1 key as mainnet ITC.

---

## The Bridge: One Key, Both Chains

No wrapped tokens. No ERC-20. No sentinel daemon watching for events.

**How it works:**

1. Open the bridge app (`itc-bridge`). Enter an amount. Click "Bridge to L2."
2. The app builds a standard ITC transaction sending to `BRIDGE_LOCK_ADDRESS`.
3. You sign it with your ITC private key (Elara wallet or any ITC wallet).
4. The oracle inside this node detects the deposit in the downloaded block.
5. It recovers your secp256k1 pubkey from the P2PKH scriptSig.
6. Derives your aITC (ETH-format) address: `keccak256(uncomp_pubkey[1:])[12:]`
7. Mints aITC to that address as native coin — no contract call needed.
8. Import your ITC key into MetaMask, add network ITC-L2 (chain 17101) → your aITC is there.

**Why this can't be faked:** Spending a UTXO requires signing with your private key.
The signature reveals your pubkey. The pubkey determines your L2 address. The oracle
reads blocks it already downloaded — no external trust, no OP_RETURN to manipulate.

**Governance fee:** 5% by default (`ITC_BRIDGE_FEE_BPS=500`). Configurable.
Lock 1 ITC → receive 0.95 aITC. The fee stays locked in the bridge address.

---

## Status

| Item | What | Status |
|------|------|--------|
| ITC P2P peer | Headers + block bodies, serves to peers, stays current | ✅ |
| NEDB persistence | Headers, blocks, EVM state — all in NEDB, instant boot | ✅ |
| EVM engine | revm + NedbState backend, provenance on every write | ✅ |
| eth_* JSON-RPC | MetaMask-compatible on :8545, EIP-155 ecrecover | ✅ |
| L1 anchor poster | OP_RETURN sovereignty proof every N epochs | ✅ |
| Deposit oracle | P2PKH scriptSig pubkey recovery → aITC mint | ✅ |
| Governance fee | Configurable BPS, locked in bridge address | ✅ |
| Block production | L2 sequencer / tx pool | 🔜 |
| Exit (aITC → ITC) | Burn on L2 → release on L1 | 🔜 |
| itc-bridge app | Next.js + FastAPI + NEDB, builds the bridge tx | separate repo |

---

## Quick Start

```bash
cargo build --release

# Required env vars
export ITC_BRIDGE_HASH160="<40-hex-char hash160 of your bridge lock address>"
export ITC_RPC_ADDR="0.0.0.0:8545"           # MetaMask endpoint
export ITC_NODE_DATADIR="./itc-node-data"

# Optional
export ITC_BRIDGE_FEE_BPS=500                  # 5% governance fee (default)
export ITC_BRIDGE_CONFIRMATIONS=3              # L1 confirmations before mint
export ITC_ANCHOR_WIF="<WIF key for L1 anchor posting>"

./target/release/itc-node [LISTEN_PORT]
```

**Add ITC-L2 to MetaMask:**
- Network name: ITC-L2
- RPC URL: `http://<your-node>:8545`
- Chain ID: `17101`
- Currency: `aITC`

**Import your ITC private key into MetaMask** → same key, your aITC balance appears automatically after bridging.

---

## Crate Layout

```
itc-node-rs/
├── crates/
│   ├── itc-proto/     # ITC P2P wire protocol, tx parsing, script analysis
│   ├── itc-node/      # Node binary: sync, serve, oracle, RPC, anchor
│   ├── itc-evm/       # revm + NedbState EVM execution engine
│   ├── itc-rpc/       # eth_* JSON-RPC server, ecrecover
│   ├── itc-anchor/    # L1 OP_RETURN poster, secp256k1 P2PKH signer
│   └── itc-oracle/    # Deposit oracle, UTXO mirror, governance fee
├── contracts/
│   └── ITC.sol        # (Reference only) ERC-20 for optional wrapped aITC
└── docs/
    ├── ARCHITECTURE.md
    └── BRIDGE.md
```

---

## Why NEDB as EVM State

NEDB gives ITC-L2 a property no other EVM chain has:

- **`caused_by: [tx_hash]`** on every account/storage write → full causal graph
- **AS OF seq N** → replay any historical account state at any point in time
- **`db.verify()`** → tamper detection on the full EVM state
- **NEDB Merkle head** posted to ITC L1 every N epochs → external verifiability without downloading all L2 data

---

© Interchained LLC × Claude Sonnet 4.6 — MIT License
