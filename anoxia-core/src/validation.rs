//! Built-in validation dossier: published reference data, worked examples
//! from recognised methods and experiments, re-computed with this engine.
//!
//! Every case documents its source, the published value, the computed value
//! and the acceptance criterion. The same cases are run by the test suite,
//! printed by `cargo run -p anoxia-core --example validation` and shown in the
//! application ("Help → Validation dossier").

use crate::config::{
    Blowdown, Configuration, Event, EventAction, ReleaseKind, StratificationMode, ThresholdPreset, ThresholdProfile,
    VentilationMode,
};
use crate::constants::{O2_AMBIENT_PERCENT, P_ATM, R};
use crate::flow::gas_orifice_flow;
use crate::gas::{saturation_densities_co2, saturation_pressure_co2, GasType};
use crate::i18n::Language;
use crate::solver::{simulate, SimulationResult};
use std::f64::consts::PI;

/// Family of validation cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Thermophysical data vs NIST / suppliers
    Properties,
    /// Source term: orifice flow and blowdown
    Source,
    /// Room model vs published calculation methods
    Method,
    /// Room model vs experiments and design standards
    Experiment,
}

impl Category {
    /// Localised name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Properties => lang.pick("Propriétés des gaz", "Gas properties"),
            Self::Source => lang.pick("Terme source (débit, vidange)", "Source term (flow, blowdown)"),
            Self::Method => lang.pick("Méthodes de calcul publiées", "Published calculation methods"),
            Self::Experiment => lang.pick("Essais et normes de conception", "Experiments and design standards"),
        }
    }
}

/// What a case demonstrates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nature {
    /// Comparison with independent data (reference database, supplier data, measurements)
    Validation,
    /// Re-computation of a published calculation method (same equations)
    Reproduction,
    /// Numerical check against a closed-form / analytic solution
    Verification,
}

impl Nature {
    /// Localised name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Validation => lang.pick("Validation (données indépendantes)", "Validation (independent data)"),
            Self::Reproduction => lang.pick("Reproduction d'une méthode publiée", "Reproduction of a published method"),
            Self::Verification => lang.pick("Vérification (solution analytique)", "Verification (analytic solution)"),
        }
    }

    /// Short label.
    #[must_use]
    pub fn short(self, lang: Language) -> &'static str {
        match self {
            Self::Validation => lang.pick("Validation", "Validation"),
            Self::Reproduction => lang.pick("Reproduction", "Reproduction"),
            Self::Verification => lang.pick("Vérification", "Verification"),
        }
    }
}

/// Acceptance criterion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Criterion {
    /// |computed − published| ≤ value
    Abs(f64),
    /// |computed − published| ≤ value·|published|
    Rel(f64),
    /// O₂-type quantity: computed may be lower (more conservative) by at most
    /// `value`, never higher than published + 0.05
    ConservativeLow(f64),
    /// Concentration-type quantity: computed may be higher (more conservative)
    /// by at most `value`, never lower than published − 0.5
    ConservativeHigh(f64),
    /// Both values on the same side of a threshold (classification agreement)
    SameSideBelow(f64),
    /// Methodological deviation, documented and explained (not a pass/fail check)
    Documented,
}

impl Criterion {
    fn check(self, published: f64, computed: f64) -> bool {
        if !computed.is_finite() {
            return false;
        }
        match self {
            Self::Abs(t) => (computed - published).abs() <= t,
            Self::Rel(t) => (computed - published).abs() <= t * published.abs(),
            Self::ConservativeLow(t) => computed <= published + 0.05 && published - computed <= t,
            Self::ConservativeHigh(t) => computed >= published - 0.5 && computed - published <= t,
            Self::SameSideBelow(th) => (published < th) == (computed < th),
            Self::Documented => true,
        }
    }

    /// Human-readable criterion.
    #[must_use]
    pub fn describe(self, lang: Language) -> String {
        // Just enough decimals to show the tolerance exactly (0.005 → "0,005").
        let decimals =
            |t: f64| (0..=4).find(|&d| ((t * 10f64.powi(d)).round() / 10f64.powi(d) - t).abs() < 1e-9).unwrap_or(4);
        let num = |t: f64| crate::i18n::fmt_num(t, decimals(t) as usize, lang);
        match self {
            Self::Abs(t) => format!("± {}", num(t)),
            Self::Rel(t) => format!("± {}", crate::i18n::fmt_pct(t * 100.0, 1, lang)),
            Self::ConservativeLow(t) | Self::ConservativeHigh(t) => {
                format!("{} ≤ {}", lang.pick("plus prudent, écart", "more conservative, deviation"), num(t))
            }
            Self::SameSideBelow(th) => format!(
                "{} {}",
                lang.pick("même classement par rapport à", "same classification vs"),
                crate::i18n::fmt_pct(th, 0, lang)
            ),
            Self::Documented => lang.pick("écart documenté", "documented deviation").to_string(),
        }
    }
}

/// One validation case.
#[derive(Debug, Clone)]
pub struct Case {
    /// Identifier
    pub id: &'static str,
    /// Family
    pub category: Category,
    /// Reference (document, page)
    pub reference: &'static str,
    /// Description (French, English)
    pub description: (&'static str, &'static str),
    /// Unit of the compared quantity
    pub unit: &'static str,
    /// Published value
    pub published: f64,
    /// Value computed by this software
    pub computed: f64,
    /// Acceptance criterion
    pub criterion: Criterion,
    /// Explanation of the deviation, if any (French, English)
    pub note: (&'static str, &'static str),
}

impl Case {
    /// Criterion satisfied.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.criterion.check(self.published, self.computed)
    }

    /// Localised description.
    #[must_use]
    pub fn description(&self, lang: Language) -> &'static str {
        lang.pick(self.description.0, self.description.1)
    }

    /// Localised note.
    #[must_use]
    pub fn note(&self, lang: Language) -> &'static str {
        lang.pick(self.note.0, self.note.1)
    }

    /// What the case demonstrates.
    #[must_use]
    pub fn nature(&self) -> Nature {
        match self.id {
            "S3" | "S4" | "S5" | "E1" | "E2" | "E3" | "E4" => Nature::Verification,
            id if id.starts_with('V') => Nature::Verification,
            id if id.starts_with('P') || id.starts_with('E') => Nature::Validation,
            _ => Nature::Reproduction,
        }
    }

    /// `true` for a documented methodological deviation.
    #[must_use]
    pub fn documented(&self) -> bool {
        self.criterion == Criterion::Documented
    }
}

/// Empty room, no ventilation, well-mixed model, 20 °C, 1 atm.
fn room(gas: GasType, volume: f64) -> Configuration {
    let mut c = Configuration { gas, ..Configuration::default() };
    c.room.volume_m3 = volume;
    c.room.height_m = 3.0;
    c.room.mechanical_enabled = false;
    c.room.natural_ach = 0.0;
    c.room.ventilation_efficiency = 1.0;
    c.room.temperature_c = 20.0;
    c.source.temperature_c = 20.0;
    c.model.stratification = StratificationMode::Off;
    c.model.max_duration_h = 48.0;
    c.thresholds = ThresholdProfile { preset: ThresholdPreset::France, alarm1: 19.0, alarm2: 17.0 };
    c
}

fn constant_flow(mut c: Configuration, flow_m3h: f64, minutes: f64) -> Configuration {
    c.release.kind = ReleaseKind::ConstantFlow;
    c.release.constant_flow_m3h = flow_m3h;
    c.release.constant_duration_min = minutes;
    c
}

fn run(c: &Configuration) -> SimulationResult {
    simulate(c).unwrap_or_else(|e| panic!("validation case failed to simulate: {e}"))
}

/// Linear interpolation of a series at time `t`.
fn at(time: &[f64], y: &[f64], t: f64) -> f64 {
    match time.iter().position(|&x| x >= t) {
        Some(0) => y[0],
        Some(i) => {
            let f = (t - time[i - 1]) / (time[i] - time[i - 1]);
            y[i - 1] + f * (y[i] - y[i - 1])
        }
        None => *y.last().unwrap_or(&f64::NAN),
    }
}

/// Our O₂ rescaled to the 21 % baseline used by many published methods.
fn to_21(o2: f64) -> f64 {
    o2 * 21.0 / O2_AMBIENT_PERCENT
}

/// All validation cases.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn cases() -> Vec<Case> {
    let mut v = Vec::new();

    // ------------------------------------------------------------ properties
    for (id, gas, t, p, z) in [
        ("P1", GasType::Nitrogen, 288.15, 200e5, 1.04779),
        ("P2", GasType::Argon, 288.15, 200e5, 0.93677),
        ("P3", GasType::Helium, 288.15, 300e5, 1.14398),
        ("P4", GasType::Neon, 288.15, 300e5, 1.15227),
    ] {
        v.push(Case {
            id,
            category: Category::Properties,
            reference: "NIST Chemistry WebBook (REFPROP), isotherme 15 °C",
            description: (
                match gas {
                    GasType::Nitrogen => "Facteur de compressibilité Z — azote, 200 bar, 15 °C",
                    GasType::Argon => "Facteur de compressibilité Z — argon, 200 bar, 15 °C",
                    GasType::Helium => "Facteur de compressibilité Z — hélium, 300 bar, 15 °C",
                    _ => "Facteur de compressibilité Z — néon, 300 bar, 15 °C",
                },
                match gas {
                    GasType::Nitrogen => "Compressibility factor Z — nitrogen, 200 bar, 15 °C",
                    GasType::Argon => "Compressibility factor Z — argon, 200 bar, 15 °C",
                    GasType::Helium => "Compressibility factor Z — helium, 300 bar, 15 °C",
                    _ => "Compressibility factor Z — neon, 300 bar, 15 °C",
                },
            ),
            unit: "–",
            published: z,
            computed: gas.compressibility(t, p),
            criterion: Criterion::Rel(0.003),
            note: (
                "Point hors grille (interpolation entre 10 et 20 °C).",
                "Off-grid point (interpolated between 10 and 20 °C).",
            ),
        });
    }

    for (id, gas, t, p_bar, rho, label) in [
        (
            "P1b",
            GasType::Argon,
            293.15,
            162.0,
            282.463,
            (
                "Facteur Z — argon, 162 bar, 20 °C (minimum de Z, hors grille)",
                "Factor Z — argon, 162 bar, 20 °C (Z minimum, off-grid)",
            ),
        ),
        (
            "P2b",
            GasType::Argon,
            288.15,
            162.0,
            289.541,
            ("Facteur Z — argon, 162 bar, 15 °C (hors grille)", "Factor Z — argon, 162 bar, 15 °C (off-grid)"),
        ),
        (
            "P3b",
            GasType::Nitrogen,
            293.15,
            212.0,
            229.694,
            ("Facteur Z — azote, 212 bar, 20 °C (hors grille)", "Factor Z — nitrogen, 212 bar, 20 °C (off-grid)"),
        ),
        (
            "P4b",
            GasType::Nitrogen,
            268.15,
            333.0,
            358.963,
            ("Facteur Z — azote, 333 bar, −5 °C (hors grille)", "Factor Z — nitrogen, 333 bar, −5 °C (off-grid)"),
        ),
        (
            "P5b",
            GasType::Helium,
            293.15,
            137.0,
            21.1120,
            ("Facteur Z — hélium, 137 bar, 20 °C (hors grille)", "Factor Z — helium, 137 bar, 20 °C (off-grid)"),
        ),
    ] {
        let z_nist = p_bar * 1e5 * gas.properties().molar_mass / (rho * R * t);
        v.push(Case {
            id,
            category: Category::Properties,
            reference: "NIST Chemistry WebBook (REFPROP), point hors grille",
            description: label,
            unit: "–",
            published: z_nist,
            computed: gas.compressibility(t, p_bar * 1e5),
            criterion: Criterion::Rel(0.003),
            note: (
                "Interpolation bilinéaire en température et en pression.",
                "Bilinear interpolation in temperature and pressure.",
            ),
        });
    }

    for (id, gas, linde) in
        [("P5", GasType::Nitrogen, 9.53), ("P6", GasType::Argon, 10.7), ("P7", GasType::Helium, 9.13)]
    {
        let mut c = room(gas, 100.0);
        c.source.temperature_c = 15.0;
        c.source.pressure_bar_g = 200.0 - P_ATM / 1e5;
        c.room.pressure_hpa = 1000.0;
        let src = crate::flow::Source::from_config(&c).expect("source");
        let contents = src.initial_mass / gas.density(288.15, 1e5);
        v.push(Case {
            id,
            category: Category::Properties,
            reference: "Linde GmbH, fiches produit (Nitrogen, Argon 5.0, Helium 4.6)",
            description: (
                match gas {
                    GasType::Nitrogen => "Contenu d'une bouteille B50 d'azote à 200 bar (m³ à 15 °C, 1 bar)",
                    GasType::Argon => "Contenu d'une bouteille B50 d'argon à 200 bar (m³ à 15 °C, 1 bar)",
                    _ => "Contenu d'une bouteille B50 d'hélium à 200 bar (m³ à 15 °C, 1 bar)",
                },
                match gas {
                    GasType::Nitrogen => "Contents of a 50 L nitrogen cylinder at 200 bar (m³ at 15 °C, 1 bar)",
                    GasType::Argon => "Contents of a 50 L argon cylinder at 200 bar (m³ at 15 °C, 1 bar)",
                    _ => "Contents of a 50 L helium cylinder at 200 bar (m³ at 15 °C, 1 bar)",
                },
            ),
            unit: "m³",
            published: linde,
            computed: contents,
            criterion: Criterion::Rel(0.015),
            note: (
                "Pression de remplissage interprétée en absolu (en relatif : +0,6 %, toujours conforme). Un gaz parfait donnerait +5 % (N₂, He) ou −6 % (Ar).",
                "Filling pressure taken as absolute (as gauge: +0.6%, still within tolerance). An ideal gas would give +5% (N₂, He) or −6% (Ar).",
            ),
        });
    }

    for (id, tc, p_nist) in
        [("P8", 0.0, 34.8514), ("P9", 10.0, 45.0218), ("P10", 20.0, 57.2905), ("P11", 25.0, 64.3424)]
    {
        v.push(Case {
            id,
            category: Category::Properties,
            reference: "NIST Chemistry WebBook — CO₂, propriétés à saturation",
            description: (
                match id {
                    "P8" => "Pression de vapeur saturante du CO₂ à 0 °C",
                    "P9" => "Pression de vapeur saturante du CO₂ à 10 °C",
                    "P10" => "Pression de vapeur saturante du CO₂ à 20 °C",
                    _ => "Pression de vapeur saturante du CO₂ à 25 °C",
                },
                match id {
                    "P8" => "CO₂ vapour pressure at 0 °C",
                    "P9" => "CO₂ vapour pressure at 10 °C",
                    "P10" => "CO₂ vapour pressure at 20 °C",
                    _ => "CO₂ vapour pressure at 25 °C",
                },
            ),
            unit: "bar",
            published: p_nist,
            computed: saturation_pressure_co2(tc + 273.15).unwrap_or(f64::NAN) / 1e5,
            criterion: Criterion::Rel(0.005),
            note: ("", ""),
        });
    }
    let (rho_l, rho_v) = saturation_densities_co2(293.15).unwrap_or((f64::NAN, f64::NAN));
    v.push(Case {
        id: "P12",
        category: Category::Properties,
        reference: "NIST Chemistry WebBook — CO₂ à saturation, 20 °C",
        description: ("Masse volumique du CO₂ liquide saturé à 20 °C", "Saturated liquid CO₂ density at 20 °C"),
        unit: "kg/m³",
        published: 773.387,
        computed: rho_l,
        criterion: Criterion::Rel(0.01),
        note: ("", ""),
    });
    v.push(Case {
        id: "P13",
        category: Category::Properties,
        reference: "NIST Chemistry WebBook — CO₂ à saturation, 20 °C",
        description: ("Masse volumique de la vapeur de CO₂ saturée à 20 °C", "Saturated CO₂ vapour density at 20 °C"),
        unit: "kg/m³",
        published: 194.202,
        computed: rho_v,
        criterion: Criterion::Rel(0.01),
        note: ("", ""),
    });
    {
        let c = room(GasType::CarbonDioxide, 100.0);
        let src = crate::flow::Source::from_config(&c).expect("source");
        v.push(Case {
            id: "P14",
            category: Category::Properties,
            reference: "NIST WebBook (ρ = 1,839 kg/m³ à 20 °C, 1 atm) ; Linde BIOGON C (B50 = 37,5 kg)",
            description: (
                "Volume de gaz d'une bouteille B50 de CO₂ (37,5 kg) à 20 °C, 1 atm",
                "Gas volume of a 50 L CO₂ cylinder (37.5 kg) at 20 °C, 1 atm",
            ),
            unit: "m³",
            published: 20.39,
            computed: src.volume_at(src.initial_mass, 293.15),
            criterion: Criterion::ConservativeHigh(0.25),
            note: (
                "Le volume utile pour la dilution est le volume molaire du gaz parfait (fraction molaire dans l'air) ; la valeur NIST du CO₂ pur (Z = 0,995) est 0,5 % plus faible.",
                "The volume relevant for dilution is the ideal-gas molar volume (mole fraction in air); the NIST value for pure CO₂ (Z = 0.995) is 0.5% lower.",
            ),
        });
    }

    // ---------------------------------------------------------- source term
    {
        let (p0, t0, m, gamma, d) = (50e5, 288.15, 2.016e-3, 1.41, 0.1);
        let rho0 = p0 * m / (R * t0);
        let q = gas_orifice_flow(0.62, PI * d * d / 4.0, gamma, p0, rho0, P_ATM).0;
        v.push(Case {
            id: "S1",
            category: Category::Source,
            reference: "TNO Yellow Book CPR 14E (2005), §2.6.2.1, tableau 2.3",
            description: (
                "Débit sonique d'hydrogène, trou de 100 mm, 50 bar, Cd = 0,62",
                "Choked hydrogen flow, 100 mm hole, 50 bar, Cd = 0.62",
            ),
            unit: "kg/s",
            published: 15.31,
            computed: q,
            criterion: Criterion::Rel(0.005),
            note: ("", ""),
        });
    }
    {
        let p0 = (200.0 + 14.7) * 6894.757;
        let t0 = (80.0 - 32.0) / 1.8 + 273.15;
        let rho0 = p0 * GasType::Nitrogen.properties().molar_mass / (R * t0);
        let d = 0.1 * 0.0254;
        let q = gas_orifice_flow(1.0, PI * d * d / 4.0, 1.41, p0, rho0, P_ATM).0;
        v.push(Case {
            id: "S2",
            category: Category::Source,
            reference: "Crowl & Louvar, Chemical Process Safety, 2e éd., exemple 4-4",
            description: (
                "Fuite d'azote par un trou de 0,1 in, 200 psig, 80 °F, C0 = 1",
                "Nitrogen leak through a 0.1 in hole, 200 psig, 80 °F, C0 = 1",
            ),
            unit: "kg/s",
            published: 0.0380 * 0.453_592,
            computed: q,
            criterion: Criterion::Rel(0.01),
            note: ("Résultat publié : 0,0380 lbm/s.", "Published result: 0.0380 lbm/s."),
        });
    }
    {
        // Ideal-gas adiabatic choked blowdown: P = P0 (1 + (γ−1)/2 · t/τ)^(−2γ/(γ−1))
        let (gamma, tau, p0): (f64, f64, f64) = (1.41, 100.0 * 4.2075 / 15.33, 50.0);
        let p = p0 * (1.0 + (gamma - 1.0) / 2.0 * 10.2 / tau).powf(-2.0 * gamma / (gamma - 1.0));
        v.push(Case {
            id: "S3",
            category: Category::Source,
            reference: "TNO Yellow Book CPR 14E, tableau 2.3 (t = 10,2 s)",
            description: (
                "Vidange adiabatique d'un réservoir d'hydrogène : pression à 10,2 s (équation du modèle adiabatique)",
                "Adiabatic hydrogen vessel blowdown: pressure at 10.2 s (adiabatic model equation)",
            ),
            unit: "bar",
            published: 30.23,
            computed: p,
            criterion: Criterion::Rel(0.01),
            note: (
                "Vérifie seulement que la loi adiabatique de gaz parfait (celle du logiciel) reproduit le tableau du TNO ; le moteur lui-même est vérifié en S4.",
                "Only checks that the ideal-gas adiabatic law (the one used by the software) reproduces the TNO table; the engine itself is verified in S4.",
            ),
        });
    }
    for (id, blowdown, label) in [
        ("S4", Blowdown::Adiabatic, ("adiabatique", "adiabatic")),
        ("S5", Blowdown::Isothermal, ("isotherme", "isothermal")),
    ] {
        let mut c = room(GasType::Nitrogen, 1000.0);
        c.source.pressure_bar_g = 10.0 - P_ATM / 1e5;
        c.release.diameter_mm = 1.0;
        c.model.blowdown = blowdown;
        let r = run(&c);
        let s = &r.summary;
        let tau = s.initial_mass / s.initial_mass_flow;
        let p_num = at(&r.time, &r.source_pressure_bar_g, tau) + P_ATM / 1e5;
        let gamma = GasType::Nitrogen.properties().gamma;
        let p_ref = match blowdown {
            Blowdown::Adiabatic => 10.0 * (1.0 + (gamma - 1.0) / 2.0).powf(-2.0 * gamma / (gamma - 1.0)),
            Blowdown::Isothermal => 10.0 / std::f64::consts::E,
        };
        v.push(Case {
            id,
            category: Category::Source,
            reference: "Solution analytique (gaz parfait, régime sonique)",
            description: (
                if label.0 == "adiabatique" {
                    "Vidange adiabatique d'une B50 d'azote à 10 bar par 1 mm : pression à t = τ"
                } else {
                    "Vidange isotherme d'une B50 d'azote à 10 bar par 1 mm : pression à t = τ"
                },
                if label.1 == "adiabatic" {
                    "Adiabatic blowdown of a 50 L nitrogen cylinder at 10 bar through 1 mm: pressure at t = τ"
                } else {
                    "Isothermal blowdown of a 50 L nitrogen cylinder at 10 bar through 1 mm: pressure at t = τ"
                },
            ),
            unit: "bar",
            published: p_ref,
            computed: p_num,
            criterion: Criterion::Rel(0.015),
            note: (
                "Vérification du schéma numérique (Z ≈ 1 à 10 bar).",
                "Verification of the numerical scheme (Z ≈ 1 at 10 bar).",
            ),
        });
    }

    // ------------------------------------------------------ published methods
    let instant = |gas: GasType, volume: f64| {
        let mut c = room(gas, volume);
        c.release.kind = ReleaseKind::Instantaneous;
        run(&c).summary.min_o2
    };
    v.push(Case {
        id: "M1",
        category: Category::Method,
        reference: "BCGA CP 52 (2023), annexe 1, feuille 1",
        description: (
            "B50 d'azote à 200 bar libérée d'un coup dans 75 m³, sans ventilation",
            "50 L nitrogen cylinder at 200 bar released at once into 75 m³, no ventilation",
        ),
        unit: "% O₂",
        published: 18.2,
        computed: instant(GasType::Nitrogen, 75.0),
        criterion: Criterion::Abs(0.15),
        note: (
            "BCGA prend Vg = P × V = 10 m³ ; le gaz réel (NIST) donne 9,37 m³ à 20 °C, d'où un résultat légèrement moins pénalisant.",
            "BCGA takes Vg = P × V = 10 m³; the real gas (NIST) gives 9.37 m³ at 20 °C, hence a slightly less severe result.",
        ),
    });
    v.push(Case {
        id: "M2",
        category: Category::Method,
        reference: "INRS ED 155 (2024), exemple 1",
        description: (
            "B50 d'azote à 200 bar dans un local de 65 m³, sans apport d'air",
            "50 L nitrogen at 200 bar in a 65 m³ room, no fresh air",
        ),
        unit: "% O₂",
        published: 17.8,
        computed: instant(GasType::Nitrogen, 65.0),
        criterion: Criterion::Abs(0.15),
        note: (
            "Même origine de l'écart que M1 (contenu réel 9,37 m³ au lieu de 10 m³).",
            "Same origin of the deviation as M1 (real contents 9.37 m³ instead of 10 m³).",
        ),
    });
    v.push(Case {
        id: "M3",
        category: Category::Method,
        reference: "INRS ED 155 (2024), exemple 1, variante",
        description: ("B50 d'azote à 200 bar dans un local de 23,4 m³", "50 L nitrogen at 200 bar in a 23.4 m³ room"),
        unit: "% O₂",
        published: 12.0,
        computed: instant(GasType::Nitrogen, 23.4),
        criterion: Criterion::Documented,
        note: (
            "Écart expliqué : l'INRS prend Vg = P × V = 10 m³ (gaz parfait) ; le gaz réel (NIST, Z = 1,05) n'en contient que 9,4 m³ à 20 °C. La méthode simplifiée est reproduite exactement en M3b et affichée dans les résultats.",
            "Explained deviation: INRS takes Vg = P × V = 10 m³ (ideal gas); the real gas (NIST, Z = 1.05) holds only 9.4 m³ at 20 °C. The simplified method is reproduced exactly in M3b and shown in the results.",
        ),
    });
    {
        let mut c = room(GasType::Nitrogen, 23.4);
        c.release.kind = ReleaseKind::Instantaneous;
        c.source.pressure_bar_g = 200.0;
        v.push(Case {
            id: "M3b",
            category: Category::Method,
            reference: "INRS ED 155 (2024), exemple 1, variante — méthode simplifiée",
            description: (
                "Même cas, contrôle « méthode simplifiée INRS / BCGA » affiché par le logiciel",
                "Same case, \"INRS / BCGA simplified method\" cross-check shown by the software",
            ),
            unit: "% O₂",
            published: 12.0,
            computed: run(&c).summary.simplified_method_o2,
            criterion: Criterion::Abs(0.05),
            note: ("", ""),
        });
    }
    v.push(Case {
        id: "M4",
        category: Category::Method,
        reference: "University of Nottingham, Gas calculation guidance (2025), exemple 1",
        description: ("B50 d'argon à 200 bar dans 18 m³ d'air libre", "50 L argon at 200 bar in 18 m³ of free air"),
        unit: "% O₂",
        published: 9.46,
        computed: instant(GasType::Argon, 18.0),
        criterion: Criterion::ConservativeLow(0.7),
        note: (
            "Nottingham suppose un gaz parfait (9,87 m³) ; l'argon réel (Z = 0,944) en contient ≈ 10,4 m³ : résultat plus prudent.",
            "Nottingham assumes an ideal gas (9.87 m³); real argon (Z = 0.944) holds ≈ 10.4 m³: more conservative result.",
        ),
    });
    {
        let mut c = room(GasType::Argon, 6.0);
        c.room.height_m = 2.0;
        c.model.stratification = StratificationMode::Auto;
        let c = constant_flow(c, 2.4, 110.0);
        v.push(Case {
            id: "M5",
            category: Category::Method,
            reference: "INRS ED 155 (2024), exemple 2 (soudage à l'argon)",
            description: (
                "Argon 40 L/min pendant 1 h 50 dans 6 m³ sans apport d'air : classement",
                "Argon 40 L/min for 1 h 50 in 6 m³ without fresh air: classification",
            ),
            unit: "% O₂",
            published: 5.6,
            computed: run(&c).summary.min_o2,
            criterion: Criterion::Documented,
            note: (
                "Écart de méthode : l'INRS applique la formule de déplacement (sans mélange) à un débit continu (5,6 %) ; le modèle bien mélangé donne 10,1 % et la zone basse du modèle bi-zone ≈ 9,7 % selon ses paramètres. Le logiciel est ici moins pénalisant ; il signale alors que la méthode par déplacement donne un classement plus sévère.",
                "Method deviation: INRS applies the displacement formula (no mixing) to a continuous flow (5.6%); the well-mixed model gives 10.1% and the two-zone lower layer ≈ 9.7% depending on its parameters. The software is less severe here; it then warns that the displacement method gives a more severe class.",
            ),
        });
    }
    {
        let mut c = room(GasType::Nitrogen, 32.0);
        c.room.natural_ach = 0.4;
        let r = run(&constant_flow(c, 1.1, 48.0 * 60.0));
        v.push(Case {
            id: "M6",
            category: Category::Method,
            reference: "BCGA CP 52 (2023), §9.3 et annexe 1, feuilles 3–4",
            description: (
                "Fuite de 1,1 m³/h dans 32 m³ à 0,4 vol/h : O₂ après 2 h",
                "1.1 m³/h leak in 32 m³ at 0.4 ACH: O₂ after 2 h",
            ),
            unit: "% O₂",
            published: 20.04,
            computed: to_21(at(&r.time, &r.o2, 7200.0)),
            criterion: Criterion::Abs(0.03),
            note: ("Valeur ramenée à la base 21 % de la source.", "Value rescaled to the 21% baseline of the source."),
        });
        v.push(Case {
            id: "M7",
            category: Category::Method,
            reference: "BCGA CP 52 (2023), §9.3 (régime permanent)",
            description: ("Même cas : O₂ en régime permanent", "Same case: steady-state O₂"),
            unit: "% O₂",
            published: 19.34,
            computed: to_21(r.summary.min_o2),
            criterion: Criterion::Abs(0.03),
            note: ("Valeur ramenée à la base 21 %.", "Value rescaled to the 21% baseline."),
        });
    }
    {
        let mut c = room(GasType::CarbonDioxide, 240.0);
        c.room.natural_ach = 5.0;
        let r = run(&constant_flow(c, 1.2, 24.0 * 60.0));
        v.push(Case {
            id: "M8",
            category: Category::Method,
            reference: "University of York, Biology H&S, « Leakage calculations »",
            description: (
                "Fuite de CO₂ de 1,2 m³/h dans 240 m³ ventilés à 5 vol/h : CO₂ en régime permanent",
                "1.2 m³/h CO₂ leak in 240 m³ at 5 ACH: steady-state CO₂",
            ),
            unit: "% CO₂",
            published: 0.1,
            computed: r.summary.max_gas_fraction * 100.0,
            criterion: Criterion::Abs(0.005),
            note: ("Hors CO₂ atmosphérique (0,04 %).", "Excluding atmospheric CO₂ (0.04%)."),
        });
    }
    {
        let mut c = room(GasType::Nitrogen, 33.6);
        c.room.natural_ach = 0.5;
        let r = run(&constant_flow(c, 0.171, 48.0 * 60.0));
        v.push(Case {
            id: "M9",
            category: Category::Method,
            reference: "University of York, Biology H&S, ventilation LN₂, exemple (a)",
            description: (
                "Évaporation d'azote liquide (0,171 m³/h) dans 33,6 m³ à 0,5 vol/h",
                "Liquid nitrogen boil-off (0.171 m³/h) in 33.6 m³ at 0.5 ACH",
            ),
            unit: "% O₂",
            published: 20.8,
            computed: to_21(r.summary.min_o2),
            criterion: Criterion::Abs(0.05),
            note: ("Base 21 %.", "21% baseline."),
        });
        let r = run(&constant_flow(room(GasType::Nitrogen, 140.0), 68.3 * 60.0, 1.0));
        v.push(Case {
            id: "M10",
            category: Category::Method,
            reference: "University of York, Biology H&S, ventilation LN₂, exemple (b)",
            description: (
                "Vaporisation totale de 100 L d'azote liquide (68,3 m³) dans 140 m³ : méthode par déplacement",
                "Total vaporisation of 100 L of liquid nitrogen (68.3 m³) in 140 m³: displacement method",
            ),
            unit: "% O₂",
            published: 10.7,
            computed: r.summary.instant_displacement_o2,
            criterion: Criterion::Abs(0.1),
            note: (
                "York utilise une base de 20,95 % comme le logiciel. Le modèle dynamique bien mélangé donne 12,9 % (efflux libre) ; le cas enveloppe par déplacement est affiché à côté.",
                "York uses a 20.95% baseline like the software. The dynamic well-mixed model gives 12.9% (free efflux); the displacement bound is shown alongside.",
            ),
        });
    }
    for (id, r_scfm, o2_pub) in [
        ("M11", 818.0, 11.46),
        ("M12", 530.0, 14.82),
        ("M13", 259.0, 17.98),
        ("M14", 65.0, 20.24),
        ("M15", 1859.0, 0.0),
    ] {
        let cfm = 1.699_011; // m³/h per ft³/min
        let mut c = room(GasType::Nitrogen, 14_063.0 * 0.028_316_8);
        c.room.mechanical_enabled = true;
        c.room.mechanical_mode = VentilationMode::Extraction;
        c.room.mechanical_flow_m3h = 1800.0 * cfm;
        let r = run(&constant_flow(c, r_scfm * cfm, 6.0 * 60.0));
        v.push(Case {
            id,
            category: Category::Method,
            reference: "Fermilab FESHM 4240 — analyse ODH du hall MTA (Geynisman 2008)",
            description: (
                match id {
                    "M11" => "Hall de 14 063 ft³, extraction 1 800 cfm, rejet 818 scfm : O₂ permanent",
                    "M12" => "Hall de 14 063 ft³, extraction 1 800 cfm, rejet 530 scfm : O₂ permanent",
                    "M13" => "Hall de 14 063 ft³, extraction 1 800 cfm, rejet 259 scfm : O₂ permanent",
                    "M14" => "Hall de 14 063 ft³, extraction 1 800 cfm, rejet 65 scfm : O₂ permanent",
                    _ => "Hall de 14 063 ft³, extraction 1 800 cfm, rejet 1 859 scfm (> extraction)",
                },
                match id {
                    "M11" => "14,063 ft³ hall, 1,800 cfm exhaust, 818 scfm release: steady O₂",
                    "M12" => "14,063 ft³ hall, 1,800 cfm exhaust, 530 scfm release: steady O₂",
                    "M13" => "14,063 ft³ hall, 1,800 cfm exhaust, 259 scfm release: steady O₂",
                    "M14" => "14,063 ft³ hall, 1,800 cfm exhaust, 65 scfm release: steady O₂",
                    _ => "14,063 ft³ hall, 1,800 cfm exhaust, 1,859 scfm release (> exhaust)",
                },
            ),
            unit: "% O₂",
            published: o2_pub,
            computed: to_21(at(&r.time, &r.o2, 5.9 * 3600.0)),
            criterion: Criterion::Abs(0.03),
            note: ("Mode extraction ; base 21 %.", "Exhaust mode; 21% baseline."),
        });
    }

    // ------------------------------------------------- ODHMOD (AIHA), near field
    // Stefana, Marciano, Drolet, Armstrong (2021), Process Saf. Environ. Prot.
    // 149, 537–556, and the ODHMOD v1.02 workbook (AIHA). Its published scheme
    // (explicit stepping, mole and energy balances) was re-computed from its
    // equations with a fine step; the examples are run here as single-gas
    // equivalents at room conditions.
    {
        let odhmod = "Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA)";
        // Example 10: 10 m³, pure N₂ 0.10 m³/s at 102 kPa for 120 s, half-sphere r = 1 m, s = 0.06 m/s
        let mut c = room(GasType::Nitrogen, 10.0);
        c.room.temperature_c = 25.0;
        c.source.temperature_c = 25.0;
        c.model.near_field_radius_m = 1.0;
        c.model.near_field_air_speed_m_s = 0.06;
        let r = run(&constant_flow(c, 0.10 * 102.0 / 101.325 * 3600.0, 2.0));
        let s = &r.summary;
        v.push(Case {
            id: "O1",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 10 (local de 10 m³, N₂ 0,1 m³/s, 120 s) : temps pour 12 % d'O₂ près de la fuite (r = 1 m)",
                "Example 10 (10 m³ room, N₂ 0.1 m³/s, 120 s): time to 12% O₂ next to the leak (r = 1 m)",
            ),
            unit: "s",
            published: 26.2,
            computed: s.near_crossing(12.0).and_then(|x| x.first).unwrap_or(f64::NAN),
            criterion: Criterion::Rel(0.05),
            note: ("Champ proche ; base 20,95 % contre 20,9 %.", "Near field; 20.95% baseline vs 20.9%."),
        });
        v.push(Case {
            id: "O2",
            category: Category::Method,
            reference: odhmod,
            description: ("Exemple 10 : O₂ minimal près de la fuite", "Example 10: minimum O₂ next to the leak"),
            unit: "% O₂",
            published: 4.36,
            computed: s.min_o2_near.unwrap_or(f64::NAN),
            criterion: Criterion::Abs(0.2),
            note: ("Champ proche, fin du rejet.", "Near field, end of the release."),
        });
        v.push(Case {
            id: "O3",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 10 : temps pour 12 % d'O₂ dans le reste du local (champ lointain)",
                "Example 10: time to 12% O₂ in the rest of the room (far field)",
            ),
            unit: "s",
            published: 58.0,
            computed: s.o2_crossing(12.0).and_then(|x| x.first).unwrap_or(f64::NAN),
            criterion: Criterion::Rel(0.06),
            note: (
                "Moyenne du local (bien mélangé) : plus tôt, donc prudente ; le champ proche d'ODHMOD retarde son champ lointain.",
                "Room average (well-mixed): earlier, hence conservative; ODHMOD's near field delays its far field.",
            ),
        });
        v.push(Case {
            id: "O4",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 10 : O₂ final du local après le rejet (champ lointain)",
                "Example 10: final room O₂ after the release (far field)",
            ),
            unit: "% O₂",
            published: 5.80,
            computed: s.min_o2_mixed,
            criterion: Criterion::Documented,
            note: (
                "ODHMOD expulse l'air du champ lointain, moins appauvri que la moyenne : le local perd plus d'O₂ (5,8 %). La relation « efflux libre » (NFPA 2001, INRS) suppose un mélange expulsé uniforme (6,3 %) ; elle est conservée pour la moyenne du local, le champ proche étant reproduit par ailleurs.",
                "ODHMOD expels far-field air, less depleted than the average: the room loses more O₂ (5.8%). The free-efflux relation (NFPA 2001, INRS) assumes a uniform expelled mixture (6.3%); it is kept for the room average, the near field being reproduced separately.",
            ),
        });

        // Example 1: 100 m³, N₂ 0.05 + Ar 0.05 m³/s at 18 °C for 1 000 s, supply 1.5 m³/s at
        // 26 °C with a 1.5 m³/s return, room at 26 °C, half-sphere r = 2 m
        let mut c = room(GasType::Nitrogen, 100.0);
        c.room.temperature_c = 26.0;
        c.source.temperature_c = 26.0;
        c.room.mechanical_enabled = true;
        c.room.mechanical_mode = VentilationMode::Supply;
        c.room.mechanical_flow_m3h = 1.5 * 3600.0;
        c.model.near_field_radius_m = 2.0;
        c.model.near_field_air_speed_m_s = 0.06;
        let q_room = 0.10 * (26.0 + 273.15) / (18.0 + 273.15);
        let r = run(&constant_flow(c, q_room * 3600.0, 1000.0 / 60.0));
        let s = &r.summary;
        v.push(Case {
            id: "O5",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 1 (100 m³, N₂ + Ar 0,1 m³/s, soufflage 1,5 m³/s) : temps pour 18 % d'O₂ près de la fuite (r = 2 m)",
                "Example 1 (100 m³, N₂ + Ar 0.1 m³/s, 1.5 m³/s supply): time to 18% O₂ next to the leak (r = 2 m)",
            ),
            unit: "s",
            published: 69.2,
            computed: s.near_crossing(18.0).and_then(|x| x.first).unwrap_or(f64::NAN),
            criterion: Criterion::Rel(0.06),
            note: (
                "Deux gaz additionnés en un débit d'azote ramené à 26 °C.",
                "Both gases summed into one nitrogen flow at 26 °C.",
            ),
        });
        v.push(Case {
            id: "O6",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 1 : O₂ près de la fuite à la fin du rejet (1 000 s)",
                "Example 1: O₂ next to the leak at the end of the release (1,000 s)",
            ),
            unit: "% O₂",
            published: 17.22,
            computed: at(&r.time, r.o2_near.as_deref().unwrap_or(&[]), 1000.0),
            criterion: Criterion::Abs(0.2),
            note: ("", ""),
        });
        v.push(Case {
            id: "O7",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 1 : O₂ du reste du local à la fin du rejet (1 000 s)",
                "Example 1: O₂ of the rest of the room at the end of the release (1,000 s)",
            ),
            unit: "% O₂",
            published: 19.56,
            computed: at(&r.time, &r.o2_mixed, 1000.0),
            criterion: Criterion::Abs(0.1),
            note: (
                "Régime permanent du soufflage : identique au bien mélangé.",
                "Supply steady state: same as well-mixed.",
            ),
        });

        // Example 7: 250 m³, N₂ 0.1 m³/s at 5 °C for 120 s, supply 1.0 m³/s at 5 °C and
        // 87.42 kPa, room at 20 °C, half-sphere r = 2 m (flows brought to room conditions)
        let mut c = room(GasType::Nitrogen, 250.0);
        c.room.temperature_c = 20.0;
        c.room.mechanical_enabled = true;
        c.room.mechanical_mode = VentilationMode::Supply;
        c.room.mechanical_flow_m3h = 1.0 * (293.15 / 278.15) * (87.42 / 101.325) * 3600.0;
        c.model.near_field_radius_m = 2.0;
        c.model.near_field_air_speed_m_s = 0.06;
        let r = run(&constant_flow(c, 0.10 * (293.15 / 278.15) * 3600.0, 2.0));
        let s = &r.summary;
        v.push(Case {
            id: "O8",
            category: Category::Method,
            reference: odhmod,
            description: (
                "Exemple 7 (250 m³, N₂ froid 0,1 m³/s, soufflage froid 1 m³/s) : temps pour 18 % d'O₂ près de la fuite",
                "Example 7 (250 m³, cold N₂ 0.1 m³/s, cold 1 m³/s supply): time to 18% O₂ next to the leak",
            ),
            unit: "s",
            published: 94.35,
            computed: s.near_crossing(18.0).and_then(|x| x.first).unwrap_or(f64::NAN),
            criterion: Criterion::Rel(0.08),
            note: (
                "Débits ramenés aux conditions du local ; ODHMOD suit en plus le refroidissement du local.",
                "Flows brought to room conditions; ODHMOD also tracks the cooling of the room.",
            ),
        });
        v.push(Case {
            id: "O9",
            category: Category::Method,
            reference: odhmod,
            description: ("Exemple 7 : O₂ minimal près de la fuite", "Example 7: minimum O₂ next to the leak"),
            unit: "% O₂",
            published: 17.85,
            computed: s.min_o2_near.unwrap_or(f64::NAN),
            criterion: Criterion::Abs(0.3),
            note: ("", ""),
        });
    }

    // --------------------------------------------- experiments and standards
    for (id, conc, x) in [("E1", 34.0, 0.4155), ("E2", 40.0, 0.5109), ("E3", 49.0, 0.6734), ("E4", 62.0, 0.9677)] {
        let volume = 100.0;
        let r = run(&constant_flow(room(GasType::Nitrogen, volume), x * volume * 60.0, 1.0));
        v.push(Case {
            id,
            category: Category::Experiment,
            reference: "NFPA 2001 (2004), tableau A.5.5.2(d) — IG-100, 20 °C",
            description: (
                match id {
                    "E1" => "Noyage à l'azote : 0,4155 m³/m³ doit donner 34 % d'agent",
                    "E2" => "Noyage à l'azote : 0,5109 m³/m³ doit donner 40 % d'agent",
                    "E3" => "Noyage à l'azote : 0,6734 m³/m³ doit donner 49 % d'agent",
                    _ => "Noyage à l'azote : 0,9677 m³/m³ doit donner 62 % d'agent",
                },
                match id {
                    "E1" => "Nitrogen flooding: 0.4155 m³/m³ must give 34% agent",
                    "E2" => "Nitrogen flooding: 0.5109 m³/m³ must give 40% agent",
                    "E3" => "Nitrogen flooding: 0.6734 m³/m³ must give 49% agent",
                    _ => "Nitrogen flooding: 0.9677 m³/m³ must give 62% agent",
                },
            ),
            unit: "%",
            published: conc,
            computed: r.summary.max_gas_fraction * 100.0,
            criterion: Criterion::Abs(0.2),
            note: (
                "Formule normalisée « efflux libre » des systèmes d'extinction à gaz inerte (également ISO 14520).",
                "Standard free-efflux formula of inert-gas extinguishing systems (also ISO 14520).",
            ),
        });
    }
    {
        let volume = 121.0;
        let r = run(&constant_flow(room(GasType::Argon, volume), 0.5108 * volume * 60.0 / 0.97, 0.97));
        v.push(Case {
            id: "E5",
            category: Category::Experiment,
            reference: "Su, Kim, Crampton & Liu (NRC Canada), J. Fire Prot. Eng. 11 (2001) 72–87",
            description: (
                "Essai : décharge d'argon à 40 % en 58 s dans un local de 121 m³ — O₂ stabilisé mesuré 12,5–12,8 %",
                "Test: 40% argon discharge in 58 s into a 121 m³ room — measured stabilised O₂ 12.5–12.8%",
            ),
            unit: "% O₂",
            published: 12.65,
            computed: r.summary.min_o2,
            criterion: Criterion::Abs(0.2),
            note: (
                "La quantité d'argon a été dimensionnée par les expérimentateurs avec la formule d'efflux libre (40 %) ; la mesure confirme cette relation, utilisée par le modèle. Ce n'est pas une prédiction aveugle. Après 250 s, l'O₂ remonte sous plafond (stratification).",
                "The argon quantity was sized by the experimenters with the free-efflux formula (40%); the measurement confirms that relation, which the model uses. This is not a blind prediction. After 250 s, O₂ rises near the ceiling (stratification).",
            ),
        });
    }
    {
        let mut c = room(GasType::Nitrogen, 21.4);
        c.room.mechanical_enabled = true;
        c.room.mechanical_mode = VentilationMode::Extraction;
        c.room.mechanical_flow_m3h = 80.0 * 0.06;
        let r = run(&constant_flow(c, 1.587 * 0.06, 40.0 * 60.0));
        v.push(Case {
            id: "E6",
            category: Category::Experiment,
            reference: "Calorimètre de chambre entière, infusion d'azote — Physiological Reports 5 (2017), PMC5704087",
            description: (
                "Infusion de 1,59 L/min dans une chambre de 21,4 m³ balayée à 80 L/min : O₂ en régime permanent",
                "1.59 L/min infusion into a 21.4 m³ chamber swept at 80 L/min: steady-state O₂",
            ),
            unit: "% O₂",
            published: 20.53,
            computed: r.summary.min_o2,
            criterion: Criterion::Abs(0.05),
            note: (
                "Valeur de référence : bilan bien mélangé des auteurs, confirmé par la récupération mesurée du gaz infusé (± 2 %) ; la tolérance couvre l'incertitude sur l'O₂ de base (20,93–20,95 %).",
                "Reference value: the authors' well-mixed balance, confirmed by the measured recovery of the infused gas (± 2%); the tolerance covers the baseline O₂ uncertainty (20.93–20.95%).",
            ),
        });
    }
    {
        let mut c = room(GasType::Nitrogen, 5700.0);
        c.room.pressure_hpa = 1195.6;
        let vg = 105.0 / GasType::Nitrogen.ideal_density(293.15, 119_560.0);
        let r = run(&constant_flow(c, vg * 2.0, 30.0));
        v.push(Case {
            id: "E7",
            category: Category::Experiment,
            reference: "MiniCLEAN, essais de déversement d'azote liquide à SNOLAB, essai 1 (arXiv:1711.02117)",
            description: (
                "105 kg d'azote vaporisés dans un hall de 5 700 m³ à 1,18 atm : baisse d'O₂ mesurée 0,25–0,30 point",
                "105 kg of nitrogen vaporised in a 5,700 m³ hall at 1.18 atm: measured O₂ drop 0.25–0.30 point",
            ),
            unit: "pts O₂",
            published: 0.275,
            computed: O2_AMBIENT_PERCENT - r.summary.min_o2,
            criterion: Criterion::Documented,
            note: (
                "Preuve à confirmer : page, figure et incertitude de l'annexe B de la thèse MiniCLEAN (arXiv:1711.02117) non vérifiées sur le document source ; recirculation en service, air neuf coupé. Compté comme écart documenté, pas comme validation acquise.",
                "Evidence to be confirmed: page, figure and uncertainty of appendix B of the MiniCLEAN thesis (arXiv:1711.02117) not checked against the source document; recirculation on, fresh air off. Counted as a documented item, not as an established validation.",
            ),
        });
    }
    {
        let volume = 993.0;
        let vg = 1255.0 / GasType::CarbonDioxide.ideal_density(293.15, P_ATM);
        let r = run(&constant_flow(room(GasType::CarbonDioxide, volume), vg * 3600.0 / 207.0, 207.0 / 60.0));
        v.push(Case {
            id: "E8",
            category: Category::Experiment,
            reference: "Zalosh & Hung, Fire Safety Science 4 (1994) 889–900 — essai FM n° 2",
            description: (
                "Décharge de 1 255 kg de CO₂ en 207 s dans 993 m³ : pic de CO₂ mesuré ≈ 40 %",
                "1,255 kg CO₂ discharged in 207 s into 993 m³: measured CO₂ peak ≈ 40%",
            ),
            unit: "% CO₂",
            published: 40.0,
            computed: r.summary.max_co2_percent.unwrap_or(f64::NAN),
            criterion: Criterion::ConservativeHigh(12.0),
            note: (
                "Le modèle surestime (≈ 50 %) : une partie du CO₂ liquide forme de la neige carbonique qui se sublime plus tard, une partie fuit pendant la décharge, et le nuage froid stratifie. Écart du côté de la sécurité.",
                "The model overestimates (≈ 50%): part of the liquid CO₂ forms dry-ice snow that sublimates later, part leaks during discharge, and the cold cloud stratifies. Deviation on the safe side.",
            ),
        });
    }

    // ------------------------------------------------- timeline (verification)
    {
        // A ventilation failure at t = 0 is the scenario without mechanical
        // ventilation: same minimum (the series are identical).
        let mut off = room(GasType::Nitrogen, 45.0);
        off.release.diameter_mm = 2.0;
        off.room.natural_ach = 0.2;
        let mut ev = off.clone();
        ev.room.mechanical_enabled = true;
        ev.room.mechanical_flow_m3h = 300.0;
        ev.events.push(Event { time_s: 0.0, action: EventAction::VentilationStops });
        v.push(Case {
            id: "V1",
            category: Category::Method,
            reference: "Vérification de la chronologie : panne à t = 0 ≡ scénario sans ventilation mécanique",
            description: (
                "B50 d'azote, fuite Ø 2 mm, 45 m³ : panne de la ventilation dès le début du rejet contre ventilation à l'arrêt",
                "Nitrogen B50, 2 mm leak, 45 m³: ventilation failure from the start of the release vs ventilation off",
            ),
            unit: "% O₂",
            published: run(&off).summary.min_o2,
            computed: run(&ev).summary.min_o2,
            criterion: Criterion::Abs(0.001),
            note: (
                "Les deux séries sont identiques point par point (test unitaire) ; le dossier ne retient que le minimum.",
                "Both series are identical point by point (unit test); the dossier only keeps the minimum.",
            ),
        });
    }
    {
        // Fan started after the end of the release: analytic recovery
        // x(t) = x₀·exp(−Q·(t − t₀)/V), checked at t₀ + V/Q (ratio 1/e).
        let mut c = constant_flow(room(GasType::Argon, 50.0), 10.0, 30.0);
        c.model.max_duration_h = 3.0;
        let t0 = 3600.0;
        c.events.push(Event {
            time_s: t0,
            action: EventAction::VentilationStarts { flow_m3h: 100.0, mode: VentilationMode::Supply },
        });
        let r = run(&c);
        let x0 = 1.0 - at(&r.time, &r.o2, t0) / O2_AMBIENT_PERCENT;
        let x_tau = 1.0 - at(&r.time, &r.o2, t0 + 50.0 / 100.0 * 3600.0) / O2_AMBIENT_PERCENT;
        v.push(Case {
            id: "V2",
            category: Category::Method,
            reference: "Solution analytique : décroissance exponentielle après le démarrage d'une ventilation (x/x₀ = e⁻¹ à t₀ + V/Q)",
            description: (
                "10 m³/h d'argon pendant 30 min dans 50 m³ sans ventilation, soufflage de 100 m³/h démarré à 60 min : fraction de gaz restante à t₀ + V/Q",
                "10 m³/h of argon for 30 min into 50 m³ without ventilation, 100 m³/h supply fan started at 60 min: remaining gas fraction at t₀ + V/Q",
            ),
            unit: "–",
            published: (-1.0_f64).exp(),
            computed: x_tau / x0,
            criterion: Criterion::Rel(0.005),
            note: (
                "Vérifie que le pas s'arrête sur l'événement, que l'état reste continu et que la récupération suit la solution exacte.",
                "Checks that the step lands on the event, that the state stays continuous and that the recovery follows the exact solution.",
            ),
        });
    }
    v
}

/// Summary line: (passed, total) over the pass/fail cases (documented
/// deviations excluded).
#[must_use]
pub fn score(cases: &[Case]) -> (usize, usize) {
    let checked: Vec<&Case> = cases.iter().filter(|c| !c.documented()).collect();
    (checked.iter().filter(|c| c.passed()).count(), checked.len())
}

/// Number of cases of a given nature.
#[must_use]
pub fn count(cases: &[Case], nature: Nature) -> usize {
    cases.iter().filter(|c| c.nature() == nature).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_validation_cases_pass() {
        let cases = cases();
        let failed: Vec<String> = cases
            .iter()
            .filter(|c| !c.passed())
            .map(|c| format!("{} {}: published {} computed {}", c.id, c.description.1, c.published, c.computed))
            .collect();
        assert!(failed.is_empty(), "failed cases:\n{}", failed.join("\n"));
        assert!(cases.len() >= 35);
    }
}
