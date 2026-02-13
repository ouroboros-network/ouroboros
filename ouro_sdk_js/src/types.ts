/**
 * Consensus type for microchain
 */
export enum ConsensusType {
  /** Single validator (fast, centralized) */
  SingleValidator = 'single_validator',
  /** BFT consensus (slower, decentralized) */
  Bft = 'bft',
}

/**
 * Anchor frequency configuration
 */
export type AnchorFrequency =
  | { type: 'blocks'; count: number }
  | { type: 'seconds'; count: number }
  | { type: 'manual' };

/**
 * Configuration for creating a microchain
 */
export interface MicrochainConfig {
  /** Microchain name */
  name: string;
  /** Owner address */
  owner: string;
  /** Consensus type */
  consensus?: {
    type: ConsensusType;
    validatorCount?: number;
  };
  /** Anchor frequency */
  anchorFrequency?: AnchorFrequency;
  /** Maximum transactions per block */
  maxTxsPerBlock?: number;
  /** Block time in seconds */
  blockTimeSecs?: number;
}

/**
 * Microchain state information
 */
export interface MicrochainState {
  id: string;
  name: string;
  owner: string;
  blockHeight: number;
  txCount: number;
  lastAnchorHeight?: number;
  createdAt: string;
}

/**
 * Transaction status
 */
export enum TxStatus {
  Pending = 'pending',
  Confirmed = 'confirmed',
  Failed = 'failed',
  Anchored = 'anchored',
}

/**
 * Balance information
 */
export interface Balance {
  address: string;
  balance: number;
  pending: number;
}

/**
 * Block header
 */
export interface BlockHeader {
  height: number;
  hash: string;
  previousHash: string;
  timestamp: string;
  txCount: number;
}

/**
 * Transaction data
 */
export interface TransactionData {
  id: string;
  from: string;
  to: string;
  amount: number;
  nonce: number;
  signature: string;
  data?: Record<string, any>;
  timestamp?: string;
}

/**
 * Internal response types
 */
export interface BalanceResponse {
  balance: number;
  pending?: number;
}

export interface MicrochainBalanceResponse {
  balance: number;
}

export interface TxSubmitResponse {
  success: boolean;
  tx_id: string;
  message?: string;
}

export interface TxStatusResponse {
  status: string;
}

export interface CreateMicrochainResponse {
  success: boolean;
  microchain_id: string;
  message?: string;
}

export interface ListMicrochainsResponse {
  microchains: MicrochainState[];
}

export interface AnchorResponse {
  success: boolean;
  anchor_id: string;
  message?: string;
}

export interface TxHistoryResponse {
  transactions: TransactionData[];
}

export interface BlocksResponse {
  blocks: BlockHeader[];
}
