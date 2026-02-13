import { OuroClient } from './client';
import { Transaction, TransactionBuilder } from './transaction';
import { InvalidConfigError, TransactionFailedError } from './errors';
import axios from 'axios';

/** Minimum deposit required to create a subchain (5,000 OURO) */
export const MIN_SUBCHAIN_DEPOSIT = 500_000_000_000;

/** Rent rate per block (0.0001 OURO) */
export const RENT_RATE_PER_BLOCK = 10_000;

/** Subchain state */
export enum SubchainState {
  /** Active and operational */
  Active = 'active',
  /** In grace period (rent depleted) */
  GracePeriod = 'grace_period',
  /** Terminated */
  Terminated = 'terminated',
}

/** Validator configuration for subchain */
export interface ValidatorConfig {
  /** Validator public key */
  pubkey: string;
  /** Validator stake amount */
  stake: number;
  /** Validator endpoint */
  endpoint?: string;
}

/** Subchain configuration */
export interface SubchainConfig {
  /** Subchain name */
  name: string;
  /** Owner address */
  owner: string;
  /** Initial deposit amount (must be >= MIN_SUBCHAIN_DEPOSIT) */
  deposit: number;
  /** Anchor frequency (blocks) */
  anchorFrequency: number;
  /** RPC endpoint for the subchain */
  rpcEndpoint?: string;
  /** Validators (for BFT consensus) */
  validators: ValidatorConfig[];
}

/** Subchain status information */
export interface SubchainStatus {
  /** Subchain ID */
  id: string;
  /** Subchain name */
  name: string;
  /** Owner address */
  owner: string;
  /** Current state */
  state: SubchainState;
  /** Current deposit balance */
  depositBalance: number;
  /** Estimated blocks remaining before rent runs out */
  blocksRemaining: number;
  /** Current block height */
  blockHeight: number;
  /** Total transactions processed */
  txCount: number;
  /** Last anchor to mainchain */
  lastAnchorHeight?: number;
  /** Number of active validators */
  validatorCount: number;
}

/** Transaction history response */
interface TxHistoryResponse {
  transactions: any[];
}

/**
 * Subchain interface for building high-scale business applications
 *
 * Subchains are designed for:
 * - Financial infrastructure (money transfer, payments)
 * - High-throughput services (oracles, bridges)
 * - Enterprise applications with dedicated resources
 *
 * Requirements:
 * - Minimum deposit: 5,000 OURO
 * - Rent: 0.01 OURO per block
 */
export class Subchain {
  /** Subchain ID */
  public id: string;

  /** Client for node communication */
  private client: OuroClient;

  /** Base URL for direct API calls */
  private baseUrl: string;

  /** Current nonce for transactions (auto-incremented) */
  private nonce: number;

  private constructor(
    id: string,
    client: OuroClient,
    baseUrl: string,
    nonce: number = 0
  ) {
    this.id = id;
    this.client = client;
    this.baseUrl = baseUrl;
    this.nonce = nonce;
  }

  /**
   * Connect to an existing subchain
   * @param subchainId - The subchain ID to connect to
   * @param nodeUrl - Node URL to connect through
   */
  static async connect(subchainId: string, nodeUrl: string): Promise<Subchain> {
    const client = new OuroClient(nodeUrl);

    // Verify subchain exists by fetching status
    const response = await axios.get(`${nodeUrl}/subchain/${subchainId}/status`);
    if (!response.data.success) {
      throw new Error(`Subchain not found: ${subchainId}`);
    }

    return new Subchain(subchainId, client, nodeUrl, 0);
  }

  /**
   * Register a new subchain
   * @param config - Subchain configuration
   * @param nodeUrl - Node URL to register through
   */
  static async register(
    config: SubchainConfig,
    nodeUrl: string
  ): Promise<Subchain> {
    // Validate configuration
    if (!config.name || config.name.length === 0 || config.name.length > 64) {
      throw new InvalidConfigError('Name must be 1-64 characters');
    }
    if (config.deposit < MIN_SUBCHAIN_DEPOSIT) {
      throw new InvalidConfigError(
        `Deposit must be at least ${MIN_SUBCHAIN_DEPOSIT / 100_000_000} OURO`
      );
    }

    const client = new OuroClient(nodeUrl);

    // Register subchain
    const response = await axios.post(`${nodeUrl}/subchain/register`, {
      name: config.name,
      owner: config.owner,
      deposit: config.deposit,
      anchor_frequency: config.anchorFrequency,
      rpc_endpoint: config.rpcEndpoint,
      validators: config.validators,
    });

    if (!response.data.success) {
      throw new Error(response.data.message || 'Failed to register subchain');
    }

    return new Subchain(response.data.subchain_id, client, nodeUrl, 0);
  }

  /**
   * Get subchain status
   */
  async status(): Promise<SubchainStatus> {
    const response = await axios.get(`${this.baseUrl}/subchain/${this.id}/status`);
    const data = response.data;

    return {
      id: data.id,
      name: data.name,
      owner: data.owner,
      state: data.state as SubchainState,
      depositBalance: data.deposit_balance,
      blocksRemaining: data.blocks_remaining,
      blockHeight: data.block_height,
      txCount: data.tx_count,
      lastAnchorHeight: data.last_anchor_height,
      validatorCount: data.validator_count,
    };
  }

  /**
   * Get current deposit balance
   */
  async depositBalance(): Promise<number> {
    const status = await this.status();
    return status.depositBalance;
  }

  /**
   * Get estimated blocks remaining before rent runs out
   */
  async blocksRemaining(): Promise<number> {
    const status = await this.status();
    return status.blocksRemaining;
  }

  /**
   * Top up rent deposit
   * @param amount - Amount to add to deposit
   */
  async topUpRent(amount: number): Promise<string> {
    const response = await axios.post(`${this.baseUrl}/subchain/${this.id}/topup`, {
      amount,
    });

    if (!response.data.success) {
      throw new Error(response.data.message || 'Failed to top up rent');
    }

    return response.data.tx_id;
  }

  /**
   * Get balance for an address on this subchain
   * @param address - Address to check
   */
  async balance(address: string): Promise<number> {
    const response = await axios.get(
      `${this.baseUrl}/subchain/${this.id}/balance/${address}`
    );
    return response.data.balance;
  }

  /**
   * Submit a transaction to this subchain
   * @param tx - Transaction to submit
   */
  async submitTx(tx: Transaction): Promise<string> {
    try {
      const url = `${this.baseUrl}/subchain/${this.id}/tx`;
      const response = await axios.post(url, tx.toJSON());

      if (response.data.success) {
        this.nonce += 1;
        return response.data.tx_id;
      } else {
        throw new TransactionFailedError(
          response.data.message || 'Unknown error'
        );
      }
    } catch (error) {
      if (error instanceof TransactionFailedError) {
        throw error;
      }
      throw new TransactionFailedError(
        error instanceof Error ? error.message : 'Unknown error'
      );
    }
  }

  /**
   * Create a transaction builder for this subchain
   */
  tx(): TransactionBuilder {
    const builder = new TransactionBuilder();
    builder.setNonce(this.nonce);
    return builder;
  }

  /**
   * Transfer tokens on this subchain
   * @param from - Sender address
   * @param to - Recipient address
   * @param amount - Amount to transfer
   */
  async transfer(from: string, to: string, amount: number): Promise<string> {
    const tx = new Transaction(from, to, amount);
    tx.nonce = this.nonce;
    return this.submitTx(tx);
  }

  /**
   * Anchor this subchain to mainchain
   */
  async anchor(): Promise<string> {
    const response = await axios.post(`${this.baseUrl}/subchain/${this.id}/anchor`);

    if (!response.data.success) {
      throw new Error(response.data.message || 'Failed to anchor');
    }

    return response.data.tx_id;
  }

  /**
   * Get transaction history
   * @param from - Start block
   * @param to - End block
   */
  async txHistory(from: number, to: number): Promise<any[]> {
    const url = `${this.baseUrl}/subchain/${this.id}/txs?from=${from}&to=${to}`;
    const response = await axios.get<TxHistoryResponse>(url);
    return response.data.transactions;
  }

  /**
   * Add a validator to the subchain
   * @param validator - Validator configuration
   */
  async addValidator(validator: ValidatorConfig): Promise<string> {
    const response = await axios.post(
      `${this.baseUrl}/subchain/${this.id}/validators`,
      validator
    );

    if (!response.data.success) {
      throw new Error(response.data.message || 'Failed to add validator');
    }

    return response.data.tx_id;
  }

  /**
   * Remove a validator from the subchain
   * @param pubkey - Validator public key to remove
   */
  async removeValidator(pubkey: string): Promise<string> {
    const response = await axios.delete(
      `${this.baseUrl}/subchain/${this.id}/validators/${pubkey}`
    );

    if (!response.data.success) {
      throw new Error(response.data.message || 'Failed to remove validator');
    }

    return response.data.tx_id;
  }

  /**
   * Get list of validators
   */
  async validators(): Promise<ValidatorConfig[]> {
    const response = await axios.get(
      `${this.baseUrl}/subchain/${this.id}/validators`
    );
    return response.data.validators;
  }

  /**
   * Withdraw deposit (only after termination)
   */
  async withdrawDeposit(): Promise<string> {
    const response = await axios.post(
      `${this.baseUrl}/subchain/${this.id}/withdraw`
    );

    if (!response.data.success) {
      throw new Error(response.data.message || 'Failed to withdraw deposit');
    }

    return response.data.tx_id;
  }
}

/**
 * Builder for creating subchains
 *
 * @example
 * ```typescript
 * const subchain = await new SubchainBuilder('Hermes', 'ouro1owner...')
 *   .node('http://localhost:8001')
 *   .deposit(1_000_000_000_000) // 10,000 OURO
 *   .anchorFrequency(50)
 *   .validator('validator1_pubkey', 100_000_000_000)
 *   .build();
 * ```
 */
export class SubchainBuilder {
  private config: SubchainConfig;
  private nodeUrl?: string;

  constructor(name: string, owner: string) {
    this.config = {
      name,
      owner,
      deposit: MIN_SUBCHAIN_DEPOSIT,
      anchorFrequency: 100,
      validators: [],
    };
  }

  /**
   * Set node URL
   * @param url - Node URL to connect to
   */
  node(url: string): this {
    this.nodeUrl = url;
    return this;
  }

  /**
   * Set deposit amount
   * @param amount - Deposit in base units (1 OURO = 100,000,000)
   */
  deposit(amount: number): this {
    this.config.deposit = amount;
    return this;
  }

  /**
   * Set anchor frequency
   * @param frequency - Number of blocks between anchors
   */
  anchorFrequency(frequency: number): this {
    this.config.anchorFrequency = frequency;
    return this;
  }

  /**
   * Set RPC endpoint
   * @param endpoint - RPC endpoint URL
   */
  rpcEndpoint(endpoint: string): this {
    this.config.rpcEndpoint = endpoint;
    return this;
  }

  /**
   * Add a validator
   * @param pubkey - Validator public key
   * @param stake - Stake amount
   * @param endpoint - Optional validator endpoint
   */
  validator(pubkey: string, stake: number, endpoint?: string): this {
    this.config.validators.push({ pubkey, stake, endpoint });
    return this;
  }

  /**
   * Build and register the subchain
   */
  async build(): Promise<Subchain> {
    if (!this.nodeUrl) {
      throw new InvalidConfigError('Node URL not specified');
    }

    return Subchain.register(this.config, this.nodeUrl);
  }
}
