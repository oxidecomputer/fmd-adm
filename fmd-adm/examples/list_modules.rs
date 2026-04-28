use fmd_adm::{FmdAdm, InvisibleResources, NvValue};

fn main() {
    let adm = FmdAdm::open().expect("failed to open fmd adm handle");

    println!("=== FMD Modules ===");
    let modules = adm.modules().expect("failed to list modules");
    for m in &modules {
        println!(
            "  {} v{} - {}{}",
            m.name,
            m.version,
            m.description,
            if m.failed { " [FAILED]" } else { "" },
        );
    }
    println!("({} modules total)", modules.len());

    println!("\n=== Faulty Resources ===");
    let resources = adm
        .resources(InvisibleResources::Included)
        .expect("failed to list resources");
    if resources.is_empty() {
        println!("  (none)");
    } else {
        for r in &resources {
            println!("  {} (case {})", r.fmri, r.uuid);
        }
    }

    println!("\n=== Cases ===");
    let cases = adm.cases(None).expect("failed to list cases");
    if cases.is_empty() {
        println!("  (none)");
    } else {
        for c in &cases {
            let severity = c
                .event
                .as_ref()
                .and_then(|e| e.lookup("severity"))
                .and_then(|v| match v {
                    NvValue::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("unknown");
            println!("  {} - {} (severity: {})", c.uuid, c.code, severity);
        }
    }
}
