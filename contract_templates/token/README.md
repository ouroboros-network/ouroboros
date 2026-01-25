# OVM Token Contract Template

ERC20-like fungible token implementation for Ouroboros Virtual Machine.

## Features

- Standard token interface (transfer, approve, transferFrom)
- Minting capabilities (owner only)
- Burning mechanism
- Event emission
- Full test coverage
- Gas-optimized

## Usage

### Build for WASM

```bash
cargo build --target wasm32-unknown-unknown --release
```

### Deploy

```bash
# The compiled contract will be at:
# target/wasm32-unknown-unknown/release/ouro_token_template.wasm
```

### API Reference

#### Initialize
```rust
initialize(name: String, symbol: String, decimals: u8, owner: String) -> TokenState
```

#### Read Methods (no gas cost)
- `balance_of(address: String) -> u64`
- `allowance(owner: String, spender: String) -> u64`
- `total_supply() -> u64`
- `name() -> String`
- `symbol() -> String`
- `decimals() -> u8`

#### Write Methods
- `transfer(to: String, amount: u64) -> Result<()>`
- `approve(spender: String, amount: u64) -> Result<()>`
- `transfer_from(from: String, to: String, amount: u64) -> Result<()>`
- `mint(to: String, amount: u64) -> Result<()>` (owner only)
- `burn(amount: u64) -> Result<()>`

## Events

- `Transfer(from, to, amount)`
- `Approval(owner, spender, amount)`
- `Mint(to, amount, total_supply)`
- `Burn(from, amount, total_supply)`

## Example

```rust
// Initialize token
let mut state = initialize(
    "My Token".to_string(),
    "MTK".to_string(),
    18,
    "0x...owner".to_string(),
);

// Mint initial supply
mint(&mut state, "0x...owner", "0x...recipient", 1_000_000)?;

// Transfer tokens
transfer(&mut state, "0x...sender", "0x...receiver", 100)?;

// Approve spender
approve(&mut state, "0x...owner", "0x...spender", 500)?;

// Spend allowance
transfer_from(&mut state, "0x...spender", "0x...owner", "0x...receiver", 200)?;
```

## Testing

```bash
cargo test
```

## Gas Costs (Estimated)

| Operation | Gas Cost |
|-----------|----------|
| transfer | ~5,000 |
| approve | ~3,000 |
| transferFrom | ~7,000 |
| mint | ~6,000 |
| burn | ~5,000 |

*Actual costs depend on OVM gas schedule*

## License

MIT
