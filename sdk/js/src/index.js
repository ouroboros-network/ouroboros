'use strict';

/**
 * OuroClient — JavaScript SDK for the Ouroboros blockchain network.
 *
 * Works in Node.js 18+ (native fetch) and modern browsers.
 *
 * @example
 * const { OuroClient } = require('ouro-sdk');
 * const client = new OuroClient('http://localhost:8000', 'your-api-key');
 *
 * // Check node health
 * const health = await client.health();
 *
 * // Submit a transaction
 * const result = await client.submitTransaction({
 *   sender: 'ouro1abc...',
 *   recipient: 'ouro1xyz...',
 *   amount: 1000000,  // 0.001 OURO (in nanoouro)
 * });
 */
class OuroClient {
  /**
   * @param {string} [apiUrl='http://localhost:8000'] - Node API base URL
   * @param {string|null} [apiKey] - API key for protected endpoints (or set OURO_API_KEY env var)
   */
  constructor(apiUrl = 'http://localhost:8000', apiKey = null) {
    this.apiUrl = apiUrl.replace(/\/$/, '');
    this.apiKey = apiKey || (typeof process !== 'undefined' ? process.env.OURO_API_KEY : null) || null;
  }

  _headers() {
    const h = { 'Content-Type': 'application/json', 'Accept': 'application/json' };
    if (this.apiKey) h['Authorization'] = `Bearer ${this.apiKey}`;
    return h;
  }

  async _get(path) {
    const res = await fetch(`${this.apiUrl}${path}`, { headers: this._headers() });
    if (!res.ok) {
      const body = await res.text().catch(() => '');
      throw new Error(`GET ${path} failed: HTTP ${res.status} ${body}`);
    }
    return res.json();
  }

  async _post(path, body) {
    const res = await fetch(`${this.apiUrl}${path}`, {
      method: 'POST',
      headers: this._headers(),
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const errBody = await res.text().catch(() => '');
      throw new Error(`POST ${path} failed: HTTP ${res.status} ${errBody}`);
    }
    return res.json();
  }

  // ─── Public endpoints (no API key required) ────────────────────────

  /** GET /health — Node liveness check */
  async health() { return this._get('/health'); }

  /** GET /identity — Node ID, role, uptime, version */
  async identity() { return this._get('/identity'); }

  /** GET /consensus — Current view, leader, highest QC, last committed block */
  async consensus() { return this._get('/consensus'); }

  /** GET /peers — Connected peer list */
  async peers() { return this._get('/peers'); }

  /** GET /ouro/balance/:address — OURO coin balance for an address */
  async balance(address) {
    return this._get(`/ouro/balance/${encodeURIComponent(address)}`);
  }

  /** GET /ouro/nonce/:address — Current nonce for an address */
  async nonce(address) {
    return this._get(`/ouro/nonce/${encodeURIComponent(address)}`);
  }

  // ─── Protected endpoints (API key required) ────────────────────────

  /** GET /metrics/json — TPS, block height, peer count, mempool size, sync % */
  async metrics() { return this._get('/metrics/json'); }

  /** GET /resources — CPU %, memory MB, disk usage */
  async resources() { return this._get('/resources'); }

  /** GET /mempool — Current mempool contents */
  async mempool() { return this._get('/mempool'); }

  /** GET /network/stats — Network statistics */
  async networkStats() { return this._get('/network/stats'); }

  /**
   * GET /tx/:id — Get a transaction by UUID or hash
   * @param {string} id - Transaction UUID or hash
   */
  async getTransaction(id) {
    return this._get(`/tx/${encodeURIComponent(id)}`);
  }

  /**
   * POST /tx/submit — Submit a transaction to the node
   * @param {Object} tx
   * @param {string} tx.sender - Sender address
   * @param {string} tx.recipient - Recipient address
   * @param {number} tx.amount - Amount in nanoouro (1 OURO = 1,000,000,000)
   * @param {string} [tx.signature] - Ed25519 signature (hex)
   * @param {number} [tx.nonce] - Account nonce
   */
  async submitTransaction(tx) { return this._post('/tx/submit', tx); }

  /**
   * POST /ouro/transfer — Transfer OURO coins between addresses
   * @param {string} from - Sender address
   * @param {string} to - Recipient address
   * @param {number} amount - Amount in nanoouro
   */
  async transfer(from, to, amount) {
    return this._post('/ouro/transfer', { from, to, amount });
  }

  // ─── Convenience methods ───────────────────────────────────────────

  /**
   * Get a combined snapshot of node state (health + identity + consensus + metrics).
   * Never throws — failed sub-requests return null.
   * @returns {Promise<{online: boolean, identity: Object|null, consensus: Object|null, metrics: Object|null}>}
   */
  async status() {
    const [health, identity, consensus, metrics] = await Promise.allSettled([
      this.health(),
      this.identity(),
      this.consensus(),
      this.metrics(),
    ]);
    return {
      online:    health.status === 'fulfilled',
      identity:  identity.value  ?? null,
      consensus: consensus.value ?? null,
      metrics:   metrics.value   ?? null,
    };
  }

  /**
   * Wait until the node is online, polling every `intervalMs` milliseconds.
   * @param {number} [timeoutMs=30000] - Max wait time in ms
   * @param {number} [intervalMs=1000] - Poll interval in ms
   */
  async waitForNode(timeoutMs = 30000, intervalMs = 1000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        await this.health();
        return true;
      } catch (_) {
        await new Promise(r => setTimeout(r, intervalMs));
      }
    }
    throw new Error(`Node at ${this.apiUrl} did not come online within ${timeoutMs}ms`);
  }
}

module.exports = { OuroClient };
