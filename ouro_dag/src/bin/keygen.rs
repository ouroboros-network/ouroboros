// Key generation CLI tool for Ouroboros node
// Generate all cryptographic keys required for node operation

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Parser)]
#[command(name = "ouro-keygen")]
#[command(about = "Generate cryptographic keys for Ouroboros node", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate all keys for first-time setup
    Init {
        /// Path to .env file (default: .env)
        #[arg(short, long, default_value = ".env")]
        env_file: String,
    },
    /// Generate BFT validator secret seed
    Validator {
        /// Output to stdout instead of .env file
        #[arg(short, long)]
        stdout: bool,
    },
    /// Generate self-signed TLS certificate
    Tls {
        /// Output directory (default: ./certs)
        #[arg(short, long, default_value = "certs")]
        output: String,
        /// Certificate validity in days (default: 365)
        #[arg(short, long, default_value = "365")]
        days: u32,
    },
    /// Generate anchor private key
    Anchor {
        /// Output to stdout instead of .env file
        #[arg(short, long)]
        stdout: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { env_file } => {
            generate_all_keys(&env_file)?;
        }
        Commands::Validator { stdout } => {
            generate_validator_key(stdout)?;
        }
        Commands::Tls { output, days } => {
            generate_tls_cert(&output, days)?;
        }
        Commands::Anchor { stdout } => {
            generate_anchor_key(stdout)?;
        }
    }

    Ok(())
}

fn generate_all_keys(env_file: &str) -> Result<()> {
    println!("Generating all keys for Ouroboros node...\n");

    // Check if .env exists
    let env_path = Path::new(env_file);
    let env_exists = env_path.exists();

    if env_exists {
        println!("WARNING: {} already exists", env_file);
        println!("Keys will be appended. Back up your file first!");
        println!("Press Enter to continue or Ctrl+C to cancel...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    }

    // Generate BFT_SECRET_SEED
    let bft_seed = generate_random_hex(32)?;
    println!("[1/3] Generated BFT_SECRET_SEED");

    // Generate ANCHOR_PRIVATE_KEY
    let anchor_key = generate_random_hex(32)?;
    println!("[2/3] Generated ANCHOR_PRIVATE_KEY");

    // Generate TLS certs
    println!("[3/3] Generating TLS certificates...");
    generate_tls_cert("certs", 365)?;

    // Write to .env file
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env_path)
        .context("Failed to open .env file")?;

    writeln!(file, "\n# Generated keys ({})", chrono::Utc::now())?;
    writeln!(file, "BFT_SECRET_SEED={}", bft_seed)?;
    writeln!(file, "ANCHOR_PRIVATE_KEY={}", anchor_key)?;
    writeln!(file, "TLS_CERT_PATH=certs/cert.pem")?;
    writeln!(file, "TLS_KEY_PATH=certs/key.pem")?;

    println!("\nSuccess! Keys saved to {}", env_file);
    println!("\nIMPORTANT:");
    println!("  - Never commit {} to version control", env_file);
    println!("  - Keep backups in a secure location");
    println!("  - Each validator needs unique BFT_SECRET_SEED");

    Ok(())
}

fn generate_validator_key(stdout: bool) -> Result<()> {
    let seed = generate_random_hex(32)?;

    if stdout {
        println!("{}", seed);
    } else {
        println!("BFT_SECRET_SEED={}", seed);
        println!("\nAdd this to your .env file");
    }

    Ok(())
}

fn generate_anchor_key(stdout: bool) -> Result<()> {
    let key = generate_random_hex(32)?;

    if stdout {
        println!("{}", key);
    } else {
        println!("ANCHOR_PRIVATE_KEY={}", key);
        println!("\nAdd this to your .env file");
    }

    Ok(())
}

fn generate_tls_cert(output_dir: &str, days: u32) -> Result<()> {
    use rcgen::generate_simple_self_signed;

    // Create output directory
    fs::create_dir_all(output_dir).context("Failed to create output directory")?;

    // Generate self-signed certificate
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let cert =
        generate_simple_self_signed(subject_alt_names).context("Failed to generate certificate")?;

    // Write certificate
    let cert_path = format!("{}/cert.pem", output_dir);
    fs::write(&cert_path, cert.serialize_pem()?).context("Failed to write certificate")?;

    // Write private key
    let key_path = format!("{}/key.pem", output_dir);
    fs::write(&key_path, cert.serialize_private_key_pem())
        .context("Failed to write private key")?;

    println!("Generated TLS certificate:");
    println!("  Certificate: {}", cert_path);
    println!("  Private key: {}", key_path);
    println!("  Valid for {} days", days);
    println!("  Subject: localhost, 127.0.0.1");

    Ok(())
}

fn generate_random_hex(bytes: usize) -> Result<String> {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut buffer = vec![0u8; bytes];
    rng.fill_bytes(&mut buffer);
    Ok(hex::encode(buffer))
}
