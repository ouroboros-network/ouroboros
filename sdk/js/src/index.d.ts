export interface HealthResponse {
  status: string;
  timestamp?: string;
  node_name?: string;
}

export interface IdentityResponse {
  node_id: string;
  role: string;
  public_name: string;
  total_uptime_secs: number;
  difficulty: string;
  version: string;
}

export interface ConsensusResponse {
  view: number;
  leader: string;
  highest_qc_view: number;
  last_committed: {
    height: number;
    timestamp: string;
  };
}

export interface MetricsResponse {
  tps_1m: number;
  tps_5m: number;
  transactions_total: number;
  peer_connections: number;
  peer_count: number;
  block_height: number;
  network_tip: number;
  sync_percent: number;
  mempool_count: number;
  uptime_secs: number;
}

export interface PeersResponse {
  count: number;
  peers: Array<{
    id: string;
    addr: string;
    role: string;
    latency_ms: number;
  }>;
}

export interface BalanceResponse {
  address: string;
  balance: number;
  nonce?: number;
}

export interface Transaction {
  sender: string;
  recipient: string;
  amount: number;
  signature?: string;
  nonce?: number;
  idempotency_key?: string;
}

export interface TxSubmitResponse {
  tx_id: string;
  status: string;
}

export interface StatusSnapshot {
  online: boolean;
  identity: IdentityResponse | null;
  consensus: ConsensusResponse | null;
  metrics: MetricsResponse | null;
}

export declare class OuroClient {
  constructor(apiUrl?: string, apiKey?: string | null);

  health(): Promise<HealthResponse>;
  identity(): Promise<IdentityResponse>;
  consensus(): Promise<ConsensusResponse>;
  peers(): Promise<PeersResponse>;
  balance(address: string): Promise<BalanceResponse>;
  nonce(address: string): Promise<{ address: string; nonce: number }>;

  metrics(): Promise<MetricsResponse>;
  resources(): Promise<{ cpu_pct: number; mem_mb: number; disk_used_gb: number; disk_total_gb: number }>;
  mempool(): Promise<{ count: number; transactions: Transaction[] }>;
  networkStats(): Promise<Record<string, unknown>>;
  getTransaction(id: string): Promise<Record<string, unknown>>;
  submitTransaction(tx: Transaction): Promise<TxSubmitResponse>;
  transfer(from: string, to: string, amount: number): Promise<Record<string, unknown>>;

  status(): Promise<StatusSnapshot>;
  waitForNode(timeoutMs?: number, intervalMs?: number): Promise<boolean>;
}
