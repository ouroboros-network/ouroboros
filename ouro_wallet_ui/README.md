# Ouroboros Wallet Desktop UI

Modern desktop wallet application for the Ouroboros blockchain, built with Tauri, Rust, and web technologies.

## Features

- **Secure Wallet Management**: Create, import, and manage wallets with BIP39 mnemonic phrases
- **Balance Tracking**: View mainchain and microchain balances in real-time
- **Send Transactions**: Send OURO tokens on mainchain and microchains
- **Receive Tokens**: Display wallet address and QR code for receiving payments
- **Microchain Support**: Interact with multiple microchains from a single interface
- **Transaction History**: View all past transactions with detailed information
- **Node Linking**: Link wallet to nodes for reward distribution
- **Customizable**: Configure node connection and wallet settings

## Architecture

### Backend (Rust/Tauri)
- **Wallet Management**: Secure key generation, storage, and signing using Ed25519
- **RPC Communication**: HTTP client for interacting with Ouroboros nodes
- **Tauri Commands**: Bridge between frontend and backend for secure operations

### Frontend (Web Technologies)
- **Modern UI**: Clean, responsive interface with dark theme
- **Real-time Updates**: Live balance and transaction updates
- **Modular Design**: Page-based navigation system

## Prerequisites

- Rust (1.70+)
- Node.js (18+)
- npm or yarn
- An Ouroboros node running at http://localhost:8001 (or configure custom URL)

## Installation

### From Source

1. Clone the repository:
```bash
cd ouro_wallet_ui
```

2. Install frontend dependencies:
```bash
npm install
```

3. Install Tauri CLI (if not already installed):
```bash
npm install -g @tauri-apps/cli
```

## Development

Run in development mode with hot-reload:

```bash
npm run tauri dev
```

This will:
1. Start the Vite dev server for the frontend
2. Compile and run the Tauri backend
3. Launch the desktop application

## Building

Build the production application:

```bash
npm run tauri build
```

This creates optimized binaries for your platform in `src-tauri/target/release/bundle/`

### Platform-specific Builds

**Windows**:
```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/msi/Ouroboros Wallet_0.3.0_x64_en-US.msi
```

**macOS**:
```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/dmg/Ouroboros Wallet_0.3.0_x64.dmg
```

**Linux**:
```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/deb/ouroboros-wallet_0.3.0_amd64.deb
#         src-tauri/target/release/bundle/appimage/ouroboros-wallet_0.3.0_amd64.AppImage
```

## Usage

### Creating a Wallet

1. Launch the application
2. Click "Create New Wallet"
3. **IMPORTANT**: Write down the 12-word recovery phrase
4. Store it securely - this is the ONLY way to recover your wallet

### Importing a Wallet

**From Recovery Phrase**:
1. Click "Import Wallet"
2. Select "Import from Recovery Phrase"
3. Enter your 12-word phrase
4. Click "Import Wallet"

**From Private Key**:
1. Click "Import Wallet"
2. Select "Import from Private Key"
3. Enter your private key (hex format)
4. Click "Import Wallet"

### Sending Transactions

1. Navigate to "Send" page
2. Select chain (Mainchain or Microchain)
3. Enter recipient address (ouro1...)
4. Enter amount
5. Click "Send Transaction"

### Linking to Node

Link your wallet to a node to receive rewards:

1. Go to Dashboard
2. Click "Link to Node"
3. Confirm the linking request

## Configuration

### Node Connection

1. Navigate to "Settings"
2. Update "Node URL" field
3. Click "Save"

Default: `http://localhost:8001`

### Wallet Data Location

Wallet data is stored securely in:
- **Windows**: `%APPDATA%\com.ouroboros.wallet\`
- **macOS**: `~/Library/Application Support/com.ouroboros.wallet/`
- **Linux**: `~/.local/share/com.ouroboros.wallet/`

## Security Features

- Ed25519 cryptographic signatures
- BIP39 mnemonic phrase support
- Secure local storage
- Private keys never leave your device
- All transactions signed locally

## Project Structure

```
ouro_wallet_ui/
├── src/                    # Frontend source
│   ├── main.js            # Main JavaScript logic
│   └── styles.css         # CSS styles
├── src-tauri/             # Tauri backend
│   ├── src/
│   │   ├── main.rs        # Tauri entry point
│   │   ├── wallet.rs      # Wallet management
│   │   └── commands.rs    # Tauri commands (API)
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── index.html             # HTML entry point
├── package.json           # Node.js dependencies
└── vite.config.js         # Vite configuration
```

## Tauri Commands (API)

The backend exposes these commands to the frontend:

### Wallet Management
- `create_wallet(name: Option<String>)` - Create new wallet
- `import_wallet(mnemonic: String, name: Option<String>)` - Import from mnemonic
- `import_from_key(private_key: String, name: Option<String>)` - Import from private key
- `get_wallet_info()` - Get wallet information
- `export_mnemonic()` - Export recovery phrase

### Blockchain Operations
- `get_balance(node_url: String)` - Get mainchain balance
- `get_microchain_balance(node_url: String, microchain_id: String)` - Get microchain balance
- `send_transaction(node_url: String, to: String, amount: u64)` - Send mainchain transaction
- `send_microchain_transaction(...)` - Send microchain transaction
- `list_microchains(node_url: String)` - List all microchains
- `link_to_node(node_url: String)` - Link wallet to node
- `get_transaction_history(node_url: String)` - Get transaction history

## Troubleshooting

### "Failed to connect to node"
- Ensure an Ouroboros node is running at the configured URL
- Check firewall settings
- Verify node URL in Settings

### "Invalid mnemonic"
- Ensure you're using the correct 12-word phrase
- Check for typos
- Words must be from the BIP39 word list

### Build errors
```bash
# Clean and rebuild
rm -rf node_modules dist
npm install
npm run tauri build
```

## Development Tips

### Hot Reload
The frontend supports hot reload during development. Changes to `src/` files will automatically refresh the UI.

### Backend Changes
Changes to Rust code in `src-tauri/src/` require a restart:
1. Stop the dev server (Ctrl+C)
2. Run `npm run tauri dev` again

### Debugging
- **Frontend**: Open DevTools (Right-click → Inspect)
- **Backend**: Check console output in terminal

## Contributing

This wallet UI is part of the Ouroboros blockchain project. For issues and contributions, visit the main repository.

## Security Considerations

**WARNING**: This is alpha software. Use at your own risk.

- Always backup your recovery phrase
- Never share your private key or mnemonic
- Test with small amounts first
- Verify all transaction details before sending

## Roadmap

- Core wallet functionality - Complete
- Mainchain and microchain support - Complete
- Transaction history - Complete
- QR code scanning - Planned
- Multi-wallet support - Planned
- Hardware wallet integration - Planned
- Built-in node status monitoring - Planned
- Staking interface - Planned

## License

MIT License - see LICENSE file for details

## Version

Current version: 0.3.0 (Alpha)

This application is under active development. Features and APIs may change.
