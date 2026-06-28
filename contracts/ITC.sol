// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title ITC — Bridged Interchained on ITC-L2
/// @author Interchained LLC × Claude Sonnet 4.6
///
/// @dev Minting is restricted to a single hard-coded EOA (MINTER), matched to
///      the bridge operator's keystore public key. No role system, no upgradeable
///      proxy — the authorized address is baked in at compile time. If the operator
///      key rotates, redeploy. Simple and auditable.
///
///      Supply model:
///        - Zero at genesis. No constructor mint, no allocation.
///        - ITC enters circulation only when the bridge operator calls mint()
///          after confirming a lock on ITC L1 mainnet.
///        - ITC leaves circulation when a holder calls initiateExit() — this burns
///          the tokens and emits an Exit event. The bridge app (itc-bridge, separate
///          Next.js + FastAPI repo) watches for Exit events and releases ITC on L1.
///
///      Trust model:
///        - MINTER is the sole party that can create ITC on L2.
///        - Any holder can exit at any time by burning their ITC (no permission needed).
///        - The bridge app is operated by Interchained LLC. Centralized by design (v1).

contract ITC {
    // ── Token metadata ────────────────────────────────────────────────────────
    string  public constant name     = "Interchained";
    string  public constant symbol   = "ITC";
    uint8   public constant decimals = 18;

    // ── Authorized minter ─────────────────────────────────────────────────────
    /// @dev The bridge operator's EOA. Only this address may call mint().
    ///      Set this to the public address of the keystore before deployment.
    ///      To rotate: redeploy with the new address.
    address public constant MINTER = address(0); // TODO: set before deploy

    // ── ERC-20 state ──────────────────────────────────────────────────────────
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    // ── Events ─────────────────────────────────────────────────────────────────
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    /// @notice Emitted when the bridge operator mints ITC to a recipient.
    event Mint(address indexed to, uint256 amount);

    /// @notice Emitted when a holder exits to ITC L1. The bridge app watches this
    ///         event and releases the equivalent ITC on mainnet to `itcL1Recipient`.
    ///         `itcL1Recipient` is a bech32 / base58 ITC mainnet address string.
    event Exit(address indexed from, string itcL1Recipient, uint256 amount);

    // ── Errors ─────────────────────────────────────────────────────────────────
    error NotMinter();
    error InsufficientBalance();
    error InsufficientAllowance();
    error ZeroAmount();

    // ── Mint (bridge operator only) ───────────────────────────────────────────
    /// @notice Mint `amount` ITC to `to`.
    ///         Called by the bridge operator's keystore after a confirmed L1 lock.
    function mint(address to, uint256 amount) external {
        if (msg.sender != MINTER) revert NotMinter();
        if (amount == 0) revert ZeroAmount();
        unchecked { totalSupply += amount; }
        unchecked { balanceOf[to] += amount; }
        emit Transfer(address(0), to, amount);
        emit Mint(to, amount);
    }

    // ── Exit (anyone) ─────────────────────────────────────────────────────────
    /// @notice Burn `amount` ITC and initiate an exit to ITC L1.
    ///         The bridge app (itc-bridge) watches for this event and releases
    ///         the equivalent ITC to `itcL1Recipient` on mainnet.
    /// @param itcL1Recipient  The ITC mainnet address to receive the unlocked ITC.
    /// @param amount          Amount to burn (in wei, 18 decimals).
    function initiateExit(string calldata itcL1Recipient, uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        if (balanceOf[msg.sender] < amount) revert InsufficientBalance();
        unchecked { balanceOf[msg.sender] -= amount; }
        unchecked { totalSupply -= amount; }
        emit Transfer(msg.sender, address(0), amount);
        emit Exit(msg.sender, itcL1Recipient, amount);
    }

    // ── ERC-20 standard ───────────────────────────────────────────────────────
    function transfer(address to, uint256 amount) external returns (bool) {
        if (balanceOf[msg.sender] < amount) revert InsufficientBalance();
        unchecked { balanceOf[msg.sender] -= amount; }
        unchecked { balanceOf[to] += amount; }
        emit Transfer(msg.sender, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        if (balanceOf[from] < amount) revert InsufficientBalance();
        if (allowance[from][msg.sender] < amount) revert InsufficientAllowance();
        unchecked { allowance[from][msg.sender] -= amount; }
        unchecked { balanceOf[from] -= amount; }
        unchecked { balanceOf[to] += amount; }
        emit Transfer(from, to, amount);
        return true;
    }
}
