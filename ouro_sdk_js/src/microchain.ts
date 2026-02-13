import { OuroClient } from './client';
import { Transaction, TransactionBuilder } from './transaction';
import { InvalidConfigError, TransactionFailedError } from './errors';
import type {
  MicrochainConfig,
  MicrochainState,
  BlockHeader,
  TransactionData,
  TxHistoryResponse,
  BlocksResponse,
  ConsensusType,
  AnchorFrequency,
} from './types';
import axios from 'axios';

/**
 * Microchain interface for building dApps
 */
export class Microchain {
  /** Microchain ID */
  public id: string;

  /** Client for node communication */
  private client: OuroClient;

  /** Current nonce for transactions (auto-incremented) */
  private nonce: number;

  /** Base URL for direct API calls */
  private baseUrl: string;

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
   * Connect to an existing microchain
   */
  static async connect(
    microchainId: string,
    nodeUrl: string
  ): Promise<Microchain> {
    const client = new OuroClient(nodeUrl);

    // Verify microchain exists
    await client.getMicrochainState(microchainId);

    return new Microchain(microchainId, client, nodeUrl, 0);
  }

  /**
   * Create a new microchain
   */
  static async create(
    config: MicrochainConfig,
    nodeUrl: string
  ): Promise<Microchain> {
    const client = new OuroClient(nodeUrl);
    const id = await client.createMicrochain(config);

    return new Microchain(id, client, nodeUrl, 0);
  }

  /**
   * Get microchain state
   */
  async state(): Promise<MicrochainState> {
    return this.client.getMicrochainState(this.id);
  }

  /**
   * Get balance for an address on this microchain
   */
  async balance(address: string): Promise<number> {
    return this.client.getMicrochainBalance(this.id, address);
  }

  /**
   * Submit a transaction to this microchain
   */
  async submitTx(tx: Transaction): Promise<string> {
    try {
      const url = `${this.baseUrl}/microchain/${this.id}/tx`;
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
   * Create a transaction builder for this microchain
   */
  tx(): TransactionBuilder {
    const builder = new TransactionBuilder();
    builder.setNonce(this.nonce);
    return builder;
  }

  /**
   * Transfer tokens on this microchain (simplified)
   */
  async transfer(from: string, to: string, amount: number): Promise<string> {
    const tx = new Transaction(from, to, amount);
    tx.nonce = this.nonce;

    return this.submitTx(tx);
  }

  /**
   * Anchor this microchain to subchain/mainchain
   */
  async anchor(): Promise<string> {
    return this.client.anchorMicrochain(this.id);
  }

  /**
   * Get transaction history for this microchain
   */
  async txHistory(from: number, to: number): Promise<TransactionData[]> {
    try {
      const url = `${this.baseUrl}/microchain/${this.id}/txs?from=${from}&to=${to}`;
      const response = await axios.get<TxHistoryResponse>(url);

      return response.data.transactions;
    } catch (error) {
      throw new Error(
        error instanceof Error ? error.message : 'Failed to fetch history'
      );
    }
  }

  /**
   * Get latest blocks from this microchain
   */
  async blocks(limit: number): Promise<BlockHeader[]> {
    try {
      const url = `${this.baseUrl}/microchain/${this.id}/blocks?limit=${limit}`;
      const response = await axios.get<BlocksResponse>(url);

      return response.data.blocks;
    } catch (error) {
      throw new Error(
        error instanceof Error ? error.message : 'Failed to fetch blocks'
      );
    }
  }
}

/**
 * Builder for creating microchains
 */
export class MicrochainBuilder {
  private config: MicrochainConfig;
  private nodeUrl?: string;

  constructor(name: string, owner: string) {
    this.config = {
      name,
      owner,
      consensus: {
        type: ConsensusType.SingleValidator,
      },
      anchorFrequency: { type: 'blocks', count: 100 },
      maxTxsPerBlock: 1000,
      blockTimeSecs: 5,
    };
  }

  /**
   * Set node URL
   */
  node(url: string): this {
    this.nodeUrl = url;
    return this;
  }

  /**
   * Set consensus type
   */
  consensus(
    type: ConsensusType,
    validatorCount?: number
  ): this {
    this.config.consensus = {
      type,
      validatorCount,
    };
    return this;
  }

  /**
   * Set anchor frequency
   */
  anchorFrequency(frequency: AnchorFrequency): this {
    this.config.anchorFrequency = frequency;
    return this;
  }

  /**
   * Set block time
   */
  blockTime(seconds: number): this {
    this.config.blockTimeSecs = seconds;
    return this;
  }

  /**
   * Build and create the microchain
   */
  async build(): Promise<Microchain> {
    if (!this.nodeUrl) {
      throw new InvalidConfigError('Node URL not specified');
    }

    return Microchain.create(this.config, this.nodeUrl);
  }
}
