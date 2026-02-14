# Shadow Mode Protocol Spec (Draft)

## Overview
When Heavy nodes (Validators) are offline, Medium nodes (Aggregators) form a "Shadow Quorum" to maintain provisional global settlement.

## Protocol Flow
1. **Heartbeat Monitoring**: Medium nodes listen for Block gossip from Heavy nodes.
2. **Emergency Trigger**: If no block is received for `HEAVY_TIMEOUT` (e.g., 30s), Medium nodes enter `SHADOW_STAGE_1`.
3. **Quorum Formation**:
   - Medium nodes broadcast `JOIN_SHADOW_QUORUM` messages.
   - If 2/3 of known Medium nodes respond, they form a Shadow Council.
4. **Shadow Consensus**:
   - The Council orders Cross-Subchain Settlement requests.
   - They sign a `ShadowCert` for each batch.
5. **Reconciliation**:
   - When a Heavy node returns, it requests the `ShadowLog` from the Council.
   - It verifies all `ShadowCert` signatures and anchors them to the Mainchain.
