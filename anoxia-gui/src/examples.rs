//! Ready-made example scenarios.

use anoxia_core::config::{BottleType, Co2Withdrawal, Configuration, Event, EventAction, ReleaseKind, VentilationMode};
use anoxia_core::{GasType, Language};

pub struct Example {
    name_fr: &'static str,
    name_en: &'static str,
    desc_fr: &'static str,
    desc_en: &'static str,
    pub config: Configuration,
}

impl Example {
    pub fn name(&self, lang: Language) -> &'static str {
        lang.pick(self.name_fr, self.name_en)
    }

    pub fn description(&self, lang: Language) -> &'static str {
        lang.pick(self.desc_fr, self.desc_en)
    }
}

/// Configuration shown at start-up.
pub fn default_config() -> Configuration {
    all().remove(0).config
}

pub fn all() -> Vec<Example> {
    let mut lab = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
    lab.source.bottle_type = BottleType::B50;
    lab.source.volume_liters = 50.0;
    lab.source.pressure_bar_g = 200.0;
    lab.release.kind = ReleaseKind::Leak;
    lab.release.diameter_mm = 2.0;
    lab.release.discharge_coefficient = 0.62;
    lab.room.volume_m3 = 45.0;
    lab.room.height_m = 2.8;
    lab.room.mechanical_enabled = true;
    lab.room.mechanical_flow_m3h = 90.0;
    lab.room.natural_ach = 0.2;

    let mut weld = Configuration { gas: GasType::Argon, ..Configuration::default() };
    weld.release.kind = ReleaseKind::ValveRupture;
    weld.release.diameter_mm = 10.0;
    weld.release.discharge_coefficient = 1.0;
    weld.room.volume_m3 = 120.0;
    weld.room.height_m = 4.0;
    weld.room.mechanical_flow_m3h = 300.0;
    weld.room.natural_ach = 0.5;

    let mut co2 = Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() };
    co2.source.count = 2;
    co2.source.co2_fill_ratio = 0.75;
    co2.source.co2_withdrawal = Co2Withdrawal::Vapour;
    co2.release.diameter_mm = 3.0;
    co2.room.volume_m3 = 30.0;
    co2.room.height_m = 2.5;
    co2.room.mechanical_enabled = false;
    co2.room.natural_ach = 0.1;

    let mut he = Configuration { gas: GasType::Helium, ..Configuration::default() };
    he.source.count = 9;
    he.source.pressure_bar_g = 200.0;
    he.release.diameter_mm = 4.0;
    he.room.volume_m3 = 80.0;
    he.room.height_m = 3.5;
    he.room.mechanical_flow_m3h = 160.0;

    let mut rack = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
    rack.source.count = 12;
    rack.source.pressure_bar_g = 200.0;
    rack.release.kind = ReleaseKind::ValveRupture;
    rack.release.diameter_mm = 8.0;
    rack.release.discharge_coefficient = 1.0;
    rack.room.volume_m3 = 25.0;
    rack.room.height_m = 2.5;
    rack.room.mechanical_flow_m3h = 150.0;
    rack.room.natural_ach = 0.5;

    // Timeline: the extraction fails 3 min into the leak, the detector starts
    // an emergency extraction 2 min later, the operator isolates the leak at 10 min.
    let mut store = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
    store.source.count = 4;
    store.source.pressure_bar_g = 200.0;
    store.release.diameter_mm = 3.0;
    store.room.volume_m3 = 60.0;
    store.room.height_m = 3.0;
    store.room.mechanical_flow_m3h = 200.0;
    store.room.natural_ach = 0.3;
    store.events = vec![
        Event { time_s: 180.0, action: EventAction::VentilationStops },
        Event {
            time_s: 300.0,
            action: EventAction::VentilationStarts { flow_m3h: 600.0, mode: VentilationMode::Extraction },
        },
        Event { time_s: 600.0, action: EventAction::ReleaseIsolated },
    ];

    vec![
        Example {
            name_fr: "Laboratoire – fuite d'azote sur raccord",
            name_en: "Laboratory – nitrogen leak at a fitting",
            desc_fr: "B50 azote 200 bar, fuite Ø 2 mm, salle de 45 m³ ventilée à 90 m³/h.",
            desc_en: "N₂ B50 at 200 bar, 2 mm leak, 45 m³ room ventilated at 90 m³/h.",
            config: lab,
        },
        Example {
            name_fr: "Atelier de soudage – arrachement de robinet d'argon",
            name_en: "Welding shop – argon valve sheared off",
            desc_fr: "B50 argon 200 bar, rupture Ø 10 mm, atelier de 120 m³, extraction 300 m³/h.",
            desc_en: "Ar B50 at 200 bar, 10 mm rupture, 120 m³ workshop, 300 m³/h extraction.",
            config: weld,
        },
        Example {
            name_fr: "Stockage de CO₂ – local non ventilé",
            name_en: "CO₂ storage – unventilated room",
            desc_fr: "2 × B50 CO₂ (37,5 kg chacune), fuite Ø 3 mm, local de 30 m³ sans ventilation mécanique.",
            desc_en: "2 × CO₂ B50 (37.5 kg each), 3 mm leak, 30 m³ room without mechanical ventilation.",
            config: co2,
        },
        Example {
            name_fr: "Local hélium – cadre de 9 bouteilles",
            name_en: "Helium room – 9-cylinder bundle",
            desc_fr:
                "Cadre 9 × B50 hélium 200 bar, fuite Ø 4 mm, local de 80 m³ (gaz léger : accumulation en partie haute).",
            desc_en: "9 × B50 helium bundle at 200 bar, 4 mm leak, 80 m³ room (light gas: accumulates at the top).",
            config: he,
        },
        Example {
            name_fr: "Local gaz – cadre de 12 bouteilles d'azote",
            name_en: "Gas room – 12-cylinder nitrogen bundle",
            desc_fr: "Cadre 12 × B50 azote 200 bar, rupture du collecteur Ø 8 mm, local de 25 m³.",
            desc_en: "12 × B50 nitrogen bundle at 200 bar, 8 mm manifold rupture, 25 m³ room.",
            config: rack,
        },
        Example {
            name_fr: "Local de stockage – panne, secours puis isolement de la fuite",
            name_en: "Storage room – failure, backup fan, then leak isolated",
            desc_fr: "4 × B50 azote 200 bar, fuite Ø 3 mm, local de 60 m³ : extraction en panne à 3 min, extraction d'urgence de 600 m³/h à 5 min, fuite isolée à 10 min.",
            desc_en: "4 × N₂ B50 at 200 bar, 3 mm leak, 60 m³ room: extraction fails at 3 min, 600 m³/h emergency extraction at 5 min, leak isolated at 10 min.",
            config: store,
        },
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn examples_are_valid() {
        for ex in super::all() {
            ex.config.validate().unwrap();
            anoxia_core::run(&ex.config).unwrap();
        }
    }
}
