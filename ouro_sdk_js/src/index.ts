/**
 * Ouroboros SDK for JavaScript/TypeScript
 *
 * Build decentralized applications on the Ouroboros blockchain platform.
 *
 * - Microchains: For dApps, games, NFTs, user wallets
 * - Subchains: For high-scale business infrastructure (payments, oracles, bridges)
 *
 * @packageDocumentation
 */

// Core classes
export { OuroClient } from './client';
export { Microchain, MicrochainBuilder } from './microchain';
export {
  Subchain,
  SubchainBuilder,
  SubchainState,
  MIN_SUBCHAIN_DEPOSIT,
  RENT_RATE_PER_BLOCK,
} from './subchain';
export type { SubchainConfig, SubchainStatus, ValidatorConfig } from './subchain';
export { Transaction, TransactionBuilder } from './transaction';

// Types
export type {
  MicrochainConfig,
  MicrochainState,
  Balance,
  BlockHeader,
  TransactionData,
  AnchorFrequency,
} from './types';

export { ConsensusType, TxStatus } from './types';

// Errors
export {
  SdkError,
  NetworkError,
  TransactionFailedError,
  MicrochainNotFoundError,
  InsufficientBalanceError,
  InvalidSignatureError,
  AnchorFailedError,
  InvalidConfigError,
} from './errors';

// Subchain imports for default export
import {
  Subchain,
  SubchainBuilder,
  SubchainState,
  MIN_SUBCHAIN_DEPOSIT,
  RENT_RATE_PER_BLOCK,
} from './subchain';

/**
 * Default export with all SDK exports
 */
export default {
  // Microchain (dApps)
  OuroClient,
  Microchain,
  MicrochainBuilder,
  Transaction,
  TransactionBuilder,
  ConsensusType,
  TxStatus,
  // Subchain (infrastructure)
  Subchain,
  SubchainBuilder,
  SubchainState,
  MIN_SUBCHAIN_DEPOSIT,
  RENT_RATE_PER_BLOCK,
};
