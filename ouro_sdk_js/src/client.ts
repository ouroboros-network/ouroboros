import axios, { AxiosInstance } from 'axios';
import {
  NetworkError,
  TransactionFailedError,
  AnchorFailedError,
  SdkError,
} from './errors';
import type {
  Balance,
  BalanceResponse,
  MicrochainBalanceResponse,
  TxSubmitResponse,
  TxStatusResponse,
  TxStatus,
  CreateMicrochainResponse,
  MicrochainState,
  ListMicrochainsResponse,
  AnchorResponse,
  MicrochainConfig,
  TransactionData,
} from './types';

/**
 * Main client for interacting with Ouroboros network
 */
export class OuroClient {
  private baseUrl: string;
  private client: AxiosInstance;
  private apiKey?: string;

  constructor(nodeUrl: string, apiKey?: string) {
    this.baseUrl = nodeUrl.replace(/\/$/, '');
    this.apiKey = apiKey;
    this.client = axios.create({
      baseURL: this.baseUrl,
      timeout: 30000,
      headers: {
        'Content-Type': 'application/json',
        ...(this.apiKey ? { 'Authorization': `Bearer ${this.apiKey}` } : {}),
      },
    });
  }

  /**
   * Get mainchain balance for address
   */
  async getBalance(address: string): Promise<Balance> {
    try {
      const response = await this.client.get<BalanceResponse>(
        `/balance/${address}`
      );

      return {
        address,
        balance: response.data.balance,
        pending: response.data.pending || 0,
      };
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Get microchain balance
   */
  async getMicrochainBalance(
    microchainId: string,
    address: string
  ): Promise<number> {
    try {
      const response = await this.client.get<MicrochainBalanceResponse>(
        `/microchain/${microchainId}/balance/${address}`
      );

      return response.data.balance;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Submit transaction to mainchain
   */
  async submitTransaction(tx: TransactionData): Promise<string> {
    try {
      const response = await this.client.post<TxSubmitResponse>(
        '/tx/submit',
        tx
      );

      if (response.data.success) {
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
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Get transaction status
   */
  async getTransactionStatus(txId: string): Promise<TxStatus> {
    try {
      const response = await this.client.get<TxStatusResponse>(`/tx/${txId}`);

      return response.data.status as TxStatus;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Create a new microchain
   */
  async createMicrochain(config: MicrochainConfig): Promise<string> {
    try {
      const response = await this.client.post<CreateMicrochainResponse>(
        '/microchain/create',
        config
      );

      if (response.data.success) {
        return response.data.microchain_id;
      } else {
        throw new SdkError(
          response.data.message || 'Failed to create microchain'
        );
      }
    } catch (error) {
      if (error instanceof SdkError) {
        throw error;
      }
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Get microchain state
   */
  async getMicrochainState(microchainId: string): Promise<MicrochainState> {
    try {
      const response = await this.client.get<MicrochainState>(
        `/microchain/${microchainId}/state`
      );

      return response.data;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * List all microchains
   */
  async listMicrochains(): Promise<MicrochainState[]> {
    try {
      const response = await this.client.get<ListMicrochainsResponse>(
        '/microchains'
      );

      return response.data.microchains;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Trigger manual anchor for a microchain
   */
  async anchorMicrochain(microchainId: string): Promise<string> {
    try {
      const response = await this.client.post<AnchorResponse>(
        `/microchain/${microchainId}/anchor`
      );

      if (response.data.success) {
        return response.data.anchor_id;
      } else {
        throw new AnchorFailedError(
          response.data.message || 'Unknown error'
        );
      }
    } catch (error) {
      if (error instanceof AnchorFailedError) {
        throw error;
      }
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Check node health
   */
  async healthCheck(): Promise<boolean> {
    try {
      const response = await this.client.get('/health');
      return response.status >= 200 && response.status < 300;
    } catch (error) {
      return false;
    }
  }

  /**
   * Get node resource usage (CPU, RAM, Disk, Network)
   */
  async getResources(): Promise<ResourcesResponse> {
    try {
      const response = await this.client.get<ResourcesResponse>('/resources');
      return response.data;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Get transaction history for an address
   */
  async getTransactionHistory(
    address: string,
    limit: number = 10
  ): Promise<TransactionData[]> {
    try {
      const response = await this.client.get<TxHistoryResponse>(
        `/ouro/transactions/${address}?limit=${limit}`
      );
      return response.data.transactions;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Get recent blocks
   */
  async getBlocks(limit: number = 10): Promise<BlockHeader[]> {
    try {
      const response = await this.client.get<BlocksResponse>(
        `/blocks?limit=${limit}`
      );
      return response.data.blocks;
    } catch (error) {
      throw new NetworkError(this.getErrorMessage(error));
    }
  }

  /**
   * Extract error message from error object
   */
  private getErrorMessage(error: any): string {
    if (axios.isAxiosError(error)) {
      return error.response?.data?.message || error.message;
    }
    return error?.message || 'Unknown error';
  }
}
