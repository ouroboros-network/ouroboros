// src/vm/host_functions.rs
//! Host functions - Bridge between WASM contracts and native Rust
//!
//! These functions are callable from WASM contracts and execute at native speed.

use super::gas::{copy_gas_cost, hash_gas_cost, GasState};
use super::precompiles::Precompiles;
use super::storage::ContractStorage;
use super::types::{ContractAddress, ContractLog, StorageKey};
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::Arc;
use wasmi::{Caller, Linker, Memory};

/// Host function context shared with WASM
pub struct HostContext {
    /// Contract storage
    pub storage: Arc<ContractStorage>,

    /// Current contract address
    pub contract_address: ContractAddress,

    /// Caller address
    pub caller: String,

    /// Logs emitted during execution
    pub logs: Arc<Mutex<Vec<ContractLog>>>,

    /// Current block number
    pub block_number: u64,

    /// Current block timestamp
    pub block_timestamp: u64,
}

impl HostContext {
    pub fn new(
        storage: Arc<ContractStorage>,
        contract_address: ContractAddress,
        caller: String,
        block_number: u64,
        block_timestamp: u64,
    ) -> Self {
        Self {
            storage,
            contract_address,
            caller,
            logs: Arc::new(Mutex::new(Vec::new())),
            block_number,
            block_timestamp,
        }
    }
}

/// Register all host functions with the WASM linker
pub fn register_host_functions(
    linker: &mut Linker<GasState>,
    context: Arc<HostContext>,
) -> Result<()> {
    // Storage operations
    register_storage_functions(linker, context.clone())?;

    // Crypto precompiles
    register_crypto_functions(linker)?;

    // Environment functions
    register_env_functions(linker, context.clone())?;

    // Logging functions
    register_log_functions(linker, context.clone())?;

    // Utility functions
    register_utility_functions(linker)?;

    Ok(())
}

/// Register storage operations
fn register_storage_functions(
    linker: &mut Linker<GasState>,
    context: Arc<HostContext>,
) -> Result<()> {
    // storage_set(key_ptr: u32, key_len: u32, value_ptr: u32, value_len: u32)
    let ctx = context.clone();
    linker.func_wrap(
        "env",
        "storage_set",
        move |mut caller: Caller<GasState>,
              key_ptr: u32,
              key_len: u32,
              value_ptr: u32,
              value_len: u32|
              -> Result<(), wasmi::core::Trap> {
            // Charge gas
            let gas_cost = caller.data().costs.storage_set;
            caller
                .data_mut()
                .consume_gas(gas_cost)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            // Read key and value from WASM memory
            let memory = get_memory(&caller)?;
            let key_bytes = read_memory(&caller, &memory, key_ptr, key_len)?;
            let value_bytes = read_memory(&caller, &memory, value_ptr, value_len)?;

            // Convert to storage key (hash the key bytes to get 32-byte key)
            let key_hash = Precompiles::sha256(&key_bytes);
            let storage_key = StorageKey::new(ctx.contract_address, key_hash);

            // Store
            ctx.storage
                .set_storage(storage_key, value_bytes)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            Ok(())
        },
    )?;

    // storage_get(key_ptr: u32, key_len: u32, value_ptr: u32) -> u32 (returns value length)
    let ctx = context.clone();
    linker.func_wrap(
        "env",
        "storage_get",
        move |mut caller: Caller<GasState>,
              key_ptr: u32,
              key_len: u32,
              value_ptr: u32|
              -> Result<u32, wasmi::core::Trap> {
            // Charge gas
            let gas_cost = caller.data().costs.storage_get;
            caller
                .data_mut()
                .consume_gas(gas_cost)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            // Read key from WASM memory
            let memory = get_memory(&caller)?;
            let key_bytes = read_memory(&caller, &memory, key_ptr, key_len)?;

            // Convert to storage key
            let key_hash = Precompiles::sha256(&key_bytes);
            let storage_key = StorageKey::new(ctx.contract_address, key_hash);

            // Get value
            match ctx
                .storage
                .get_storage(&storage_key)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?
            {
                Some(value) => {
                    // Write value to WASM memory
                    write_memory(&mut caller, &memory, value_ptr, &value)?;
                    Ok(value.len() as u32)
                }
                None => Ok(0), // Not found
            }
        },
    )?;

    Ok(())
}

/// Register crypto precompiles
fn register_crypto_functions(linker: &mut Linker<GasState>) -> Result<()> {
    // sha256(data_ptr: u32, data_len: u32, output_ptr: u32)
    linker.func_wrap(
        "env",
        "sha256",
        |mut caller: Caller<GasState>,
         data_ptr: u32,
         data_len: u32,
         output_ptr: u32|
         -> Result<(), wasmi::core::Trap> {
            // Charge gas
            let gas_cost = hash_gas_cost(
                caller.data().costs.sha256_base,
                caller.data().costs.sha256_per_word,
                data_len as u64,
            );
            caller
                .data_mut()
                .consume_gas(gas_cost)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            // Read data
            let memory = get_memory(&caller)?;
            let data = read_memory(&caller, &memory, data_ptr, data_len)?;

            // Compute hash at NATIVE speed
            let hash = Precompiles::sha256(&data);

            // Write output
            write_memory(&mut caller, &memory, output_ptr, &hash)?;

            Ok(())
        },
    )?;

    // keccak256(data_ptr: u32, data_len: u32, output_ptr: u32)
    linker.func_wrap(
        "env",
        "keccak256",
        |mut caller: Caller<GasState>,
         data_ptr: u32,
         data_len: u32,
         output_ptr: u32|
         -> Result<(), wasmi::core::Trap> {
            let gas_cost = hash_gas_cost(
                caller.data().costs.keccak256_base,
                caller.data().costs.keccak256_per_word,
                data_len as u64,
            );
            caller
                .data_mut()
                .consume_gas(gas_cost)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            let memory = get_memory(&caller)?;
            let data = read_memory(&caller, &memory, data_ptr, data_len)?;
            let hash = Precompiles::keccak256(&data);
            write_memory(&mut caller, &memory, output_ptr, &hash)?;

            Ok(())
        },
    )?;

    // ed25519_verify(pubkey_ptr: u32, sig_ptr: u32, msg_ptr: u32, msg_len: u32) -> u32
    linker.func_wrap(
        "env",
        "ed25519_verify",
        |mut caller: Caller<GasState>,
         pubkey_ptr: u32,
         sig_ptr: u32,
         msg_ptr: u32,
         msg_len: u32|
         -> Result<u32, wasmi::core::Trap> {
            let gas_cost = caller.data().costs.ed25519_verify;
            caller
                .data_mut()
                .consume_gas(gas_cost)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            let memory = get_memory(&caller)?;
            let pubkey = read_memory(&caller, &memory, pubkey_ptr, 32)?;
            let signature = read_memory(&caller, &memory, sig_ptr, 64)?;
            let message = read_memory(&caller, &memory, msg_ptr, msg_len)?;

            // Verify at NATIVE speed
            match Precompiles::ed25519_verify(&pubkey, &signature, &message) {
                Ok(true) => Ok(1),
                Ok(false) => Ok(0),
                Err(_) => Ok(0),
            }
        },
    )?;

    Ok(())
}

/// Register environment functions (block info, caller info)
fn register_env_functions(linker: &mut Linker<GasState>, context: Arc<HostContext>) -> Result<()> {
    // get_caller(output_ptr: u32) - Write caller address to output
    let ctx = context.clone();
    linker.func_wrap(
        "env",
        "get_caller",
        move |mut caller: Caller<GasState>, output_ptr: u32| -> Result<u32, wasmi::core::Trap> {
            let memory = get_memory(&caller)?;
            let caller_bytes = ctx.caller.as_bytes();
            write_memory(&mut caller, &memory, output_ptr, caller_bytes)?;
            Ok(caller_bytes.len() as u32)
        },
    )?;

    // get_block_number() -> u64
    let ctx = context.clone();
    linker.func_wrap(
        "env",
        "get_block_number",
        move |_caller: Caller<GasState>| -> u64 { ctx.block_number },
    )?;

    // get_block_timestamp() -> u64
    let ctx = context.clone();
    linker.func_wrap(
        "env",
        "get_block_timestamp",
        move |_caller: Caller<GasState>| -> u64 { ctx.block_timestamp },
    )?;

    Ok(())
}

/// Register logging functions
fn register_log_functions(linker: &mut Linker<GasState>, context: Arc<HostContext>) -> Result<()> {
    // log(data_ptr: u32, data_len: u32)
    let ctx = context.clone();
    linker.func_wrap(
        "env",
        "log",
        move |mut caller: Caller<GasState>,
              data_ptr: u32,
              data_len: u32|
              -> Result<(), wasmi::core::Trap> {
            // Charge gas for logging (8 gas per byte)
            caller
                .data_mut()
                .consume_gas(data_len as u64 * 8)
                .map_err(|e| wasmi::core::Trap::new(e.to_string()))?;

            let memory = get_memory(&caller)?;
            let data = read_memory(&caller, &memory, data_ptr, data_len)?;

            let log = ContractLog {
                contract_address: ctx.contract_address,
                topics: vec![],
                data,
                block_number: ctx.block_number,
                tx_index: 0,
            };

            ctx.logs.lock().push(log);

            Ok(())
        },
    )?;

    Ok(())
}

/// Register utility functions
fn register_utility_functions(linker: &mut Linker<GasState>) -> Result<()> {
    // revert(msg_ptr: u32, msg_len: u32) - Abort execution with error
    linker.func_wrap(
        "env",
        "revert",
        |caller: Caller<GasState>, msg_ptr: u32, msg_len: u32| -> Result<(), wasmi::core::Trap> {
            let memory = get_memory(&caller)?;
            let msg_bytes = read_memory(&caller, &memory, msg_ptr, msg_len)?;
            let msg = String::from_utf8_lossy(&msg_bytes);
            Err(wasmi::core::Trap::new(format!(
                "Contract reverted: {}",
                msg
            )))
        },
    )?;

    Ok(())
}

// Helper functions for memory access

fn get_memory(caller: &Caller<GasState>) -> Result<Memory, wasmi::core::Trap> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| wasmi::core::Trap::new("No memory export found"))
}

fn read_memory(
    caller: &Caller<GasState>,
    memory: &Memory,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, wasmi::core::Trap> {
    let mut buffer = vec![0u8; len as usize];
    memory
        .read(caller, ptr as usize, &mut buffer)
        .map_err(|e| wasmi::core::Trap::new(format!("Memory read error: {}", e)))?;
    Ok(buffer)
}

fn write_memory(
    caller: &mut Caller<GasState>,
    memory: &Memory,
    ptr: u32,
    data: &[u8],
) -> Result<(), wasmi::core::Trap> {
    memory
        .write(caller, ptr as usize, data)
        .map_err(|e| wasmi::core::Trap::new(format!("Memory write error: {}", e)))
}
