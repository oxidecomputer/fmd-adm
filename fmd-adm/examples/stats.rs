use fmd_adm::FmdAdm;

fn main() {
    let adm = FmdAdm::open().expect("failed to open fmd adm handle");

    // Validate resource_count against resources().len()
    let resources = adm.resources(true).expect("failed to list resources");
    let count = adm.resource_count(true).expect("failed to count resources");
    println!("=== Resources ===");
    println!(
        "  resources(all=true).len() = {}, resource_count(all=true) = {}",
        resources.len(),
        count,
    );
    assert_eq!(resources.len(), count as usize, "resource count mismatch!");

    // Transports
    println!("\n=== Transports ===");
    let xprts = adm.transports().expect("failed to list transports");
    if xprts.is_empty() {
        println!("  (none)");
    } else {
        for id in &xprts {
            println!("  transport {id}");
        }
    }

    // SERD engines for each module
    println!("\n=== SERD Engines ===");
    let modules = adm.modules().expect("failed to list modules");
    let mut total = 0;
    for m in &modules {
        let engines = adm
            .serd_engines(&m.name)
            .expect("failed to list serd engines");
        if !engines.is_empty() {
            println!("  {} ({} engines):", m.name, engines.len());
            for e in &engines {
                println!(
                    "    {} - count={}, n={}, fired={}",
                    e.name, e.count, e.n, e.fired,
                );
            }
            total += engines.len();
        }
    }
    if total == 0 {
        println!("  (none)");
    }

    // Global stats
    println!("\n=== Global Stats ===");
    let stats = adm.stats(None).expect("failed to read stats");
    for s in &stats {
        println!("  {}: {} ({})", s.name, s.value, s.description);
    }

    // Per-module stats for the first module
    if let Some(m) = modules.first() {
        println!("\n=== Stats for {} ===", m.name);
        let mstats = adm
            .stats(Some(&m.name))
            .expect("failed to read module stats");
        for s in &mstats {
            println!("  {}: {} ({})", s.name, s.value, s.description);
        }
    }
}
