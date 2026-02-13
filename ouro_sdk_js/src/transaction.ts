import { v4 as uuidv4 } from 'uuid';
import * as nacl from 'tweetnacl';
import { encodeBase64, decodeUTF8 } from 'tweetnacl-util';
import { InvalidConfigError, InvalidSignatureError } from './errors';
import type { TransactionData } from './types';

/**
 * Transaction class for building and signing transactions
 */
export class Transaction {
  id: string;
  from: string;
  to: string;
  amount: number;
  nonce: number;
  signature: string;
  data?: Record<string, any>;
  timestamp?: string;

  constructor(from: string, to: string, amount: number) {
    this.id = uuidv4();
    this.from = from;
    this.to = to;
    this.amount = amount;
    this.nonce = 0;
    this.signature = '';
    this.timestamp = new Date().toISOString();
  }

  /**
   * Set transaction nonce
   */
  withNonce(nonce: number): this {
    this.nonce = nonce;
    return this;
  }

  /**
   * Add custom data to transaction
   */
  withData(data: Record<string, any>): this {
    this.data = data;
    return this;
  }

  /**
   * Sign transaction with private key (hex string)
   */
  sign(privateKeyHex: string): this {
    try {
      const privateKeyBytes = Buffer.from(privateKeyHex, 'hex');
      if (privateKeyBytes.length !== 64) {
        throw new InvalidSignatureError();
      }

      const keypair = nacl.sign.keyPair.fromSecretKey(privateKeyBytes);
      const message = this.getSigningMessage();
      const messageBytes = decodeUTF8(message);
      const signatureBytes = nacl.sign.detached(messageBytes, keypair.secretKey);

      this.signature = Buffer.from(signatureBytes).toString('hex');
      return this;
    } catch (error) {
      throw new InvalidSignatureError();
    }
  }

  /**
   * Get signing message
   */
  private getSigningMessage(): string {
    return `${this.id}:${this.from}:${this.to}:${this.amount}:${this.nonce}`;
  }

  /**
   * Convert to JSON for API submission
   */
  toJSON(): TransactionData {
    return {
      id: this.id,
      from: this.from,
      to: this.to,
      amount: this.amount,
      nonce: this.nonce,
      signature: this.signature,
      data: this.data,
      timestamp: this.timestamp,
    };
  }

  /**
   * Create from JSON data
   */
  static fromJSON(data: TransactionData): Transaction {
    const tx = new Transaction(data.from, data.to, data.amount);
    tx.id = data.id;
    tx.nonce = data.nonce;
    tx.signature = data.signature;
    tx.data = data.data;
    tx.timestamp = data.timestamp;
    return tx;
  }
}

/**
 * Builder for creating transactions
 */
export class TransactionBuilder {
  private from?: string;
  private to?: string;
  private amount?: number;
  private nonce: number = 0;
  private data?: Record<string, any>;

  /**
   * Set sender address
   */
  setFrom(from: string): this {
    this.from = from;
    return this;
  }

  /**
   * Set recipient address
   */
  setTo(to: string): this {
    this.to = to;
    return this;
  }

  /**
   * Set amount
   */
  setAmount(amount: number): this {
    this.amount = amount;
    return this;
  }

  /**
   * Set nonce
   */
  setNonce(nonce: number): this {
    this.nonce = nonce;
    return this;
  }

  /**
   * Add custom data
   */
  setData(data: Record<string, any>): this {
    this.data = data;
    return this;
  }

  /**
   * Build transaction
   */
  build(): Transaction {
    if (!this.from) {
      throw new InvalidConfigError("Missing 'from' address");
    }
    if (!this.to) {
      throw new InvalidConfigError("Missing 'to' address");
    }
    if (this.amount === undefined) {
      throw new InvalidConfigError('Missing amount');
    }

    const tx = new Transaction(this.from, this.to, this.amount);
    tx.nonce = this.nonce;
    if (this.data) {
      tx.data = this.data;
    }

    return tx;
  }
}
