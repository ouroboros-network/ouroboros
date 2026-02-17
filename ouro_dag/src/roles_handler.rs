
async fn handle_roles_command() -> std::io::Result<()> {
    use cli_dashboard::colors;

    println!("
{}{}OUROBOROS NODE TIERS & ROLES{}", colors::BOLD, colors::CYAN, colors::RESET);
    println!("{}", cli_dashboard::horizontal_line(70));
    println!("Ouroboros uses a tiered architecture to balance security and scale.");
    println!("Choose the role that best matches your hardware and goals.
");

    println!("{}1. HEAVY (Validator / Settlement Node){}", colors::BOLD, colors::RESET);
    println!("   {}Language:{} Rust", colors::DIM, colors::RESET);
    println!("   {}Reward:  {} 100% (1.0x multiplier)", colors::GREEN, colors::RESET);
    println!("   {}Duties:  {} BFT Consensus, Global Finality, Fraud Adjudication.", colors::DIM, colors::RESET);
    println!("   {}Hardware:{} 8+ CPU cores, 16GB+ RAM, 1TB+ SSD, 1Gbps Fiber.", colors::DIM, colors::RESET);
    println!("   {}Use Case:{} For institutional stakers and core security providers.
");

    println!("{}2. MEDIUM (Subchain Aggregator / Shadow Hub){}", colors::BOLD, colors::RESET);
    println!("   {}Language:{} Python/Rust", colors::DIM, colors::RESET);
    println!("   {}Reward:  {} 50% (0.5x multiplier) + Aggregation Fees", colors::GREEN, colors::RESET);
    println!("   {}Duties:  {} Batching microchains, Ordering, Shadow Settlement.", colors::DIM, colors::RESET);
    println!("   {}Hardware:{} 4+ CPU cores, 8GB RAM, 500GB SSD, stable connection.", colors::DIM, colors::RESET);
    println!("   {}Use Case:{} For community infrastructure and app developers.
");

    println!("{}3. LIGHT (App Node / Surveillance Watchdog){}", colors::BOLD, colors::RESET);
    println!("   {}Language:{} Python", colors::DIM, colors::RESET);
    println!("   {}Reward:  {} 10% (0.1x multiplier) + Fraud Bounties", colors::GREEN, colors::RESET);
    println!("   {}Duties:  {} Running App-WASM, Verifying Anchors, Catching Fraud.", colors::DIM, colors::RESET);
    println!("   {}Hardware:{} Any modern laptop, phone, or Raspberry Pi.", colors::DIM, colors::RESET);
    println!("   {}Use Case:{} For everyday users, gamers, and privacy advocates.
");

    println!("{}", cli_dashboard::horizontal_line(70));
    println!("To start with a specific role:");
    println!("  {}ouro start --role <heavy|medium|light>{}", colors::YELLOW, colors::RESET);
    println!();

    Ok(())
}
