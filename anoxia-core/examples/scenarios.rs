//! Prints the key results of a few reference scenarios (sanity check).
//! Run with: cargo run -p anoxia-core --example scenarios --release
use anoxia_core::config::{Configuration, ReleaseKind};
use anoxia_core::{run, GasType, Language};
use std::time::Instant;

fn main() {
    let cases: Vec<(&str, Configuration)> = vec![
        ("N2 B50 200 bar, 2 mm, 60 m3, 120 m3/h", Configuration { gas: GasType::Nitrogen, ..Configuration::default() }),
        ("Ar B50 200 bar, 2 mm, 60 m3 (bi-zone auto)", Configuration::default()),
        ("CO2 B50 0.75 kg/L, 2 mm, 60 m3", Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() }),
        ("He B50 200 bar, 2 mm, 60 m3", Configuration { gas: GasType::Helium, ..Configuration::default() }),
        ("N2 B50 rupture 10 mm, 20 m3, no vent", {
            let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
            c.release.kind = ReleaseKind::ValveRupture;
            c.release.diameter_mm = 10.0;
            c.release.discharge_coefficient = 1.0;
            c.room.volume_m3 = 20.0;
            c.room.mechanical_enabled = false;
            c.room.natural_ach = 0.0;
            c
        }),
        ("N2 0.2 mm slow leak, 30 m3, 0.2 ACH", {
            let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
            c.release.diameter_mm = 0.2;
            c.room.volume_m3 = 30.0;
            c.room.mechanical_enabled = false;
            c
        }),
    ];
    for (name, c) in cases {
        let t0 = Instant::now();
        let r = run(&c).expect("simulation");
        let s = &r.result.summary;
        println!(
            "{name}\n  mass {:.2} kg, gas {:.2} m3, flow0 {:.4} kg/s ({:.1} m3/h), t99 {:?} s\n  minO2 {:.2} % (mixed {:.2}, zone {:?}) at {:.0} s, co2max {:?}, level {:?}, t_a1 {:?}, t_a2 {:?}, points {}, truncated {}, {:.1} ms",
            s.initial_mass, s.releasable_volume_m3, s.initial_mass_flow, s.initial_volume_flow_m3h,
            s.release_duration.map(|v| v.round()), s.min_o2, s.min_o2_mixed, s.min_o2_zone.map(|v| (v * 100.0).round() / 100.0),
            s.time_at_min, s.max_co2_percent.map(|v| (v * 100.0).round() / 100.0), r.assessment.overall,
            r.assessment.t_alarm1.map(|v| v.round()), r.assessment.t_alarm2.map(|v| v.round()),
            r.result.time.len(), s.truncated, t0.elapsed().as_secs_f64() * 1000.0
        );
        let _ = r.assessment.recommendations(Language::French);
    }
}
