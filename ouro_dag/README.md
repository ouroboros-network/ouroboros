# Ouroboros Node
A high-performance Directed Acyclic Graph (DAG) node with Post-Quantum security.

## Quick Start

### Installation (Linux & macOS)
For the simplest installation, run:
```bash
curl -sSL https://cli.ouroboros.xyz/install.sh | sh
```
*Note: During development, this script builds from source using Cargo.*

### Proving & Running
To start your node with a persistent identity:
```bash
ouro register-node
ouro start
```

To register a wallet address for rewards:
```bash
ouro register-user --wallet-address <your-address>
```

### Hardware Benchmarking
Ouroboros features an adaptive difficulty system. To instantly optimize for your hardware:
```bash
ouro benchmark
```

## Adaptive Task Difficulty
The node automatically adjusts task difficulty based on your system's performance.

| Difficulty | Use Case | Multiplier |
|------------|----------|------------|
| small      | Default / Background | 1x |
| medium     | Standard Desktop     | 2x |
| large      | High-performance     | 4x |
| extra_large| Dedicated Proving    | 8x |

### Manual Override
You can override the adaptive system using flags:
```bash
ouro start --min-difficulty medium --max-difficulty extra_large
```

## Post-Quantum Security
Ouroboros is hardened against quantum attacks using **Dilithium5** hybrid signatures.
To enable Post-Quantum mode:
```bash
ENABLE_PQ_CRYPTO=true ouro start
```

## CLI Dashboard
Monitor your node in real-time:
```bash
ouro status --watch
```

## License
MIT
