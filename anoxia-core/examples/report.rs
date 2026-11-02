//! Writes an HTML report for a CO₂ scenario: cargo run -p anoxia-core --example report -- out.html
use anoxia_core::report::{html, ReportOptions};
use anoxia_core::{run, Configuration, GasType, Language};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "report.html".into());
    let mut c = Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() };
    c.identification.reference = "EVR-2026-014".into();
    c.identification.unit = "Site de Lyon".into();
    c.identification.installation = "Local de stockage CO₂".into();
    c.identification.user = "N. B.".into();
    c.source.count = 2;
    c.release.diameter_mm = 3.0;
    c.room.volume_m3 = 30.0;
    c.room.mechanical_enabled = false;
    c.room.natural_ach = 0.1;
    let r = run(&c).expect("run");
    let opts = ReportOptions {
        lang: Language::French,
        generated_at: "01/11/2026 10:00".into(),
        sizing: None,
        failure: None,
        before_after: None,
    };
    std::fs::write(&out, html(&r, &opts)).expect("write");
}
