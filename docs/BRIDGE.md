# ITC-L2 Federated Bridge

> © Interchained LLC × Claude Sonnet 4.6  
> Status: **design-locked** (2026-06-28)

---

## Overview

The ITC-L2 bridge is a **federated, operator-run, always-on** bridge between
ITC mainnet (L1) and the ITC-L2 sidechain (L2). Interchained LLC operates both
sentinels. The bridge is intentionally centralized in v1 — trust is in the operator,
not a trustless protocol. Decentralization is a future upgrade path.

**Core invariant:** ITC supply on L2 = ITC locked in the bridge address on L1. Always.
Zero ITC exist at sidechain genesis. Every ITC in circulation was bridged in.

---

## Architecture

```
ITC L1 (mainnet)                      ITC-L2 (sidechain)
────────────────                       ─────────────────────────────
Bridge Lock Address                    ITC ERC-20 Contract
  (P2SH multisig,                        (mint: L2 Sentinel only
   keys held by sentinel)                 burn: anyone, triggers exit)

        ↑ watches UTXOs                       ↑ watches Burn events
┌───────────────────┐                  ┌──────────────────────────┐
│   L1 Sentinel     │◄─── RPC ────────►│   L2 Sentinel            │
│  (itc-sentinel)   │                  │  (itc-sentinel)           │
└───────────────────┘                  └──────────────────────────┘
        ↓ submits L1 txs                      ↓ submits L2 txs
   ITC mainnet                          ITC-L2 EVM
```

Both sentinels are daemons operated by Interchained LLC.
They communicate over an authenticated internal channel (mTLS, shared secret).
Each sentinel has a funded wallet on its respective chain for gas/fees.

---

## Bridge Flows

### Flow 1 — Deposit: ITC → ITC (L1 to L2)

```
User                    L1 Sentinel             L2 Sentinel
 │                           │                       │
 │  1. Send ITC to           │                       │
 │     bridge lock addr      │                       │
 │     OP_RETURN:            │                       │
 │       [L2_ADDR_20]        │                       │
 │  (+ L1 deposit fee)       │                       │
 │──────────────────────────►│                       │
 │                           │  2. Detect lock tx    │
 │                           │     (3 L1 confirms)   │
 │                           │──────────────────────►│
 │                           │                       │  3. Verify lock
 │                           │                       │     amount ≥ min
 │                           │                       │  4. Deduct L2 fee
 │                           │                       │  5. Call ITC.mint(
 │                           │                       │       l2_addr,
 │                           │                       │       net_amount)
 │                           │                       │──► L2 EVM
 │◄──────────────────────────────────────────────────│
 │  6. ITC arrives in       │                       │
 │     L2 wallet             │                       │
```

**Deposit fee:** collected on the L1 side, denominated in ITC.
The OP_RETURN output encodes the destination L2 address (20 bytes).
Minimum deposit enforced by the sentinel (reject dust bridging).

### Flow 2 — Withdrawal: ITC → ITC (L2 to L1)

```
User                    L2 Sentinel             L1 Sentinel
 │                           │                       │
 │  1. Call ITC.initiateExit(│                       │
 │       itc_l1_addr,        │                       │
 │       amount)             │                       │
 │     (burns ITC,          │                       │
 │      emits Exit event)    │                       │
 │──────────────────────────►│                       │
 │                           │  2. Detect Exit event │
 │                           │     (1 L2 confirm)    │
 │                           │──────────────────────►│
 │                           │                       │  3. Verify burn
 │                           │                       │     on L2 (irreversible)
 │                           │                       │  4. Build L1 release tx:
 │                           │                       │     - Send (amount - fee)
 │                           │                       │       ITC to itc_l1_addr
 │                           │                       │     - Fee in ITC retained
 │                           │                       │       by sentinel wallet
 │◄──────────────────────────────────────────────────│
 │  5. ITC arrives in        │                       │
 │     L1 wallet             │                       │
```

**Withdrawal fee:** collected on the L2 side, denominated in ITC.
The ITC burn is irreversible before the L1 release is sent.
The sentinel confirms the L2 burn with at least 1 L2 block finality before touching L1.

---

## Fee Model

| Direction   | Fee currency | Fee rate      | Who collects  |
|------------|-------------|---------------|---------------|
| L1 → L2    | ITC (native) | 0.1% of amount (min 0.01 ITC) | L1 Sentinel wallet |
| L2 → L1    | ITC         | 0.1% of amount (min 0.01 ITC) | L2 Sentinel wallet |

Fees are configurable by the operator (sentinel config, hot-reloadable).
Fee wallets are separate from the bridge lock address.
Collected fees remain on their respective chains — ITC fees stay on L1,
ITC fees stay on L2 (or can be bridged back by the operator).

---

## ITC ERC-20 Contract Spec

```solidity
// SPDX-License-Identifier: MIT
// Deployed at genesis by the L2 sentinel at a deterministic address.
// Only the bridge sentinel (MINTER_ROLE) may mint or burn.

interface IITC {
    // Called by L2 Sentinel on successful deposit
    function mint(address to, uint256 amount) external;

    // Called by user to initiate withdrawal — burns ITC, emits Exit
    function initiateExit(address itcL1Recipient, uint256 amount) external;

    // ERC-20 standard
    function transfer(address to, uint256 amount) external returns (bool);
    function approve(address spender, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    // Events
    event Exit(address indexed from, address itcL1Recipient, uint256 amount);
    event Mint(address indexed to, uint256 amount);
}
```

Properties:
- **Zero supply at genesis** — no constructor mint
- **Mintable only by L2 Sentinel** (role-gated, OpenZeppelin AccessControl)
- **Burnable by anyone** (via `initiateExit`) — this is the exit trigger
- **ERC-20 compliant** — wallets, DEXes, and other contracts use it normally
- **Non-upgradeable in v1** — the bridge address is fixed at deploy time

---

## Sentinel Daemon Spec

Both sentinels run as persistent daemons (`itc-sentinel`), implemented in Rust.

### L1 Sentinel responsibilities
- Watch the bridge lock address for incoming UTXOs (polling itcd RPC / electrumx)
- Parse `OP_RETURN` payload to extract the destination L2 address
- Wait for 3 L1 confirmations (configurable)
- Notify L2 Sentinel with: `{l2_addr, net_amount, l1_txid, l1_height}`
- Watch for L1 release requests from L2 Sentinel
- Build and broadcast L1 release transactions (from locked pool)

### L2 Sentinel responsibilities
- Watch the ITC contract for `Exit` events
- Wait for 1 L2 confirmation
- Notify L1 Sentinel with: `{itc_l1_addr, net_amount, l2_txid, l2_height}`
- Receive deposit instructions from L1 Sentinel
- Call `ITC.mint(to, amount)` on the L2 EVM

### Inter-sentinel communication
- Authenticated gRPC (mTLS, both sides present client certs)
- Retry with exponential backoff (max 1h retry window)
- All bridge operations logged to NEDB on both sides for auditability
- Pending operations survive sentinel restart (NEDB persistence)

### Always-on guarantees
- Each sentinel runs under systemd with `Restart=always`
- Pending bridge operations are stored in NEDB; on restart, the sentinel
  resumes from the last unconfirmed operation
- No operation is lost if a sentinel restarts mid-flight — idempotency keys
  prevent double-minting or double-releasing

---

## Security Model

**Trust boundary:** The operator (Interchained LLC) controls both sentinels.
This is a **trusted bridge** — users are trusting the operator not to:
- Mint ITC without a corresponding L1 lock
- Fail to release ITC after a confirmed ITC burn

**Mitigations:**
- All bridge operations are anchored in NEDB on both sides (auditable)
- The L1 lock address is public — anyone can verify the locked ITC matches ITC supply
- L1 anchor `OP_RETURN` posts include the NEDB Merkle head — external verifiers can
  confirm the bridge is operating correctly without trusting the operator

**v1 limitations (acknowledged):**
- Operator can censor withdrawals (not relay the Exit event to L1)
- Operator can run off with locked ITC
- No trustless exit mechanism in v1

**v2 upgrade path (not in scope now):**
- Replace federated bridge with a light-client bridge (SPV proofs)
- Add forced-exit mechanism via L1 (user can exit without sentinel cooperation)
- Multi-party sentinel federation (M-of-N threshold)

---

## Minimum Bridge Amounts

| Operation    | Minimum  | Rationale                             |
|-------------|---------|---------------------------------------|
| Deposit      | 1 ITC   | Cover L1 fees + bridge overhead       |
| Withdrawal   | 1 ITC  | Cover L1 release tx fee               |

Dust is rejected at the sentinel level before any on-chain action.

---

## Bridge Lock Address (L1)

The bridge lock address is a P2SH multisig controlled by the L1 Sentinel's signing key(s).
In v1 this is a 1-of-1 (single sentinel key). In v2 it becomes M-of-N.

**Address derivation:** deterministic from the sentinel's extended public key and a
well-known derivation path `m/44'/0'/0'/2/0` (bridge path).

The address is published in `docs/BRIDGE_ADDRESSES.md` and hardcoded in the sentinel
config. It never changes without a coordinated migration + announcement.
