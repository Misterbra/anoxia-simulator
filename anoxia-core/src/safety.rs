//! Risk classification (O₂ and CO₂) and recommendations.
//!
//! References used for thresholds and wording:
//! - INRS, *Les espaces confinés* (ED 967) and *Détecteurs portables de gaz et
//!   de vapeurs* (ED 6088): usual O₂ detector alarms at 19 % and 17 %.
//! - EN 50104: performance requirements of O₂ detectors (alarm values set by the user).
//! - OSHA 29 CFR 1910.146(b): oxygen-deficient atmosphere below 19.5 %.
//! - EIGA Doc. 44, *Hazards of inert gases and oxygen depletion*.
//! - NIOSH 86-110 / 87-113, confined-space fatalities (≈ 60 % are would-be rescuers).
//! - CO₂: French indicative 8-h limit value 5 000 ppm (arrêté du 30 juin 2004
//!   modifié, directive 2006/15/CE; the software compares the instantaneous
//!   peak to this value, it does not compute an 8-h average),
//!   ACGIH STEL 30 000 ppm, NIOSH IDLH 40 000 ppm.
//!
//! The assessment never declares an atmosphere "safe": a screening model
//! cannot establish that, and anoxia gives no warning signs.

use crate::config::{Configuration, ReleaseKind};
use crate::gas::Buoyancy;
use crate::i18n::Language;
use crate::solver::SimulationResult;
use serde::{Deserialize, Serialize};

/// Apply French typographic rules (non-breaking spaces before `: ; ! ? %`).
#[must_use]
pub fn typo(text: &str, lang: Language) -> String {
    match lang {
        Language::English => text.to_string(),
        Language::French => text
            .replace(" :", "\u{a0}:")
            .replace(" %", "\u{a0}%")
            .replace(" ;", "\u{202f};")
            .replace(" !", "\u{202f}!")
            .replace(" ?", "\u{202f}?")
            .replace("« ", "«\u{a0}")
            .replace(" »", "\u{a0}»"),
    }
}

/// Overall risk level, ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    /// O₂ stays ≥ 20.5 % in the model (no significant depletion)
    NoDepletion,
    /// Depletion below 20.5 % but above alarm 1: abnormal, leak indicator
    Depletion,
    /// Alarm 1 crossed: evacuation
    Alarm1,
    /// Alarm 2 crossed: imminent danger
    Alarm2,
    /// Below 10 % O₂ (or CO₂ ≥ 10 %): danger to life
    LifeThreatening,
}

impl RiskLevel {
    /// Classify an O₂ minimum against the alarm thresholds.
    #[must_use]
    pub fn from_o2(min_o2: f64, alarm1: f64, alarm2: f64) -> Self {
        if min_o2 < 10.0 {
            Self::LifeThreatening
        } else if min_o2 < alarm2 {
            Self::Alarm2
        } else if min_o2 < alarm1 {
            Self::Alarm1
        } else if min_o2 < 20.5 {
            Self::Depletion
        } else {
            Self::NoDepletion
        }
    }

    /// Short label.
    #[must_use]
    pub fn label(self, lang: Language) -> &'static str {
        match self {
            Self::NoDepletion => lang.pick("AUCUN SEUIL FRANCHI", "NO THRESHOLD REACHED"),
            Self::Depletion => lang.pick("APPAUVRISSEMENT", "DEPLETION"),
            Self::Alarm1 => lang.pick("ALARME 1 – ÉVACUATION", "ALARM 1 – EVACUATE"),
            Self::Alarm2 => lang.pick("ALARME 2 – DANGER", "ALARM 2 – DANGER"),
            Self::LifeThreatening => lang.pick("DANGER VITAL", "LIFE-THREATENING"),
        }
    }

    /// One-sentence meaning of the level.
    #[must_use]
    pub fn meaning(self, lang: Language) -> &'static str {
        match self {
            Self::NoDepletion => lang.pick(
                "Aucun seuil d'alarme n'est atteint dans ce modèle. Cela ne démontre pas que l'atmosphère est respirable en tout point : mesurer avant d'entrer.",
                "No alarm threshold is reached in this model. This does not demonstrate that the atmosphere is breathable everywhere: measure before entry.",
            ),
            Self::Depletion => lang.pick(
                "Appauvrissement en oxygène mesurable : indicateur de fuite, à investiguer. Seuils d'alarme non atteints dans ce modèle.",
                "Measurable oxygen depletion: leak indicator, to be investigated. Alarm thresholds not reached in this model.",
            ),
            Self::Alarm1 => lang.pick(
                "Le seuil d'alarme 1 est franchi : évacuation du local, aucune entrée sans autorisation, appareil respiratoire isolant et surveillance continue.",
                "Alarm 1 is crossed: evacuate the room; no entry without a permit, self-contained breathing apparatus and continuous monitoring.",
            ),
            Self::Alarm2 => lang.pick(
                "Le seuil d'alarme 2 est franchi : danger grave et imminent. Altération du jugement et de la coordination sans signe d'alerte ressenti.",
                "Alarm 2 is crossed: serious and imminent danger. Impaired judgement and coordination without any perceived warning.",
            ),
            Self::LifeThreatening => lang.pick(
                "Danger vital : perte de connaissance possible sans signe avant-coureur, décès en quelques minutes.",
                "Danger to life: loss of consciousness possible without warning, death within minutes.",
            ),
        }
    }

    /// Display colour (sRGB), chosen for ≥ 4.5:1 contrast with white text.
    #[must_use]
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Self::NoDepletion => (27, 94, 32),
            Self::Depletion => (102, 94, 0),
            Self::Alarm1 => (180, 83, 9),
            Self::Alarm2 => (183, 28, 28),
            Self::LifeThreatening => (74, 20, 140),
        }
    }

    /// Severity index 0–4.
    #[must_use]
    pub fn index(self) -> u8 {
        self as u8
    }
}

/// CO₂ exposure class (only when the released gas is CO₂).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Co2Level {
    /// Below the 8-h occupational limit (0.5 %)
    BelowVlep,
    /// ≥ 0.5 %: occupational exposure limit exceeded
    AboveVlep,
    /// ≥ 3 %: short-term limit exceeded
    AboveStel,
    /// ≥ 4 %: immediately dangerous to life or health
    Idlh,
    /// ≥ 10 %: rapid loss of consciousness
    Lethal,
}

impl Co2Level {
    /// Classify a peak CO₂ concentration (% vol).
    #[must_use]
    pub fn from_percent(co2: f64) -> Self {
        if co2 >= 10.0 {
            Self::Lethal
        } else if co2 >= 4.0 {
            Self::Idlh
        } else if co2 >= 3.0 {
            Self::AboveStel
        } else if co2 >= 0.5 {
            Self::AboveVlep
        } else {
            Self::BelowVlep
        }
    }

    /// Headline label when CO₂ drives the classification.
    #[must_use]
    pub fn headline(self, lang: Language) -> &'static str {
        match self {
            Self::BelowVlep => lang.pick("CO₂ SOUS LA VLEP", "CO₂ BELOW OEL"),
            Self::AboveVlep => lang.pick("CO₂ > VLEP 8 h", "CO₂ > 8-h OEL"),
            Self::AboveStel => lang.pick("CO₂ > VLCT – ÉVACUATION", "CO₂ > STEL – EVACUATE"),
            Self::Idlh => lang.pick("CO₂ > DIVS – DANGER", "CO₂ > IDLH – DANGER"),
            Self::Lethal => lang.pick("CO₂ ≥ 10 % – DANGER VITAL", "CO₂ ≥ 10% – LIFE-THREATENING"),
        }
    }

    /// Meaning when CO₂ drives the classification.
    #[must_use]
    pub fn meaning(self, lang: Language) -> &'static str {
        match self {
            Self::BelowVlep => lang.pick("Le CO₂ reste sous la valeur limite d'exposition professionnelle.", "CO₂ stays below the occupational exposure limit."),
            Self::AboveVlep => lang.pick(
                "Le pic de CO₂ dépasse la valeur de la VLEP 8 h (0,5 %) : dépassement instantané de la valeur limite (la moyenne sur 8 h n'est pas calculée), exposition anormale, fuite à rechercher. Un détecteur d'oxygène ne réagirait pas.",
                "The CO₂ peak exceeds the 8-h OEL value (0.5%): instantaneous exceedance of the limit value (the 8-h average is not computed), abnormal exposure, leak to be found. An oxygen detector would not react.",
            ),
            Self::AboveStel => lang.pick(
                "Le pic de CO₂ dépasse la valeur de la VLCT (3 %) : dépassement instantané (la moyenne sur 15 min n'est pas calculée) ; céphalées, essoufflement ; évacuation. L'oxygène reste presque normal : seul un détecteur CO₂ alerte.",
                "The CO₂ peak exceeds the STEL value (3%): instantaneous exceedance (the 15-min average is not computed); headache, breathlessness; evacuate. Oxygen stays almost normal: only a CO₂ detector warns.",
            ),
            Self::Idlh => lang.pick(
                "Le CO₂ dépasse la DIVS (4 %) : danger immédiat pour la vie ou la santé ; aucune entrée sans ARI.",
                "CO₂ exceeds the IDLH (4%): immediately dangerous to life or health; no entry without SCBA.",
            ),
            Self::Lethal => lang.pick(
                "CO₂ ≥ 10 % : perte de connaissance en quelques minutes, décès.",
                "CO₂ ≥ 10%: unconsciousness within minutes, death.",
            ),
        }
    }

    /// Equivalent overall risk level.
    #[must_use]
    pub fn risk(self) -> RiskLevel {
        match self {
            Self::BelowVlep => RiskLevel::NoDepletion,
            Self::AboveVlep => RiskLevel::Depletion,
            Self::AboveStel => RiskLevel::Alarm1,
            Self::Idlh => RiskLevel::Alarm2,
            Self::Lethal => RiskLevel::LifeThreatening,
        }
    }

    /// Short label.
    #[must_use]
    pub fn label(self, lang: Language) -> &'static str {
        match self {
            Self::BelowVlep => lang.pick("< VLEP (0,5 %)", "< OEL (0.5%)"),
            Self::AboveVlep => lang.pick("> VLEP 8 h (0,5 %)", "> 8-h OEL (0.5%)"),
            Self::AboveStel => lang.pick("> VLCT (3 %)", "> STEL (3%)"),
            Self::Idlh => lang.pick("> DIVS / IDLH (4 %)", "> IDLH (4%)"),
            Self::Lethal => lang.pick("≥ 10 % – perte de connaissance", "≥ 10% – loss of consciousness"),
        }
    }
}

/// Situations deserving a specific warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flag {
    /// Heavy gas: accumulation in low points
    HeavyGas,
    /// Light gas: accumulation at the ceiling
    LightGas,
    /// CO₂: toxic independently of O₂
    Co2Toxic,
    /// Simulation stopped at the maximum duration
    Truncated,
    /// CO₂ container liquid-full at the given temperature
    LiquidFull,
    /// Two-zone model results are indicative
    TwoZoneIndicative,
    /// Bounding instantaneous release
    InstantaneousBound,
    /// No ventilation at all
    NoVentilation,
    /// Mechanical ventilation credited: failure case must be checked
    VentilationCredited,
    /// Release flow far above the effective ventilation: ventilation barely
    /// changes the minimum O₂, the source must be addressed
    VentilationOverwhelmed,
    /// Short time between alarm 1 and alarm 2 / 10 %
    ShortEscapeTime,
    /// Cold gas from a cryogenic liquid
    ColdGas,
    /// The INRS / BCGA displacement method gives a more severe class
    DisplacementMoreSevere,
    /// Adiabatic blowdown option (indicative)
    AdiabaticIndicative,
    /// The near field (next to the release) is classified worse than the room
    NearFieldMoreSevere,
    /// The near field is included in the overall classification
    NearFieldInAssessment,
    /// Still below an alarm level when the computation ended: durations are lower bounds
    RecoveryNotObserved,
    /// Events (ventilation failure, backup fan, isolation) applied during the scenario
    Timeline,
    /// Low atmospheric pressure (altitude): the same O₂ percentage is more hypoxic
    LowAmbientPressure,
}

/// Display level of a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WarningLevel {
    /// Changes the conclusion (toxicity, more severe regulatory method, incomplete result)
    Danger,
    /// Important context (stratification, short escape time, no ventilation)
    Warning,
    /// Modelling information
    Info,
}

impl Flag {
    /// Display level.
    #[must_use]
    pub fn level(self) -> WarningLevel {
        match self {
            Self::Co2Toxic | Self::DisplacementMoreSevere | Self::Truncated | Self::LiquidFull => WarningLevel::Danger,
            Self::HeavyGas
            | Self::LightGas
            | Self::ColdGas
            | Self::ShortEscapeTime
            | Self::NoVentilation
            | Self::VentilationOverwhelmed
            | Self::NearFieldMoreSevere
            | Self::LowAmbientPressure => WarningLevel::Warning,
            Self::TwoZoneIndicative
            | Self::InstantaneousBound
            | Self::VentilationCredited
            | Self::AdiabaticIndicative
            | Self::NearFieldInAssessment
            | Self::RecoveryNotObserved
            | Self::Timeline => WarningLevel::Info,
        }
    }
}

/// Priority of a recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    /// Must be implemented before operating
    Critical,
    /// Strongly recommended
    High,
    /// Good practice
    Medium,
    /// Information
    Info,
}

impl Priority {
    /// Localised label.
    #[must_use]
    pub fn label(self, lang: Language) -> &'static str {
        match self {
            Self::Critical => lang.pick("Impératif", "Mandatory"),
            Self::High => lang.pick("Fortement recommandé", "Strongly recommended"),
            Self::Medium => lang.pick("Bonne pratique", "Good practice"),
            Self::Info => lang.pick("Information", "Information"),
        }
    }
}

/// A recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    /// Stable identifier (used to record the author's choices in the scenario file)
    pub id: &'static str,
    /// Priority
    pub priority: Priority,
    /// Short title
    pub title: String,
    /// Explanation / action
    pub text: String,
}

/// Risk assessment of a simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assessment {
    /// Alarm 1 threshold (% O₂)
    pub alarm1: f64,
    /// Alarm 2 threshold (% O₂)
    pub alarm2: f64,
    /// Classification from O₂
    pub o2_level: RiskLevel,
    /// Classification from CO₂ (CO₂ releases only)
    pub co2_level: Option<Co2Level>,
    /// Overall level (worst of O₂ and CO₂)
    pub overall: RiskLevel,
    /// Time to alarm 1 (s)
    pub t_alarm1: Option<f64>,
    /// Time to alarm 2 (s)
    pub t_alarm2: Option<f64>,
    /// Time to 10 % O₂ (s)
    pub t_10: Option<f64>,
    /// Time to CO₂ IDLH 4 % (s)
    pub t_co2_idlh: Option<f64>,
    /// Indicative time between alarm 1 and the next dangerous level (s)
    pub escape_margin: Option<f64>,
    /// Warnings
    pub flags: Vec<Flag>,
    /// Gas buoyancy
    pub buoyancy: Buoyancy,
    /// Mechanical ventilation was credited in the scenario
    pub mechanical_ventilation: bool,
    /// CO₂ (not O₂) drives the overall classification
    pub co2_driven: bool,
    /// O₂ from the INRS / BCGA displacement method (whole inventory at once)
    pub displacement_o2: f64,
    /// Initial release volume flow (m³/h), for the ventilation comparison;
    /// infinite for an instantaneous release
    #[serde(with = "crate::solver::f64_ext")]
    pub release_flow_m3h: f64,
    /// Effective ventilation η·Q (m³/h)
    pub effective_ventilation_m3h: f64,
    /// Near-field (next to the release) results, when computed
    pub near: Option<NearFieldAssessment>,
    /// Simulated duration (s)
    pub simulated_duration: f64,
    /// Timeline of the scenario (ventilation failure, backup fan, isolation), sorted
    #[serde(default)]
    pub events: Vec<crate::config::Event>,
    /// Atmospheric pressure of the room (hPa)
    #[serde(default = "default_pressure_hpa")]
    pub room_pressure_hpa: f64,
    /// Minimum partial pressure of oxygen (kPa) at the room pressure
    #[serde(default)]
    pub po2_min_kpa: f64,
    /// Partial pressure of oxygen in ambient air at the room pressure (kPa)
    #[serde(default)]
    pub po2_ambient_kpa: f64,
}

fn default_pressure_hpa() -> f64 {
    1013.25
}

/// Exposure next to the release point (near-field / far-field model).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NearFieldAssessment {
    /// Hemisphere radius (m)
    pub radius_m: f64,
    /// Random air speed at the boundary (m/s)
    pub air_speed_m_s: f64,
    /// Minimum O₂ in the near field (% vol)
    pub min_o2: f64,
    /// Time of that minimum (s)
    pub time_at_min: f64,
    /// Classification of the near field alone
    pub level: RiskLevel,
    /// Time to alarm 1 in the near field (s)
    pub t_alarm1: Option<f64>,
    /// Time to alarm 2 in the near field (s)
    pub t_alarm2: Option<f64>,
    /// Time to 10 % O₂ in the near field (s)
    pub t_10: Option<f64>,
    /// The near field is part of the overall classification
    pub in_assessment: bool,
}

impl Assessment {
    /// Assess a simulation result.
    #[must_use]
    pub fn new(config: &Configuration, result: &SimulationResult) -> Self {
        let s = &result.summary;
        let (alarm1, alarm2) = config.thresholds.levels();
        let o2_level = RiskLevel::from_o2(s.min_o2, alarm1, alarm2);
        let co2_level = s.max_co2_percent.map(Co2Level::from_percent);
        let overall = co2_level.map_or(o2_level, |c| c.risk().max(o2_level));
        let co2_driven = co2_level.is_some_and(|c| c.risk() > o2_level);

        let first = |l: f64| s.o2_crossing(l).and_then(|c| c.first);
        let t_alarm1 = first(alarm1);
        let t_alarm2 = first(alarm2);
        let t_10 = first(10.0);
        let t_co2_idlh = s.co2_crossing(4.0).and_then(|c| c.first);
        let escape_margin = match (t_alarm1, t_alarm2.or(t_10)) {
            (Some(a), Some(b)) => Some((b - a).max(0.0)),
            _ => None,
        };
        let near = s.min_o2_near.map(|min_o2| {
            let first = |l: f64| s.near_crossing(l).and_then(|c| c.first);
            NearFieldAssessment {
                radius_m: config.model.near_field_radius_m,
                air_speed_m_s: config.model.near_field_air_speed_m_s,
                min_o2,
                time_at_min: s.time_at_min_near.unwrap_or(0.0),
                level: RiskLevel::from_o2(min_o2, alarm1, alarm2),
                t_alarm1: first(alarm1),
                t_alarm2: first(alarm2),
                t_10: first(10.0),
                in_assessment: config.model.near_field_in_assessment,
            }
        });

        // Partial pressure of oxygen: what the body actually gets. At altitude
        // the same percentage is more hypoxic (ambient 21.2 kPa at sea level).
        let p_kpa = config.room.pressure_hpa / 10.0;
        let po2_min_kpa = s.min_o2 / 100.0 * p_kpa;
        let po2_ambient_kpa = crate::constants::O2_AMBIENT_PERCENT / 100.0 * p_kpa;
        let events = config.timeline();

        let mut flags = Vec::new();
        if !s.truncated && [alarm1, alarm2].iter().any(|&l| s.o2_crossing(l).is_some_and(|c| c.censored)) {
            flags.push(Flag::RecoveryNotObserved);
        }
        if !events.is_empty() {
            flags.push(Flag::Timeline);
        }
        if config.room.pressure_hpa < 900.0 {
            flags.push(Flag::LowAmbientPressure);
        }
        if let Some(n) = near {
            if n.in_assessment {
                flags.push(Flag::NearFieldInAssessment);
            } else if n.level > o2_level {
                flags.push(Flag::NearFieldMoreSevere);
            }
        }
        match s.buoyancy {
            Buoyancy::Heavy => flags.push(Flag::HeavyGas),
            Buoyancy::Light => flags.push(Flag::LightGas),
            Buoyancy::Neutral => {}
        }
        if config.gas.is_toxic() {
            flags.push(Flag::Co2Toxic);
        }
        if s.truncated {
            flags.push(Flag::Truncated);
        }
        if s.liquid_full {
            flags.push(Flag::LiquidFull);
        }
        if s.two_zone {
            flags.push(Flag::TwoZoneIndicative);
        }
        if config.release.kind == ReleaseKind::Instantaneous {
            flags.push(Flag::InstantaneousBound);
        }
        let fan_starts_later = events.iter().any(
            |e| matches!(e.action, crate::config::EventAction::VentilationStarts { flow_m3h, .. } if flow_m3h > 0.0),
        );
        if s.effective_ventilation_m3h <= 0.0 {
            if !fan_starts_later {
                flags.push(Flag::NoVentilation);
            }
        } else if config.room.mechanical_enabled && config.room.mechanical_flow_m3h > 0.0 {
            flags.push(Flag::VentilationCredited);
            // A release several times larger than the effective ventilation
            // sets the minimum almost alone (steady state x ≈ Q_gas / (Q_gas + η·Q_air)):
            // the user must not expect the ventilation to fix the scenario.
            if config.release.kind != ReleaseKind::Instantaneous
                && s.initial_volume_flow_m3h > 3.0 * s.effective_ventilation_m3h
                && o2_level >= RiskLevel::Alarm1
            {
                flags.push(Flag::VentilationOverwhelmed);
            }
        }
        if escape_margin.is_some_and(|m| m < 120.0) {
            flags.push(Flag::ShortEscapeTime);
        }
        if config.release.cold_gas && config.release.kind == ReleaseKind::ConstantFlow {
            flags.push(Flag::ColdGas);
        }
        if config.release.kind != ReleaseKind::Instantaneous
            && RiskLevel::from_o2(s.instant_displacement_o2, alarm1, alarm2) > o2_level
        {
            flags.push(Flag::DisplacementMoreSevere);
        }
        if config.model.blowdown == crate::config::Blowdown::Adiabatic && config.release.kind.uses_orifice() {
            flags.push(Flag::AdiabaticIndicative);
        }

        Self {
            alarm1,
            alarm2,
            o2_level,
            co2_level,
            overall,
            t_alarm1,
            t_alarm2,
            t_10,
            t_co2_idlh,
            escape_margin,
            flags,
            buoyancy: s.buoyancy,
            mechanical_ventilation: config.room.mechanical_enabled && config.room.mechanical_flow_m3h > 0.0,
            co2_driven,
            displacement_o2: s.instant_displacement_o2,
            release_flow_m3h: s.initial_volume_flow_m3h,
            effective_ventilation_m3h: s.effective_ventilation_m3h,
            near,
            simulated_duration: s.simulated_duration,
            events,
            room_pressure_hpa: config.room.pressure_hpa,
            po2_min_kpa,
            po2_ambient_kpa,
        }
    }

    /// `true` if the flag is set.
    #[must_use]
    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }

    /// Headline label (CO₂-specific when CO₂ drives the classification).
    #[must_use]
    pub fn headline(&self, lang: Language) -> &'static str {
        match self.co2_level {
            Some(c) if self.co2_driven => c.headline(lang),
            _ => self.overall.label(lang),
        }
    }

    /// Headline conclusion.
    #[must_use]
    pub fn conclusion(&self, lang: Language) -> String {
        if let Some(c) = self.co2_level.filter(|_| self.co2_driven) {
            return typo(c.meaning(lang), lang);
        }
        let mut text = self.overall.meaning(lang).to_string();
        if let Some(c) = self.co2_level {
            if c >= Co2Level::AboveVlep {
                text.push(' ');
                text.push_str(lang.pick(
                    "Le CO₂ est toxique par lui-même : la concentration en CO₂ est déterminante, bien avant la baisse d'oxygène.",
                    "CO₂ is toxic in its own right: the CO₂ concentration is decisive, well before oxygen falls.",
                ));
            }
        }
        typo(&text, lang)
    }

    /// Warning messages attached to the flags.
    #[must_use]
    pub fn warnings(&self, lang: Language) -> Vec<String> {
        self.flags
            .iter()
            .map(|f| {
                if *f == Flag::DisplacementMoreSevere {
                    let v = crate::i18n::fmt_pct(self.displacement_o2, 1, lang);
                    return typo(
                        &match lang {
                            Language::French => format!("La méthode réglementaire par déplacement (INRS ED 155 / BCGA CP 52 : tout l'inventaire libéré d'un coup, sans ventilation) donne {v} d'O₂, soit un classement plus sévère que la simulation dynamique. À retenir si la ventilation ou le mélange ne peuvent pas être garantis."),
                            Language::English => format!("The regulatory displacement method (INRS ED 155 / BCGA CP 52: whole inventory released at once, no ventilation) gives {v} O₂, a more severe class than the dynamic simulation. Use it if ventilation or mixing cannot be guaranteed."),
                        },
                        lang,
                    );
                }
                if *f == Flag::RecoveryNotObserved {
                    let h = crate::i18n::fmt_duration(self.simulated_duration, lang);
                    return typo(
                        &match lang {
                            Language::French => format!("À la fin du calcul ({h} simulées), l'oxygène n'est pas remonté au-dessus d'un seuil d'alarme : les durées sous les seuils sont des minimums (« ≥ »), pas des durées complètes. Allonger la durée maximale simulée pour observer le retour."),
                            Language::English => format!("At the end of the computation ({h} simulated), oxygen has not risen back above an alarm level: times below the levels are lower bounds (\"≥\"), not complete durations. Extend the maximum simulated time to observe the recovery."),
                        },
                        lang,
                    );
                }
                if matches!(f, Flag::NearFieldMoreSevere | Flag::NearFieldInAssessment) {
                    let Some(n) = self.near else { return String::new() };
                    let r = crate::i18n::fmt_num(n.radius_m, 1, lang);
                    let sp = crate::i18n::fmt_num(n.air_speed_m_s, 2, lang);
                    let x = crate::i18n::fmt_pct(n.min_o2, 1, lang);
                    let when = crate::i18n::fmt_duration(n.time_at_min, lang);
                    let lvl = n.level.label(lang);
                    let room = self.o2_level.label(lang);
                    let text = if *f == Flag::NearFieldInAssessment {
                        match lang {
                            Language::French => format!("Le classement retient aussi le champ proche (demi-sphère de {r} m autour de la fuite, air à {sp} m/s) : l'oxygène y descend à {x} ({when} après le début du rejet)."),
                            Language::English => format!("The classification also includes the near field (hemisphere of {r} m around the release, air at {sp} m/s): oxygen falls there to {x} ({when} after the start of the release)."),
                        }
                    } else {
                        match lang {
                            Language::French => format!("Près de la fuite (champ proche : demi-sphère de {r} m, air à {sp} m/s), l'oxygène descend à {x} ({when} après le début du rejet) : {lvl} à la source alors que le local est classé {room}. Ce modèle donne une moyenne de zone pour un rejet à faible vitesse ; pour un jet sous pression, l'entraînement d'air n'est pas résolu et la concentration dans l'axe du jet peut être bien plus élevée. À prendre en compte pour l'intervenant à la source (détecteur portatif, pas de travail isolé, ARI pour intervenir sur la fuite)."),
                            Language::English => format!("Next to the release (near field: hemisphere of {r} m, air at {sp} m/s), oxygen falls to {x} ({when} after the start of the release): {lvl} at the source while the room is classified {room}. The model gives a zone average for a low-momentum release; for a pressurised jet, air entrainment is not resolved and the concentration in the jet axis can be much higher. To be considered for the person at the source (personal monitor, no lone work, SCBA to intervene on the leak)."),
                        }
                    };
                    return typo(&text, lang);
                }
                if *f == Flag::Timeline {
                    let list = self.events.iter().map(|e| e.describe(lang)).collect::<Vec<_>>().join(" ; ");
                    return typo(
                        &match lang {
                            Language::French => format!("Chronologie prise en compte — {list}. Le classement couvre toute la période simulée, y compris après ces événements ; les instants sont comptés depuis le début du rejet."),
                            Language::English => format!("Timeline applied — {list}. The classification covers the whole simulated period, including after these events; times are counted from the start of the release."),
                        },
                        lang,
                    );
                }
                if *f == Flag::LowAmbientPressure {
                    let p = crate::i18n::fmt_unit(self.room_pressure_hpa, 0, "hPa", lang);
                    let amb = crate::i18n::fmt_unit(self.po2_ambient_kpa, 1, "kPa", lang);
                    let min = crate::i18n::fmt_unit(self.po2_min_kpa, 1, "kPa", lang);
                    let sea = crate::i18n::fmt_unit(21.2, 1, "kPa", lang);
                    return typo(
                        &match lang {
                            Language::French => format!("Pression atmosphérique de {p} (altitude) : l'air ambiant n'apporte que {amb} d'oxygène (pression partielle) au lieu de {sea} au niveau de la mer ; un même pourcentage d'O₂ est donc plus hypoxiant. Les seuils en % restent ceux des détecteurs ; la pression partielle d'O₂ minimale calculée est de {min}."),
                            Language::English => format!("Atmospheric pressure of {p} (altitude): ambient air only provides {amb} of oxygen (partial pressure) instead of {sea} at sea level, so the same O₂ percentage is more hypoxic. The thresholds in % remain those of the detectors; the computed minimum O₂ partial pressure is {min}."),
                        },
                        lang,
                    );
                }
                if *f == Flag::VentilationOverwhelmed {
                    let q = crate::i18n::fmt_unit(self.release_flow_m3h, 0, "m³/h", lang);
                    let v = crate::i18n::fmt_unit(self.effective_ventilation_m3h, 0, "m³/h", lang);
                    return typo(
                        &match lang {
                            Language::French => format!("Le débit de rejet initial ({q}) dépasse largement la ventilation effective ({v}) : la ventilation ne modifie presque pas le minimum d'O₂. Agir d'abord à la source (inventaire, limiteur de débit, arrêt automatique) ; Analyses → Dimensionnement donne le débit qui serait nécessaire."),
                            Language::English => format!("The initial release flow ({q}) is far above the effective ventilation ({v}): ventilation barely changes the minimum O₂. Act at the source first (inventory, flow limiter, automatic shut-off); Analyses → Sizing gives the flow that would be needed."),
                        },
                        lang,
                    );
                }
                let s = match f {
                    Flag::HeavyGas => lang.pick(
                        "Gaz plus lourd que l'air : il s'accumule au sol, dans les fosses, caniveaux et sous-sols. Concentrations locales bien pires que la moyenne du local.",
                        "Heavier than air: accumulates on the floor, in pits, trenches and basements. Local concentrations much worse than the room average.",
                    ),
                    Flag::LightGas => lang.pick(
                        "Gaz plus léger que l'air : il s'accumule en partie haute (plafond, mezzanines, travaux en hauteur).",
                        "Lighter than air: accumulates at high level (ceiling, mezzanines, work at height).",
                    ),
                    Flag::Co2Toxic => lang.pick(
                        "CO₂ : gaz toxique. Un détecteur d'oxygène ne protège pas contre le CO₂ — un détecteur CO₂ dédié (infrarouge NDIR) est nécessaire.",
                        "CO₂: toxic gas. An oxygen detector does not protect against CO₂ — a dedicated CO₂ detector (NDIR infrared) is required.",
                    ),
                    Flag::Truncated => lang.pick(
                        "Simulation interrompue à la durée maximale avant la fin de l'événement : le minimum réel peut être plus bas. Augmenter la durée maximale.",
                        "Simulation stopped at the maximum duration before the end of the event: the real minimum may be lower. Increase the maximum duration.",
                    ),
                    Flag::LiquidFull => lang.pick(
                        "Bouteille de CO₂ pleine de liquide à cette température (au-delà de ≈ 22 °C pour 0,75 kg/L) : elle rejette du liquide même debout, débit bien plus élevé (pris en compte). Domaine du liquide comprimé : la pression est prise à la saturation (64 bar à 25 °C) alors que la pression réelle est plus haute (≈ 72 bar à 25 °C pour 0,75 kg/L, CoolProp) et monte vite avec la température ; le débit initial est indicatif (sous-estimé d'environ 5 à 10 %), ne pas dimensionner finement sur ce cas.",
                        "CO₂ cylinder liquid-full at this temperature (above ≈ 22 °C at 0.75 kg/L): it discharges liquid even upright, much higher flow (accounted for). Compressed-liquid domain: the pressure is taken at saturation (64 bar at 25 °C) while the real pressure is higher (≈ 72 bar at 25 °C for 0.75 kg/L, CoolProp) and rises quickly with temperature; the initial flow is indicative (underestimated by about 5 to 10%), do not size finely on this case.",
                    ),
                    Flag::ColdGas => lang.pick(
                        "Gaz froid issu d'un liquide cryogénique : plus dense que l'air tant qu'il ne s'est pas réchauffé (même l'azote ou l'hélium), il s'accumule d'abord au sol. Modèle bi-zone activé en conséquence.",
                        "Cold gas from a cryogenic liquid: denser than air until it warms up (even nitrogen or helium), it first accumulates at floor level. Two-zone model enabled accordingly.",
                    ),
                    Flag::AdiabaticIndicative => lang.pick(
                        "Vidange adiabatique : option indicative (loi isentropique de gaz parfait, vérifiée à basse pression seulement). Le mode isotherme, plus prudent pour l'oxygène, reste la référence.",
                        "Adiabatic blowdown: indicative option (ideal-gas isentropic law, verified at low pressure only). The isothermal mode, more conservative for oxygen, remains the reference.",
                    ),
                    Flag::DisplacementMoreSevere => "",
                    Flag::TwoZoneIndicative => lang.pick(
                        "Modèle bi-zone indicatif : ses paramètres (épaisseur de couche, échange inter-zones) sont des hypothèses à justifier. Le classement retient le plus défavorable des deux modèles.",
                        "Two-zone model is indicative: its parameters (layer thickness, inter-zone exchange) are assumptions to be justified. The classification uses the worse of the two models.",
                    ),
                    Flag::InstantaneousBound => lang.pick(
                        "Rejet instantané : cas enveloppe (tout l'inventaire libéré d'un coup), à utiliser comme borne supérieure.",
                        "Instantaneous release: bounding case (whole inventory released at once), to be used as an upper bound.",
                    ),
                    Flag::NoVentilation => lang.pick(
                        "Aucune ventilation : la concentration ne redescend pas après la fuite.",
                        "No ventilation: the concentration does not decrease after the release.",
                    ),
                    Flag::VentilationCredited => lang.pick(
                        "La ventilation mécanique est prise en compte : vérifier aussi le scénario de panne de ventilation et surveiller son bon fonctionnement (contrôleur de débit).",
                        "Mechanical ventilation is credited: also check the ventilation-failure scenario and monitor its operation (flow switch).",
                    ),
                    Flag::ShortEscapeTime => lang.pick(
                        "Délai très court entre l'alarme 1 et le niveau dangereux suivant : le temps de réponse du détecteur (T90) et le temps de sortie peuvent ne pas suffire.",
                        "Very short time between alarm 1 and the next dangerous level: detector response time (T90) and egress time may not be sufficient.",
                    ),
                    // Formatted above (need numbers).
                    Flag::VentilationOverwhelmed
                    | Flag::NearFieldMoreSevere
                    | Flag::NearFieldInAssessment
                    | Flag::RecoveryNotObserved
                    | Flag::Timeline
                    | Flag::LowAmbientPressure => "",
                };
                typo(s, lang)
            })
            .collect()
    }

    /// Warnings with their display level, most severe first.
    #[must_use]
    pub fn warnings_ranked(&self, lang: Language) -> Vec<(WarningLevel, String)> {
        let mut v: Vec<(WarningLevel, String)> =
            self.flags.iter().map(|f| f.level()).zip(self.warnings(lang)).collect();
        v.sort_by_key(|(l, _)| *l);
        v
    }

    /// Recommendations, most important first.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn recommendations(&self, lang: Language) -> Vec<Recommendation> {
        let mut out: Vec<Recommendation> = Vec::new();
        let level = self.overall;
        let a1 = crate::i18n::fmt_pct(self.alarm1, 1, lang);
        let a2 = crate::i18n::fmt_pct(self.alarm2, 1, lang);
        let placement = match self.buoyancy {
            Buoyancy::Heavy => lang
                .pick("près du sol (≈ 0,3 m) et dans les points bas", "close to the floor (≈ 0.3 m) and in low points"),
            Buoyancy::Light => lang.pick("en partie haute, près du plafond", "at high level, near the ceiling"),
            Buoyancy::Neutral => {
                lang.pick("à hauteur des voies respiratoires (≈ 1,5 m)", "at breathing height (≈ 1.5 m)")
            }
        };

        if level >= RiskLevel::LifeThreatening {
            out.push(mk(lang,
                "source_reduction",
                Priority::Critical,
                "Scénario inacceptable en l'état",
                "Scenario unacceptable as is",
                "Réduire le risque à la source avant exploitation : diminuer la quantité de gaz dans le local, déplacer le stockage à l'extérieur ou en armoire ventilée, installer un limiteur de débit / orifice de restriction.",
                "Reduce the risk at the source before operation: reduce the gas inventory in the room, move storage outdoors or to a ventilated cabinet, fit a flow limiter / restriction orifice.",
            ));
        }
        if level >= RiskLevel::Alarm1 {
            out.push(Recommendation {
                id: "o2_detection",
                priority: Priority::Critical,
                title: typo(lang.pick("Détection fixe d'oxygène", "Fixed oxygen detection"), lang),
                text: typo(
                    &match lang {
                        Language::French => format!(
                            "Installer une détection O₂ fixe (EN 50104), capteurs {placement}, avec alarme 1 à {a1} et alarme 2 à {a2}, report d'alarme sonore et lumineuse à l'intérieur et à l'entrée du local. Étalonnage et tests périodiques."
                        ),
                        Language::English => format!(
                            "Install fixed O₂ detection (EN 50104), sensors {placement}, alarm 1 at {a1} and alarm 2 at {a2}, audible and visual alarms inside and at the entrance. Periodic calibration and bump tests."
                        ),
                    },
                    lang,
                ),
            });
            out.push(mk(lang,
                "alarm_response",
                Priority::Critical,
                "Conduite à tenir en cas d'alarme",
                "Alarm response",
                "Évacuation immédiate dès l'alarme 1. Ne jamais entrer pour porter secours sans appareil respiratoire isolant (ARI) : environ 60 % des victimes en espace confiné sont des sauveteurs (NIOSH).",
                "Immediate evacuation at alarm 1. Never enter to rescue without self-contained breathing apparatus (SCBA): about 60% of confined-space fatalities are would-be rescuers (NIOSH).",
            ));
            out.push(mk(lang,
                "respiratory_protection",
                Priority::Critical,
                "Protection respiratoire",
                "Respiratory protection",
                "Seuls les appareils respiratoires isolants (ARI) protègent en atmosphère appauvrie en oxygène. Les masques filtrants sont inefficaces.",
                "Only self-contained breathing apparatus (SCBA) protects in an oxygen-deficient atmosphere. Filter masks are ineffective.",
            ));
            out.push(mk(lang,
                "interlock",
                Priority::High,
                "Coupure automatique et ventilation asservie",
                "Automatic shut-off and interlocked ventilation",
                "Asservir à la détection une vanne de coupure automatique de l'alimentation en gaz et le passage de la ventilation en grande vitesse.",
                "Interlock the detection with an automatic gas supply shut-off valve and with high-speed ventilation.",
            ));
            out.push(mk(lang,
                "training",
                Priority::High,
                "Information et formation",
                "Information and training",
                "Former le personnel au risque d'anoxie (absence de signe d'alerte), signaler le risque à l'entrée du local, fournir des détecteurs portatifs O₂ aux intervenants.",
                "Train staff on anoxia (no warning signs), post hazard signage at the entrance, provide personal O₂ monitors to workers.",
            ));
        }
        if level >= RiskLevel::Alarm2 {
            out.push(mk(lang,
                "entry_permit",
                Priority::Critical,
                "Accès et intervention",
                "Entry and intervention",
                "Toute intervention après alarme : permis de travail, mesure préalable et continue de l'atmosphère, surveillant extérieur, ARI et plan de secours écrit.",
                "Any entry after an alarm: work permit, pre-entry and continuous atmospheric testing, attendant outside, SCBA and a written rescue plan.",
            ));
            out.push(mk(lang,
                "ventilation_sizing",
                Priority::High,
                "Dimensionner la ventilation",
                "Size the ventilation",
                "Utiliser le calcul « ventilation nécessaire » (onglet Analyses) pour dimensionner une extraction maintenant l'O₂ au-dessus de l'alarme 1, en tenant compte de l'efficacité réelle de brassage.",
                "Use the \"required ventilation\" calculation (Analyses tab) to size an extraction keeping O₂ above alarm 1, accounting for the real mixing effectiveness.",
            ));
        }
        if level == RiskLevel::Depletion {
            out.push(mk(lang,
                "leak_indicator",
                Priority::High,
                "Indicateur de fuite",
                "Leak indicator",
                "Un appauvrissement mesurable traduit une fuite : contrôle d'étanchéité périodique des raccords et détendeurs, fermeture des bouteilles hors utilisation.",
                "Measurable depletion indicates a leak: periodic leak testing of fittings and regulators, close cylinders when not in use.",
            ));
            out.push(Recommendation {
                id: "o2_detection_consider",
                priority: Priority::High,
                title: typo(lang.pick("Détection d'oxygène", "Oxygen detection"), lang),
                text: typo(
                    &match lang {
                        Language::French => format!(
                            "Envisager une détection O₂ fixe ou portative (capteurs {placement}) avec alarme à {a1}, notamment si la quantité de gaz ou le débit peuvent augmenter."
                        ),
                        Language::English => format!(
                            "Consider fixed or personal O₂ detection (sensors {placement}) with an alarm at {a1}, especially if the gas inventory or flow may increase."
                        ),
                    },
                    lang,
                ),
            });
        }
        if level == RiskLevel::NoDepletion {
            out.push(mk(lang,
                "robustness_check",
                Priority::Info,
                "Aucun seuil franchi dans ce modèle",
                "No threshold reached in this model",
                "Vérifier la robustesse du résultat : scénario de panne de ventilation, cas enveloppe (rejet instantané), analyse de sensibilité. Maintenir les mesures de prévention de base (étanchéité, entretien, signalisation).",
                "Check the robustness of the result: ventilation failure, bounding case (instantaneous release), sensitivity analysis. Keep basic prevention measures (leak tightness, maintenance, signage).",
            ));
        }
        if let Some(c) = self.co2_level {
            if c >= Co2Level::AboveVlep {
                out.push(mk(lang,
                    "co2_detection",
                    Priority::Critical,
                    "Détection CO₂ dédiée",
                    "Dedicated CO₂ detection",
                    "Installer des détecteurs CO₂ infrarouges (NDIR) en partie basse, alarmes typiques à 0,5 % (VLEP 8 h) et 3 % (VLCT). Le CO₂ provoque céphalées, essoufflement puis perte de connaissance bien avant que l'O₂ ne baisse significativement.",
                    "Install infrared (NDIR) CO₂ detectors at low level, typical alarms at 0.5% (8-h OEL) and 3% (STEL). CO₂ causes headache, breathlessness and then unconsciousness well before O₂ falls significantly.",
                ));
            }
        }
        match self.buoyancy {
            Buoyancy::Heavy if level >= RiskLevel::Depletion => out.push(mk(lang,
                "low_points",
                Priority::High,
                "Points bas",
                "Low points",
                "Interdire le travail allongé ou en fosse sans mesure préalable, placer l'extraction en partie basse, supprimer ou ventiler les points bas (fosses, caniveaux).",
                "Forbid work lying down or in pits without prior testing, place extraction at low level, remove or ventilate low points (pits, trenches).",
            )),
            Buoyancy::Light if level >= RiskLevel::Depletion => out.push(mk(lang,
                "high_points",
                Priority::High,
                "Parties hautes",
                "High points",
                "Placer l'extraction en partie haute ; mesurer l'atmosphère avant tout travail en hauteur (nacelle, mezzanine).",
                "Place extraction at high level; test the atmosphere before any work at height (lift, mezzanine).",
            )),
            _ => {}
        }
        if let Some(n) = self.near.filter(|n| n.level >= RiskLevel::Alarm1) {
            let r = crate::i18n::fmt_num(n.radius_m, 1, lang);
            out.push(Recommendation {
                id: "near_field_intervention",
                priority: if n.level >= RiskLevel::Alarm2 { Priority::Critical } else { Priority::High },
                title: typo(lang.pick("Intervenant à la source", "Person at the source"), lang),
                text: typo(
                    &match lang {
                        Language::French => format!("À moins de {r} m du point de fuite, l'oxygène baisse avant le reste du local : détecteur O₂ portatif avec alarme pour toute personne travaillant à la source, pas de travail isolé, coupure d'urgence de l'alimentation en gaz à portée de main, ARI pour intervenir sur une fuite active."),
                        Language::English => format!("Within {r} m of the leak, oxygen falls before the rest of the room: personal O₂ monitor with alarm for anyone working at the source, no lone work, emergency gas shut-off within reach, SCBA to intervene on an active leak."),
                    },
                    lang,
                ),
            });
        }
        if self.mechanical_ventilation && level >= RiskLevel::Depletion {
            out.push(mk(lang,
                "ventilation_reliability",
                Priority::High,
                "Fiabilité de la ventilation",
                "Ventilation reliability",
                "La ventilation est une barrière de sécurité : contrôleur de débit avec alarme de défaut, maintenance préventive, et vérification du scénario de panne.",
                "Ventilation is a safety barrier: flow switch with fault alarm, preventive maintenance, and check of the failure scenario.",
            ));
        }
        out.push(mk(lang,
            "risk_assessment",
            Priority::Medium,
            "Évaluation des risques",
            "Risk assessment",
            "Intégrer ce scénario au document unique (EvRP) et faire valider les hypothèses par une personne compétente.",
            "Include this scenario in the workplace risk assessment and have the assumptions validated by a competent person.",
        ));
        out.sort_by_key(|r| r.priority);
        out
    }
}

fn mk(
    lang: Language,
    id: &'static str,
    p: Priority,
    fr_title: &str,
    en_title: &str,
    fr: &str,
    en: &str,
) -> Recommendation {
    Recommendation {
        id,
        priority: p,
        title: typo(lang.pick(fr_title, en_title), lang),
        text: typo(lang.pick(fr, en), lang),
    }
}

/// Physiological effects of oxygen deficiency (healthy adult at rest).
#[must_use]
pub fn o2_effects_table(lang: Language) -> Vec<(&'static str, &'static str)> {
    vec![
        ("20,95", lang.pick("Air ambiant", "Ambient air")),
        (
            "< 19,5",
            lang.pick(
                "Atmosphère sous-oxygénée (OSHA 1910.146) — entrée interdite sans ARI",
                "Oxygen-deficient atmosphere (OSHA 1910.146) — no entry without SCBA",
            ),
        ),
        (
            "19 – 17",
            lang.pick(
                "Seuils d'alarme usuels en France (INRS). Début possible de baisse des performances, sans sensation",
                "Usual alarm levels in France (INRS). Possible early loss of performance, not perceived",
            ),
        ),
        (
            "16 – 12",
            lang.pick(
                "Respiration et pouls accélérés, jugement et coordination altérés",
                "Faster breathing and pulse, impaired judgement and coordination",
            ),
        ),
        (
            "< 11",
            lang.pick(
                "Évanouissement possible en quelques minutes, sans signe avant-coureur (EIGA Doc. 44)",
                "Fainting possible within minutes, without warning (EIGA Doc. 44)",
            ),
        ),
        (
            "10 – 8",
            lang.pick("Défaillance mentale, nausées, perte de connaissance", "Mental failure, nausea, unconsciousness"),
        ),
        (
            "8 – 6",
            lang.pick(
                "Décès en ≈ 8 min ; réanimation possible si immédiate",
                "Death in ≈ 8 min; resuscitation possible if immediate",
            ),
        ),
        ("< 6", lang.pick("Coma en ≈ 40 s, arrêt respiratoire, décès", "Coma in ≈ 40 s, respiratory arrest, death")),
    ]
}

/// Effects of CO₂ (as a toxic gas).
#[must_use]
pub fn co2_effects_table(lang: Language) -> Vec<(&'static str, &'static str)> {
    vec![
        ("0,5", lang.pick("Valeur de la VLEP 8 h (valeur limite indicative, arrêté du 30 juin 2004 modifié ; directive 2006/15/CE) / OSHA PEL — comparée au pic, pas à une moyenne", "8-h OEL value (EU 2006/15/EC) / OSHA PEL — compared with the peak, not an average")),
        (
            "3",
            lang.pick(
                "VLCT 15 min (ACGIH STEL) : essoufflement, céphalées",
                "15-min STEL (ACGIH): breathlessness, headache",
            ),
        ),
        ("4", lang.pick("DIVS / IDLH (NIOSH) : danger immédiat", "IDLH (NIOSH): immediately dangerous")),
        (
            "5 – 10",
            lang.pick(
                "Céphalées violentes, vertiges, troubles visuels, perte de connaissance",
                "Severe headache, dizziness, visual disturbance, unconsciousness",
            ),
        ),
        (
            "> 10",
            lang.pick("Perte de connaissance en quelques minutes, décès", "Unconsciousness within minutes, death"),
        ),
    ]
}

/// Limitations of the model, to be shown with every result.
#[must_use]
pub fn model_limitations(lang: Language) -> Vec<String> {
    let items: [(&str, &str); 8] = [
        ("Local considéré à pression constante (non étanche) ; gaz rejeté supposé à la température du local une fois mélangé.",
         "Room assumed at constant pressure (not airtight); released gas assumed at room temperature once mixed."),
        ("Modèle « bien mélangé » : concentration moyenne du local. Le champ proche (demi-sphère autour de la fuite, modèle Nicas / IH Mod / ODHMOD) est une moyenne de zone estimée à part ; il suppose un rejet à faible vitesse et ne borne pas la concentration dans l'axe d'un jet sous pression.",
         "Well-mixed model: room-average concentration. The near field (hemisphere around the leak, Nicas / IH Mod / ODHMOD model) is a zone average estimated separately; it assumes a low-momentum release and does not bound the concentration in the axis of a pressurised jet."),
        ("Modèle bi-zone : indicatif ; épaisseur de couche et échange inter-zones sont des hypothèses utilisateur.",
         "Two-zone model: indicative; layer thickness and inter-zone exchange are user assumptions."),
        ("Vidange isotherme par défaut (échanges avec les parois) : débit légèrement surestimé pour les vidanges rapides (prudent) ; l'option adiabatique est indicative.",
         "Isothermal blowdown by default (wall heat exchange): flow slightly overestimated for fast blowdowns (conservative); the adiabatic option is indicative."),
        ("CO₂ : équilibre liquide-vapeur à température constante ; le refroidissement réel ralentit la vidange. Sortie liquide : borne haute (écoulement non flashant).",
         "CO₂: liquid-vapour equilibrium at constant temperature; real cooling slows the release. Liquid withdrawal: upper bound (non-flashing flow)."),
        ("Liquides cryogéniques : seule une évaporation à débit imposé est modélisée (1 L de liquide ≈ 692 L de N₂, 840 L d'Ar, 752 L d'He à 20 °C) ; les flaques, nuages froids et jets ne le sont pas.",
         "Cryogenic liquids: only boil-off at an imposed flow is modelled (1 L of liquid ≈ 692 L of N₂, 840 L of Ar, 752 L of He at 20 °C); pools, cold clouds and jets are not."),
        ("Coefficient de décharge et efficacité de ventilation : valeurs à justifier ; tester leur influence avec l'analyse de sensibilité.",
         "Discharge coefficient and ventilation effectiveness: values to be justified; test their influence with the sensitivity analysis."),
        ("Ne remplace ni l'évaluation des risques par une personne compétente, ni le permis de travail, ni la mesure continue de l'atmosphère par un détecteur étalonné.",
         "Does not replace a risk assessment by a competent person, a work permit or continuous atmospheric monitoring with a calibrated detector."),
    ];
    items.iter().map(|(fr, en)| typo(lang.pick(fr, en), lang)).collect()
}

/// Short disclaimer.
#[must_use]
pub fn disclaimer(lang: Language) -> String {
    typo(
        lang.pick(
            "Outil d'aide à l'évaluation des risques. Modèle simplifié : les résultats ne doivent jamais servir à déclarer une atmosphère sûre. Ils doivent être interprétés par une personne compétente en prévention des risques ; seule la mesure par un détecteur étalonné fait foi.",
            "Risk assessment support tool. Simplified model: results must never be used to declare an atmosphere safe. They must be interpreted by a competent safety professional; only measurement with a calibrated detector is authoritative.",
        ),
        lang,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StratificationMode;
    use crate::gas::GasType;

    #[test]
    fn classification_bands() {
        assert_eq!(RiskLevel::from_o2(20.95, 19.0, 17.0), RiskLevel::NoDepletion);
        assert_eq!(RiskLevel::from_o2(20.0, 19.0, 17.0), RiskLevel::Depletion);
        assert_eq!(RiskLevel::from_o2(18.5, 19.0, 17.0), RiskLevel::Alarm1);
        assert_eq!(RiskLevel::from_o2(16.0, 19.0, 17.0), RiskLevel::Alarm2);
        assert_eq!(RiskLevel::from_o2(9.0, 19.0, 17.0), RiskLevel::LifeThreatening);
        // OSHA profile: 19.4 % is already alarm 1
        assert_eq!(RiskLevel::from_o2(19.4, 19.5, 16.0), RiskLevel::Alarm1);
    }

    #[test]
    fn co2_classes() {
        assert_eq!(Co2Level::from_percent(0.3), Co2Level::BelowVlep);
        assert_eq!(Co2Level::from_percent(4.2), Co2Level::Idlh);
        assert_eq!(Co2Level::from_percent(4.2).risk(), RiskLevel::Alarm2);
    }

    #[test]
    fn no_text_ever_says_safe() {
        for lang in [Language::French, Language::English] {
            for gas in GasType::all() {
                for d in [0.2, 2.0, 10.0] {
                    let mut c = Configuration { gas, ..Configuration::default() };
                    c.release.diameter_mm = d;
                    let run = crate::run(&c).unwrap();
                    let mut all = run.assessment.conclusion(lang);
                    for r in run.assessment.recommendations(lang) {
                        all.push_str(&r.title);
                        all.push_str(&r.text);
                    }
                    let lower = all.to_lowercase();
                    assert!(!lower.contains("sûr ") && !lower.contains("conditions sûres"), "{lower}");
                    assert!(!lower.contains(" safe ") && !lower.contains("safe conditions"), "{lower}");
                }
            }
        }
    }

    #[test]
    fn co2_overrides_o2_classification() {
        let mut c = Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() };
        c.room.volume_m3 = 400.0;
        c.release.diameter_mm = 1.0;
        c.model.stratification = StratificationMode::Off;
        let run = crate::run(&c).unwrap();
        let a = &run.assessment;
        assert!(a.co2_level.unwrap() >= Co2Level::Idlh);
        assert!(a.overall > a.o2_level || a.o2_level >= RiskLevel::Alarm2);
        assert!(a.recommendations(Language::French).iter().any(|r| r.title.contains("CO₂")));
    }

    #[test]
    fn scba_wording_present_when_alarm() {
        let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        c.room.volume_m3 = 15.0;
        let run = crate::run(&c).unwrap();
        assert!(run.assessment.overall >= RiskLevel::Alarm1);
        let recs = run.assessment.recommendations(Language::French);
        assert!(recs.iter().any(|r| r.text.contains("ARI")));
        assert!(recs.iter().any(|r| r.text.contains("filtrants sont inefficaces")));
    }

    #[test]
    fn french_typography() {
        assert_eq!(typo("Seuil : 19 %", Language::French), "Seuil\u{a0}: 19\u{a0}%");
        assert_eq!(typo("Seuil : 19 %", Language::English), "Seuil : 19 %");
    }
}
