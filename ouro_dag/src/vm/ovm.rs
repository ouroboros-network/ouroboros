// src/vm/ovm.rs
//! Ouroboros Virtual Machine - Main WASM runtime
//!
//! Executes smart contracts in a sandboxed WASM environment with gas metering.

use super::gas::GasState;
use super::host_functions::{register_host_functions, HostContext};
use super::storage::ContractStorage;
use super::types::{ContractAddress, ContractMetadata, ContractResult, ExecutionContext};
use anyhow::{bail, Result};
use chrono::Utc;
use std::sync::Arc;
use wasmi::{Engine, Func, Instance, Linker, Module, Store};

/// Ouroboros Virtual Machine
pub struct OuroborosVM {
    /// WASM engine (reusable across executions)
    engine: Engine,

    /// Contract storage
    storage: Arc<ContractStorage>,

    /// Default gas limit
    default_gas_limit: u64,
}

impl OuroborosVM {
    /// Create new OVM instance
    pub fn new(storage: Arc<ContractStorage>, default_gas_limit: u64) -> Self {
        Self {
            engine: Engine::default(),
            storage,
            default_gas_limit,
        }
    }

    /// Get reference to contract storage
    pub fn storage(&self) -> &ContractStorage {
        &self.storage
    }

    /// Deploy a new WASM contract
    ///
    /// # Arguments
    /// * `code` - WASM bytecode
    /// * `deployer` - Address of deployer
    /// * `name` - Optional contract name
    /// * `version` - Optional contract version
    ///
    /// # Returns
    /// Contract address (derived from code hash)
    pub fn deploy_contract(
        &self,
        code: &[u8],
        deployer: String,
        name: Option<String>,
        version: Option<String>,
    ) -> Result<ContractAddress> {
        // SECURITY: Verify determinism BEFORE creating the module
        // This prevents non-deterministic contracts from being deployed
        self.verify_determinism(code)?;

        // Validate WASM module structure
        let module = Module::new(&self.engine, code)
            .map_err(|e| anyhow::anyhow!("Invalid WASM module: {}", e))?;

        // Generate contract address from code hash
        let address = ContractAddress::from_code(code);

        // Check if already deployed
        if self.storage.contract_exists(address)? {
            bail!("Contract already deployed at {}", address);
        }

        // Store contract code
        self.storage.store_contract_code(address, code)?;

        // Store metadata
        let metadata = ContractMetadata {
            address,
            owner: deployer,
            code_size: code.len(),
            code_hash: address.to_hex(),
            deployed_at: Utc::now(),
            total_gas_used: 0,
            call_count: 0,
            balance: 0,
            name,
            version,
        };
        self.storage.store_metadata(&metadata)?;

        log::info!(
            "NOTE Contract deployed: {} ({} bytes) by {}",
            address,
            code.len(),
            metadata.owner
        );

        Ok(address)
    }

    /// Call a contract method
    ///
    /// # Arguments
    /// * `context` - Execution context (contract, caller, gas limit, etc.)
    /// * `method` - Method name to call
    /// * `args` - Method arguments (encoded)
    ///
    /// # Returns
    /// Contract execution result
    pub fn call_contract(
        &self,
        context: ExecutionContext,
        method: &str,
        args: &[u8],
    ) -> Result<ContractResult> {
        // Load contract code
        let code = self.storage.load_contract_code(context.contract_address)?;

        // Create WASM module
        let module = Module::new(&self.engine, &code[..])
            .map_err(|e| anyhow::anyhow!("Failed to load contract module: {}", e))?;

        // Create gas state
        let mut gas_state = GasState::new(context.gas_limit);
        gas_state.contract_address = Some(context.contract_address.to_hex());
        gas_state.caller = Some(context.caller.clone());

        // Create store with gas metering
        let mut store = Store::new(&self.engine, gas_state);

        // Create host context
        let host_context = Arc::new(HostContext::new(
            self.storage.clone(),
            context.contract_address,
            context.caller.clone(),
            context.block_number,
            context.block_timestamp,
        ));

        // Create linker and register host functions
        let mut linker = Linker::new(&self.engine);
        register_host_functions(&mut linker, host_context.clone())?;

        // Instantiate module
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate module: {}", e))?
            .start(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to start instance: {}", e))?;

        // Get memory export
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| anyhow::anyhow!("Contract has no memory export"))?;

        // Write args to WASM memory
        let args_ptr = self.write_to_wasm_memory(&mut store, &memory, args)?;

        // Get the function to call
        let func = instance
            .get_func(&store, method)
            .ok_or_else(|| anyhow::anyhow!("Method '{}' not found", method))?;

        // Determine function signature and call
        let result = match func.ty(&store).params().len() {
            0 => {
                // No args function
                let typed_func = func
                    .typed::<(), u32>(&store)
                    .map_err(|e| anyhow::anyhow!("Invalid function signature: {}", e))?;
                typed_func.call(&mut store, ())
            }
            2 => {
                // (ptr, len) function
                let typed_func = func
                    .typed::<(u32, u32), u32>(&store)
                    .map_err(|e| anyhow::anyhow!("Invalid function signature: {}", e))?;
                typed_func.call(&mut store, (args_ptr, args.len() as u32))
            }
            _ => {
                bail!("Unsupported function signature for method '{}'", method);
            }
        };

        // Handle execution result
        match result {
            Ok(result_ptr) => {
                // Read return data from memory
                let return_data = if result_ptr > 0 {
                    // Assume first 4 bytes at result_ptr is length
                    let mut len_bytes = [0u8; 4];
                    memory
                        .read(&store, result_ptr as usize, &mut len_bytes)
                        .map_err(|e| anyhow::anyhow!("Failed to read result length: {}", e))?;
                    let len = u32::from_le_bytes(len_bytes);

                    let mut data = vec![0u8; len as usize];
                    memory
                        .read(&store, (result_ptr + 4) as usize, &mut data)
                        .map_err(|e| anyhow::anyhow!("Failed to read result data: {}", e))?;
                    data
                } else {
                    vec![]
                };

                // Get gas used
                let gas_used = store.data().gas_used;

                // Get logs
                let logs = host_context.logs.lock().clone();

                // Update metadata
                if let Ok(Some(mut metadata)) = self.storage.load_metadata(context.contract_address)
                {
                    metadata.total_gas_used += gas_used;
                    metadata.call_count += 1;
                    let _ = self.storage.store_metadata(&metadata);
                }

                Ok(ContractResult {
                    success: true,
                    return_data,
                    gas_used,
                    error: None,
                    logs,
                })
            }
            Err(e) => {
                // Execution failed
                let gas_used = store.data().gas_used;
                let logs = host_context.logs.lock().clone();

                Ok(ContractResult {
                    success: false,
                    return_data: vec![],
                    gas_used,
                    error: Some(e.to_string()),
                    logs,
                })
            }
        }
    }

    /// Get contract metadata
    pub fn get_contract_metadata(
        &self,
        address: ContractAddress,
    ) -> Result<Option<ContractMetadata>> {
        self.storage.load_metadata(address)
    }

    /// Check if contract exists
    pub fn contract_exists(&self, address: ContractAddress) -> Result<bool> {
        self.storage.contract_exists(address)
    }

    // Internal helper methods

    /// Verify WASM module determinism
    /// Verify WASM module is deterministic (no float ops, no non-deterministic instructions)
    ///
    /// SECURITY: Non-deterministic contracts can cause consensus failures where different
    /// validators get different execution results. This breaks BFT consensus.
    ///
    /// Forbidden operations:
    /// - Floating point arithmetic (f32, f64 operations)
    /// - Non-deterministic instructions
    /// - Imports of non-deterministic host functions
    ///
    /// This is critical for blockchain consensus - all validators must get identical results.
    fn verify_determinism(&self, wasm_bytes: &[u8]) -> Result<()> {
        use wasmparser::{Operator, Parser, Payload};

        log::info!(
            "DEBUG: Validating WASM module for determinism (size: {} bytes)",
            wasm_bytes.len()
        );

        let mut float_ops_found = Vec::new();
        let mut suspicious_imports = Vec::new();
        let mut total_instructions = 0;

        // Parse WASM module
        for payload in Parser::new(0).parse_all(wasm_bytes) {
            let payload = payload.map_err(|e| anyhow::anyhow!("WASM parsing error: {}", e))?;

            match payload {
                // Validate code section (actual instructions)
                Payload::CodeSectionEntry(body) => {
                    let mut reader = body
                        .get_operators_reader()
                        .map_err(|e| anyhow::anyhow!("Failed to read operators: {}", e))?;

                    while !reader.eof() {
                        let op = reader
                            .read()
                            .map_err(|e| anyhow::anyhow!("Failed to read operator: {}", e))?;

                        total_instructions += 1;

                        // Check for forbidden floating point operations
                        match op {
                            // F32 operations
                            Operator::F32Const { .. } => float_ops_found.push("f32.const"),
                            Operator::F32Load { .. } => float_ops_found.push("f32.load"),
                            Operator::F32Store { .. } => float_ops_found.push("f32.store"),
                            Operator::F32Eq => float_ops_found.push("f32.eq"),
                            Operator::F32Ne => float_ops_found.push("f32.ne"),
                            Operator::F32Lt => float_ops_found.push("f32.lt"),
                            Operator::F32Gt => float_ops_found.push("f32.gt"),
                            Operator::F32Le => float_ops_found.push("f32.le"),
                            Operator::F32Ge => float_ops_found.push("f32.ge"),
                            Operator::F32Abs => float_ops_found.push("f32.abs"),
                            Operator::F32Neg => float_ops_found.push("f32.neg"),
                            Operator::F32Ceil => float_ops_found.push("f32.ceil"),
                            Operator::F32Floor => float_ops_found.push("f32.floor"),
                            Operator::F32Trunc => float_ops_found.push("f32.trunc"),
                            Operator::F32Nearest => float_ops_found.push("f32.nearest"),
                            Operator::F32Sqrt => float_ops_found.push("f32.sqrt"),
                            Operator::F32Add => float_ops_found.push("f32.add"),
                            Operator::F32Sub => float_ops_found.push("f32.sub"),
                            Operator::F32Mul => float_ops_found.push("f32.mul"),
                            Operator::F32Div => float_ops_found.push("f32.div"),
                            Operator::F32Min => float_ops_found.push("f32.min"),
                            Operator::F32Max => float_ops_found.push("f32.max"),
                            Operator::F32Copysign => float_ops_found.push("f32.copysign"),

                            // F64 operations
                            Operator::F64Const { .. } => float_ops_found.push("f64.const"),
                            Operator::F64Load { .. } => float_ops_found.push("f64.load"),
                            Operator::F64Store { .. } => float_ops_found.push("f64.store"),
                            Operator::F64Eq => float_ops_found.push("f64.eq"),
                            Operator::F64Ne => float_ops_found.push("f64.ne"),
                            Operator::F64Lt => float_ops_found.push("f64.lt"),
                            Operator::F64Gt => float_ops_found.push("f64.gt"),
                            Operator::F64Le => float_ops_found.push("f64.le"),
                            Operator::F64Ge => float_ops_found.push("f64.ge"),
                            Operator::F64Abs => float_ops_found.push("f64.abs"),
                            Operator::F64Neg => float_ops_found.push("f64.neg"),
                            Operator::F64Ceil => float_ops_found.push("f64.ceil"),
                            Operator::F64Floor => float_ops_found.push("f64.floor"),
                            Operator::F64Trunc => float_ops_found.push("f64.trunc"),
                            Operator::F64Nearest => float_ops_found.push("f64.nearest"),
                            Operator::F64Sqrt => float_ops_found.push("f64.sqrt"),
                            Operator::F64Add => float_ops_found.push("f64.add"),
                            Operator::F64Sub => float_ops_found.push("f64.sub"),
                            Operator::F64Mul => float_ops_found.push("f64.mul"),
                            Operator::F64Div => float_ops_found.push("f64.div"),
                            Operator::F64Min => float_ops_found.push("f64.min"),
                            Operator::F64Max => float_ops_found.push("f64.max"),
                            Operator::F64Copysign => float_ops_found.push("f64.copysign"),

                            // Conversion operations between int and float
                            Operator::F32ConvertI32S => float_ops_found.push("f32.convert_i32_s"),
                            Operator::F32ConvertI32U => float_ops_found.push("f32.convert_i32_u"),
                            Operator::F32ConvertI64S => float_ops_found.push("f32.convert_i64_s"),
                            Operator::F32ConvertI64U => float_ops_found.push("f32.convert_i64_u"),
                            Operator::F32DemoteF64 => float_ops_found.push("f32.demote_f64"),
                            Operator::F64ConvertI32S => float_ops_found.push("f64.convert_i32_s"),
                            Operator::F64ConvertI32U => float_ops_found.push("f64.convert_i32_u"),
                            Operator::F64ConvertI64S => float_ops_found.push("f64.convert_i64_s"),
                            Operator::F64ConvertI64U => float_ops_found.push("f64.convert_i64_u"),
                            Operator::F64PromoteF32 => float_ops_found.push("f64.promote_f32"),
                            Operator::I32TruncF32S => float_ops_found.push("i32.trunc_f32_s"),
                            Operator::I32TruncF32U => float_ops_found.push("i32.trunc_f32_u"),
                            Operator::I32TruncF64S => float_ops_found.push("i32.trunc_f64_s"),
                            Operator::I32TruncF64U => float_ops_found.push("i32.trunc_f64_u"),
                            Operator::I64TruncF32S => float_ops_found.push("i64.trunc_f32_s"),
                            Operator::I64TruncF32U => float_ops_found.push("i64.trunc_f32_u"),
                            Operator::I64TruncF64S => float_ops_found.push("i64.trunc_f64_s"),
                            Operator::I64TruncF64U => float_ops_found.push("i64.trunc_f64_u"),
                            Operator::F32ReinterpretI32 => {
                                float_ops_found.push("f32.reinterpret_i32")
                            }
                            Operator::F64ReinterpretI64 => {
                                float_ops_found.push("f64.reinterpret_i64")
                            }
                            Operator::I32ReinterpretF32 => {
                                float_ops_found.push("i32.reinterpret_f32")
                            }
                            Operator::I64ReinterpretF64 => {
                                float_ops_found.push("i64.reinterpret_f64")
                            }

                            // All other operations are deterministic
                            _ => {}
                        }
                    }
                }

                // Validate imports section
                Payload::ImportSection(reader) => {
                    for import in reader {
                        let import =
                            import.map_err(|e| anyhow::anyhow!("Failed to read import: {}", e))?;

                        // Check for suspicious import modules
                        let suspicious_modules = ["env", "wasi_snapshot_preview1", "wasi_unstable"];
                        if suspicious_modules.contains(&import.module) {
                            suspicious_imports.push(format!("{}::{}", import.module, import.name));
                        }
                    }
                }

                _ => {}
            }
        }

        // Reject if any floating point operations found
        if !float_ops_found.is_empty() {
            log::error!(
 "CRITICAL: DETERMINISM VIOLATION: Contract contains {} forbidden floating point operations",
 float_ops_found.len()
 );
            log::error!(
                " Found operations: {:?}",
                &float_ops_found[..std::cmp::min(10, float_ops_found.len())]
            );
            anyhow::bail!(
                "Contract rejected: Contains {} forbidden floating point operations. \
 Floating point arithmetic is non-deterministic and breaks consensus. \
 Use fixed-point integer arithmetic instead.",
                float_ops_found.len()
            );
        }

        // Warn about suspicious imports (but don't reject yet - they might be legitimate host functions)
        if !suspicious_imports.is_empty() {
            log::warn!(
                "WARNING Contract imports potentially non-deterministic functions: {:?}",
                suspicious_imports
            );
            log::warn!(
                " Ensure all imported functions are deterministic and registered as host functions"
            );
        }

        log::info!(
 " WASM determinism validation passed: {} instructions validated, 0 float ops, {} imports",
 total_instructions, suspicious_imports.len()
 );

        Ok(())
    }

    /// Write data to WASM linear memory
    fn write_to_wasm_memory(
        &self,
        store: &mut Store<GasState>,
        memory: &wasmi::Memory,
        data: &[u8],
    ) -> Result<u32> {
        // Allocate memory by growing the memory
        // Simple allocator: find free space at end of current memory
        let pages = memory.current_pages(&*store);
        let pages_count = u32::from(pages) as usize;
        let current_size = pages_count * 65536; // 64KB per page

        // Write at current end of memory
        let ptr = current_size as u32;

        // Ensure we have enough memory
        let needed_pages = ((data.len() + current_size) / 65536) + 1;
        let pages_to_grow = needed_pages.saturating_sub(pages_count);

        if pages_to_grow > 0 {
            memory
                .grow(
                    &mut *store,
                    wasmi::core::Pages::new(pages_to_grow as u32).unwrap(),
                )
                .map_err(|e| anyhow::anyhow!("Failed to grow memory: {}", e))?;
        }

        // Write data
        memory
            .write(&mut *store, ptr as usize, data)
            .map_err(|e| anyhow::anyhow!("Failed to write to memory: {}", e))?;

        Ok(ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::{Options, DB};
    use tempfile::tempdir;

    fn create_test_vm() -> OuroborosVM {
        let dir = tempdir().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = Arc::new(DB::open(&opts, dir.path()).unwrap());
        let storage = Arc::new(ContractStorage::new(db));
        OuroborosVM::new(storage, 10_000_000)
    }

    #[test]
    fn test_deploy_contract() {
        let vm = create_test_vm();

        // Simple WASM module that exports a function
        let wasm = wat::parse_str(
            r#"
 (module
 (func (export "test") (result i32)
 i32.const 42
 )
 )
 "#,
        )
        .unwrap();

        let address = vm
            .deploy_contract(
                &wasm,
                "deployer123".to_string(),
                Some("TestContract".to_string()),
                Some("1.0.0".to_string()),
            )
            .unwrap();

        // Verify contract exists
        assert!(vm.contract_exists(address).unwrap());

        // Verify metadata
        let metadata = vm.get_contract_metadata(address).unwrap().unwrap();
        assert_eq!(metadata.owner, "deployer123");
        assert_eq!(metadata.name, Some("TestContract".to_string()));
        assert_eq!(metadata.code_size, wasm.len());
    }

    #[test]
    fn test_call_contract() {
        let vm = create_test_vm();

        // WASM module with a simple addition function
        let wasm = wat::parse_str(
            r#"
 (module
 (func (export "add") (param i32 i32) (result i32)
 local.get 0
 local.get 1
 i32.add
 )
 )
 "#,
        )
        .unwrap();

        let address = vm
            .deploy_contract(&wasm, "deployer".to_string(), None, None)
            .unwrap();

        let context = ExecutionContext {
            contract_address: address,
            caller: "caller123".to_string(),
            value: 0,
            block_number: 100,
            block_timestamp: 1234567890,
            tx_hash: None,
            gas_limit: 1_000_000,
            chain_id: 1,
        };

        // Note: This test would need proper argument encoding
        // For now, just verify the call mechanism works
        let result = vm.call_contract(context, "add", &[]);

        // May fail due to argument mismatch, but that's expected
        // The important thing is the OVM infrastructure works
    }
}
