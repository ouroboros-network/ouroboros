# Midgard Wallet

A CLI wallet for the Ouroboros blockchain written in Rust.

## Features

- **Generate new wallets** with BIP39 12-word mnemonic phrases
- **Import wallets** from mnemonic phrase or private key
- **Check balance** for your address (mainchain and microchains)
- **Send OURO tokens** to other addresses
- **View transaction history**
- **Monitor node status** with detailed information
- **View connected peers**
- **List and interact with microchains**

## Installation

Build the wallet from source:

```bash
cd midgard_wallet
cargo build --release
```

The compiled binary will be located at `target/release/midgard-wallet.exe` (Windows) or `target/release/midgard-wallet` (Linux/Mac).

## Usage

### Wallet Management

**Create a New Wallet:**
```bash
midgard-wallet create --name "My Wallet"
```
This generates a new wallet with a 12-word mnemonic phrase. Save the mnemonic securely - it's the only way to recover your wallet.

**Import from Mnemonic:**
```bash
midgard-wallet import --mnemonic "your twelve word mnemonic phrase goes here" --name "Imported Wallet"
```

**Import from Private Key:**
```bash
midgard-wallet import --private-key "your_private_key_hex" --name "Imported Wallet"
```

**View Wallet Info:**
```bash
midgard-wallet info
```

### Balance and Transactions

**Check Balance:**
```bash
midgard-wallet balance
```

**Send OURO Tokens:**
```bash
midgard-wallet send <recipient_address> <amount> --fee 1000
```
Amount is in smallest units (1 OURO = 1,000,000,000,000 units). Nonce is automatically fetched from the blockchain.

**View Transaction History:**
```bash
midgard-wallet history --limit 20
```

### Node Monitoring

**Check Node Status:**
```bash
midgard-wallet status
```

**View Detailed Node Info:**
```bash
midgard-wallet node
```
Shows node ID, version, block height, peer count, sync status, mempool size, and uptime.

**View Connected Peers:**
```bash
midgard-wallet peers
```

### Microchains

**List Microchains:**
```bash
midgard-wallet microchains
```

**Check Microchain Balance:**
```bash
midgard-wallet micro-balance <microchain_id>
```

### Connect to Custom Node

By default, the wallet connects to `http://localhost:8001`. To use a different node:

```bash
midgard-wallet --node-url http://your-node-ip:8001 balance
```

## Command Reference

| Command | Description |
|---------|-------------|
| `create` | Create a new wallet |
| `import` | Import wallet from mnemonic or private key |
| `info` | Show wallet information |
| `balance` | Check mainchain balance |
| `send` | Send OURO tokens |
| `history` | View transaction history |
| `status` | Quick node status check |
| `node` | Detailed node information |
| `peers` | List connected peers |
| `microchains` | List available microchains |
| `micro-balance` | Check balance on a microchain |

## Wallet Storage

The wallet is stored in your home directory:
- Windows: `C:\Users\YourName\midgard_wallet.json`
- Linux/Mac: `~/midgard_wallet.json`

## Security

1. **Backup your mnemonic phrase** - Write it down and store it securely offline
2. **Never share your mnemonic or private key** with anyone
3. **The wallet file contains your private key** - keep it secure
4. For production use, consider adding encryption to the wallet file

## Transaction Format

Transactions are signed using Ed25519 and include:
- Sender/recipient addresses
- Amount and fee
- Nonce (for replay protection)
- Chain ID (default: "ouroboros-mainnet-1")
- Optional payload for smart contract calls

## Requirements

- Ouroboros node running at configured URL
- Rust 1.70+ for building from source

## Compatibility

This wallet is compatible with Ouroboros blockchain with:
- Chain ID support ("ouroboros-mainnet-1")
- Automatic nonce management via `/ouro/nonce/{address}` endpoint
- Ed25519 signature verification
- Replay protection through chain_id + nonce
- Bech32 address encoding with "ouro" prefix

## License

MIT License

## Contributing

Contributions are welcome. Please submit a Pull Request.
