/**
 * Base SDK Error class
 */
export class SdkError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'SdkError';
    Object.setPrototypeOf(this, SdkError.prototype);
  }
}

/**
 * Network error
 */
export class NetworkError extends SdkError {
  constructor(message: string) {
    super(`Network error: ${message}`);
    this.name = 'NetworkError';
    Object.setPrototypeOf(this, NetworkError.prototype);
  }
}

/**
 * Transaction failed error
 */
export class TransactionFailedError extends SdkError {
  constructor(message: string) {
    super(`Transaction failed: ${message}`);
    this.name = 'TransactionFailedError';
    Object.setPrototypeOf(this, TransactionFailedError.prototype);
  }
}

/**
 * Microchain not found error
 */
export class MicrochainNotFoundError extends SdkError {
  constructor(microchainId: string) {
    super(`Microchain not found: ${microchainId}`);
    this.name = 'MicrochainNotFoundError';
    Object.setPrototypeOf(this, MicrochainNotFoundError.prototype);
  }
}

/**
 * Insufficient balance error
 */
export class InsufficientBalanceError extends SdkError {
  required: number;
  available: number;

  constructor(required: number, available: number) {
    super(`Insufficient balance: required ${required}, available ${available}`);
    this.name = 'InsufficientBalanceError';
    this.required = required;
    this.available = available;
    Object.setPrototypeOf(this, InsufficientBalanceError.prototype);
  }
}

/**
 * Invalid signature error
 */
export class InvalidSignatureError extends SdkError {
  constructor() {
    super('Invalid signature');
    this.name = 'InvalidSignatureError';
    Object.setPrototypeOf(this, InvalidSignatureError.prototype);
  }
}

/**
 * Anchor failed error
 */
export class AnchorFailedError extends SdkError {
  constructor(message: string) {
    super(`Anchor failed: ${message}`);
    this.name = 'AnchorFailedError';
    Object.setPrototypeOf(this, AnchorFailedError.prototype);
  }
}

/**
 * Invalid configuration error
 */
export class InvalidConfigError extends SdkError {
  constructor(message: string) {
    super(`Invalid configuration: ${message}`);
    this.name = 'InvalidConfigError';
    Object.setPrototypeOf(this, InvalidConfigError.prototype);
  }
}
