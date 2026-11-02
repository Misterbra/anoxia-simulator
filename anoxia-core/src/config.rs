//! Simulation configuration: inputs, validation, (de)serialisation and
//! migration of files written by earlier versions.

use crate::error::{check_range, AnoxiaError, Result};
use crate::gas::{Buoyancy, GasType};
use crate::i18n::Language;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Current version of the configuration file format.
pub const SCHEMA_VERSION: u32 = 2;

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

/// Complete, self-contained description of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Configuration {
    /// File format version
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Project identification (shown in reports)
    pub identification: Identification,
    /// Released gas
    pub gas: GasType,
    /// Gas container
    pub source: SourceParameters,
    /// Release scenario
    pub release: ReleaseParameters,
    /// Room and ventilation
    pub room: RoomParameters,
    /// Advanced model options
    pub model: ModelOptions,
    /// Alarm thresholds
    pub thresholds: ThresholdProfile,
    /// Author's choices for the report (retained recommendations, comments)
    pub report: ReportChoices,
    /// Planned change of the installation, for the before / after study
    pub future: FutureChange,
    /// Events during the release (ventilation failure, backup fan, isolation
    /// of the leak), at a given time after the start of the release
    pub events: Vec<Event>,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            identification: Identification::default(),
            gas: GasType::Argon,
            source: SourceParameters::default(),
            release: ReleaseParameters::default(),
            room: RoomParameters::default(),
            model: ModelOptions::default(),
            thresholds: ThresholdProfile::default(),
            report: ReportChoices::default(),
            future: FutureChange::default(),
            events: Vec::new(),
        }
    }
}

/// What happens at a given time after the start of the release.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EventAction {
    /// The mechanical ventilation stops (failure, power cut); only the
    /// natural air change remains
    VentilationStops,
    /// A mechanical ventilation (re)starts at the given flow: backup fan,
    /// emergency extraction triggered by the detector, restart after a failure
    VentilationStarts {
        /// Flow of the fan that starts (m³/h)
        flow_m3h: f64,
        /// Exhaust or supply
        mode: VentilationMode,
    },
    /// The release is isolated (valve closed, automatic shut-off): no more
    /// gas enters the room
    ReleaseIsolated,
}

impl EventAction {
    /// Localised description, with the flow for a ventilation start.
    #[must_use]
    pub fn describe(self, lang: Language) -> String {
        match self {
            Self::VentilationStops => {
                lang.pick("arrêt de la ventilation mécanique", "mechanical ventilation stops").to_string()
            }
            Self::VentilationStarts { flow_m3h, mode } => format!(
                "{} {} ({})",
                lang.pick("démarrage d'une ventilation de", "a ventilation of"),
                crate::i18n::fmt_unit(flow_m3h, 0, "m³/h", lang),
                match mode {
                    VentilationMode::Extraction => lang.pick("extraction", "exhaust, starts"),
                    VentilationMode::Supply => lang.pick("soufflage", "supply, starts"),
                }
            ),
            Self::ReleaseIsolated => lang
                .pick("isolement de la fuite (plus de gaz rejeté)", "leak isolated (no more gas released)")
                .to_string(),
        }
    }

    /// Short label for chart markers ("panne ventilation", "fuite isolée").
    #[must_use]
    pub fn short(self, lang: Language) -> String {
        match self {
            Self::VentilationStops => lang.pick("panne ventilation", "ventilation stops").to_string(),
            Self::VentilationStarts { flow_m3h, .. } => {
                format!(
                    "{} {}",
                    lang.pick("ventilation", "ventilation"),
                    crate::i18n::fmt_unit(flow_m3h, 0, "m³/h", lang)
                )
            }
            Self::ReleaseIsolated => lang.pick("fuite isolée", "leak isolated").to_string(),
        }
    }

    /// Order of application for simultaneous events (stops, then starts,
    /// then isolation), so that the timeline is deterministic.
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::VentilationStops => 0,
            Self::VentilationStarts { .. } => 1,
            Self::ReleaseIsolated => 2,
        }
    }

    /// `true` for an event that changes the mechanical ventilation.
    #[must_use]
    pub fn concerns_ventilation(self) -> bool {
        !matches!(self, Self::ReleaseIsolated)
    }
}

/// A dated event of the scenario timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Time after the start of the release (s)
    pub time_s: f64,
    /// What happens
    pub action: EventAction,
}

impl Event {
    /// Localised sentence: "5 min 00 s après le début du rejet : arrêt de la
    /// ventilation mécanique".
    #[must_use]
    pub fn describe(&self, lang: Language) -> String {
        let when = if self.time_s < 1.0 {
            lang.pick("dès le début du rejet", "from the start of the release").to_string()
        } else {
            format!(
                "{} {}",
                crate::i18n::fmt_duration(self.time_s, lang),
                lang.pick("après le début du rejet", "after the start of the release")
            )
        };
        format!("{when} : {}", self.action.describe(lang))
    }
}

/// Planned change of the installation (added machine, cylinders or tank, new
/// ventilation), compared with the current scenario in the "before / after"
/// study. Each field overrides the current value when set; the other inputs
/// (room, leak hypothesis, thresholds) stay identical so that the comparison
/// isolates the change. Adding cylinders does not by itself mean that they all
/// leak at once: the leak hypothesis remains the one of the scenario.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FutureChange {
    /// Study enabled (the four configurations are computed and reported)
    pub enabled: bool,
    /// Free description of the change ("ajout d'un congélateur −80 °C", "2 B50 supplémentaires")
    pub label: String,
    /// Future number of containers
    pub source_count: Option<u32>,
    /// Future container water capacity (L)
    pub volume_liters: Option<f64>,
    /// Future container gauge pressure (bar)
    pub pressure_bar_g: Option<f64>,
    /// Future leak diameter (mm)
    pub diameter_mm: Option<f64>,
    /// Future imposed flow (m³/h), constant-flow scenarios
    pub constant_flow_m3h: Option<f64>,
    /// Future mechanical ventilation flow (m³/h); 0 = no mechanical ventilation
    pub mechanical_flow_m3h: Option<f64>,
}

impl FutureChange {
    /// `true` when no value differs from the current installation.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.source_count.is_none()
            && self.volume_liters.is_none()
            && self.pressure_bar_g.is_none()
            && self.diameter_mm.is_none()
            && self.constant_flow_m3h.is_none()
            && self.mechanical_flow_m3h.is_none()
    }

    /// The future configuration: the current one with the overrides applied.
    #[must_use]
    pub fn apply(&self, base: &Configuration) -> Configuration {
        let mut c = base.clone();
        if let Some(n) = self.source_count {
            c.source.count = n.max(1);
        }
        if let Some(v) = self.volume_liters {
            c.source.volume_liters = v;
        }
        if let Some(p) = self.pressure_bar_g {
            c.source.pressure_bar_g = p;
        }
        if let Some(d) = self.diameter_mm {
            c.release.diameter_mm = d;
        }
        if let Some(q) = self.constant_flow_m3h {
            c.release.constant_flow_m3h = q;
        }
        if let Some(q) = self.mechanical_flow_m3h {
            c.room.mechanical_enabled = q > 0.0;
            c.room.mechanical_flow_m3h = q.max(0.0);
        }
        c
    }

    /// Human-readable list of the differences ("Nombre de récipients : 2 → 4").
    #[must_use]
    pub fn differences(&self, base: &Configuration, lang: crate::i18n::Language) -> Vec<String> {
        use crate::i18n::fmt_unit;
        let mut out = Vec::new();
        if let Some(n) = self.source_count {
            out.push(format!(
                "{} : {} → {}",
                lang.pick("Nombre de récipients", "Number of containers"),
                base.source.count,
                n
            ));
        }
        if let Some(v) = self.volume_liters {
            out.push(format!(
                "{} : {} → {}",
                lang.pick("Capacité en eau", "Water capacity"),
                fmt_unit(base.source.volume_liters, 1, "L", lang),
                fmt_unit(v, 1, "L", lang)
            ));
        }
        if let Some(p) = self.pressure_bar_g {
            out.push(format!(
                "{} : {} → {}",
                lang.pick("Pression", "Pressure"),
                fmt_unit(base.source.pressure_bar_g, 0, "bar", lang),
                fmt_unit(p, 0, "bar", lang)
            ));
        }
        if let Some(d) = self.diameter_mm {
            out.push(format!(
                "{} : {} → {}",
                lang.pick("Diamètre de fuite", "Leak diameter"),
                fmt_unit(base.release.diameter_mm, 2, "mm", lang),
                fmt_unit(d, 2, "mm", lang)
            ));
        }
        if let Some(q) = self.constant_flow_m3h {
            out.push(format!(
                "{} : {} → {}",
                lang.pick("Débit imposé", "Imposed flow"),
                fmt_unit(base.release.constant_flow_m3h, 1, "m³/h", lang),
                fmt_unit(q, 1, "m³/h", lang)
            ));
        }
        if let Some(q) = self.mechanical_flow_m3h {
            let current = if base.room.mechanical_enabled { base.room.mechanical_flow_m3h } else { 0.0 };
            out.push(format!(
                "{} : {} → {}",
                lang.pick("Ventilation mécanique", "Mechanical ventilation"),
                fmt_unit(current, 0, "m³/h", lang),
                fmt_unit(q, 0, "m³/h", lang)
            ));
        }
        out
    }
}

/// Choices made by the author of the study for the report. The software
/// proposes recommendations; the person who signs decides which ones are
/// retained and can justify the others ("already in place", "not applicable").
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportChoices {
    /// Identifiers of the proposed recommendations the author did not retain
    pub excluded: Vec<String>,
    /// Author's comment per recommendation identifier
    pub notes: std::collections::BTreeMap<String, String>,
    /// Additional measures written by the author
    pub additional: String,
}

impl ReportChoices {
    /// `true` unless the author excluded the recommendation.
    #[must_use]
    pub fn is_retained(&self, id: &str) -> bool {
        !self.excluded.iter().any(|x| x == id)
    }

    /// Retain or exclude a recommendation.
    pub fn set_retained(&mut self, id: &str, retained: bool) {
        self.excluded.retain(|x| x != id);
        if !retained {
            self.excluded.push(id.to_string());
        }
    }

    /// Author's comment for a recommendation (empty if none).
    #[must_use]
    pub fn note(&self, id: &str) -> &str {
        self.notes.get(id).map_or("", String::as_str)
    }

    /// Set (or clear, when blank) the author's comment for a recommendation.
    pub fn set_note(&mut self, id: &str, note: &str) {
        if note.trim().is_empty() {
            self.notes.remove(id);
        } else {
            self.notes.insert(id.to_string(), note.to_string());
        }
    }
}

/// Project identification.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Identification {
    /// Document reference (e.g. "EVR-2026-014")
    pub reference: String,
    /// Site / unit / department
    pub unit: String,
    /// Building
    pub building: String,
    /// Room or installation
    pub installation: String,
    /// Author of the study
    pub user: String,
    /// Reviewer / approver
    pub reviewer: String,
    /// Free comments
    pub comments: String,
}

/// Standard container sizes (water capacity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BottleType {
    /// 2 L cylinder
    B2,
    /// 5 L cylinder
    B5,
    /// 10 L cylinder
    B10,
    /// 20 L cylinder
    B20,
    /// 50 L cylinder
    B50,
    /// 100 L tank
    Tank100,
    /// 200 L tank
    Tank200,
    /// 500 L tank
    Tank500,
    /// 5000 L tank
    Tank5000,
    /// Custom water capacity
    Custom,
}

impl BottleType {
    /// All container types in display order.
    #[must_use]
    pub fn all() -> [Self; 10] {
        [
            Self::B2,
            Self::B5,
            Self::B10,
            Self::B20,
            Self::B50,
            Self::Tank100,
            Self::Tank200,
            Self::Tank500,
            Self::Tank5000,
            Self::Custom,
        ]
    }

    /// Water capacity in litres (`None` for custom).
    #[must_use]
    pub fn volume_liters(self) -> Option<f64> {
        match self {
            Self::B2 => Some(2.0),
            Self::B5 => Some(5.0),
            Self::B10 => Some(10.0),
            Self::B20 => Some(20.0),
            Self::B50 => Some(50.0),
            Self::Tank100 => Some(100.0),
            Self::Tank200 => Some(200.0),
            Self::Tank500 => Some(500.0),
            Self::Tank5000 => Some(5000.0),
            Self::Custom => None,
        }
    }

    /// Localised display name.
    #[must_use]
    pub fn name(self, lang: Language) -> String {
        match self {
            Self::B2 | Self::B5 | Self::B10 | Self::B20 | Self::B50 => {
                let v = self.volume_liters().unwrap_or_default();
                format!("{self:?} ({v:.0}\u{a0}L)")
            }
            Self::Tank100 | Self::Tank200 | Self::Tank500 | Self::Tank5000 => {
                let v = self.volume_liters().unwrap_or_default();
                format!("{} {v:.0}\u{a0}L", lang.pick("Réservoir", "Tank"))
            }
            Self::Custom => lang.pick("Autre (saisie libre)", "Other (custom)").to_string(),
        }
    }

    /// Container matching a given capacity, or `Custom`.
    #[must_use]
    pub fn from_volume(volume: f64) -> Self {
        Self::all()
            .into_iter()
            .find(|b| b.volume_liters().is_some_and(|v| (v - volume).abs() < 0.01))
            .unwrap_or(Self::Custom)
    }
}

/// How CO₂ leaves a liquefied-gas cylinder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Co2Withdrawal {
    /// From the vapour phase (cylinder upright, no dip tube)
    #[default]
    Vapour,
    /// From the liquid phase (dip tube or inverted cylinder) — much faster release
    Liquid,
}

/// Gas source (cylinder, rack of cylinders or tank).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceParameters {
    /// Container type
    pub bottle_type: BottleType,
    /// Water capacity of one container (L)
    pub volume_liters: f64,
    /// Number of manifolded containers (rack / bundle)
    pub count: u32,
    /// Initial pressure read on the gauge (bar, relative). Ignored for CO₂
    /// (the pressure is the saturation pressure).
    pub pressure_bar_g: f64,
    /// Container temperature (°C)
    pub temperature_c: f64,
    /// CO₂ filling ratio (kg of CO₂ per litre of water capacity)
    pub co2_fill_ratio: f64,
    /// CO₂ withdrawal phase
    pub co2_withdrawal: Co2Withdrawal,
}

impl Default for SourceParameters {
    fn default() -> Self {
        Self {
            bottle_type: BottleType::B50,
            volume_liters: 50.0,
            count: 1,
            pressure_bar_g: 200.0,
            temperature_c: 20.0,
            co2_fill_ratio: 0.75,
            co2_withdrawal: Co2Withdrawal::Vapour,
        }
    }
}

impl SourceParameters {
    /// Total internal volume (m³).
    #[must_use]
    pub fn total_volume_m3(&self) -> f64 {
        self.volume_liters * f64::from(self.count.max(1)) / 1000.0
    }
}

/// Type of release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReleaseKind {
    /// Leak through a hole / fitting of given diameter
    #[default]
    Leak,
    /// Valve fully open or torn off: release through the valve bore
    ValveRupture,
    /// Bounding case: whole inventory released instantaneously
    Instantaneous,
    /// Known constant gas flow for a given duration (process line, shielding
    /// gas, cryogenic boil-off…)
    ConstantFlow,
}

impl ReleaseKind {
    /// All kinds in display order.
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Leak, Self::ValveRupture, Self::Instantaneous, Self::ConstantFlow]
    }

    /// `true` if the release comes from the container through an orifice.
    #[must_use]
    pub fn uses_orifice(self) -> bool {
        matches!(self, Self::Leak | Self::ValveRupture)
    }

    /// Localised name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Leak => lang.pick("Fuite par orifice", "Leak through an orifice"),
            Self::ValveRupture => {
                lang.pick("Ouverture totale / arrachement du robinet", "Valve fully open / sheared off")
            }
            Self::Instantaneous => {
                lang.pick("Rejet instantané (cas enveloppe)", "Instantaneous release (bounding case)")
            }
            Self::ConstantFlow => lang.pick("Débit constant imposé", "Imposed constant flow"),
        }
    }

    /// Default equivalent diameter (mm) and discharge coefficient for the kind.
    #[must_use]
    pub fn defaults(self) -> (f64, f64) {
        match self {
            Self::Leak => (2.0, 0.62),
            Self::ValveRupture | Self::Instantaneous | Self::ConstantFlow => (10.0, 1.0),
        }
    }
}

/// Release scenario.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseParameters {
    /// Kind of release
    pub kind: ReleaseKind,
    /// Equivalent orifice diameter (mm)
    pub diameter_mm: f64,
    /// Discharge coefficient Cd (0.62 sharp-edged hole, 1.0 conservative)
    pub discharge_coefficient: f64,
    /// Constant-flow release: gas flow at room conditions (m³/h)
    pub constant_flow_m3h: f64,
    /// Constant-flow release: duration (min)
    pub constant_duration_min: f64,
    /// Cold gas from a cryogenic liquid: denser than air whatever the gas
    pub cold_gas: bool,
}

impl Default for ReleaseParameters {
    fn default() -> Self {
        Self {
            kind: ReleaseKind::Leak,
            diameter_mm: 2.0,
            discharge_coefficient: 0.62,
            constant_flow_m3h: 2.4,
            constant_duration_min: 60.0,
            cold_gas: false,
        }
    }
}

/// How the mechanical ventilation moves air.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VentilationMode {
    /// Exhaust fan: the extracted flow includes the released gas, so the fresh
    /// air drawn in is (Q_extract − Q_gas) — conservative, Fermilab FESHM 4240
    #[default]
    Extraction,
    /// Supply fan or balanced system: the fresh-air flow is the fan flow
    Supply,
}

impl VentilationMode {
    /// Localised name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Extraction => lang.pick("Extraction (local en dépression)", "Exhaust (room under depression)"),
            Self::Supply => lang.pick("Soufflage / double flux", "Supply / balanced"),
        }
    }
}

/// Container blowdown thermodynamics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Blowdown {
    /// Constant gas temperature (heat exchange with the walls): slow leaks, conservative flow
    #[default]
    Isothermal,
    /// Isentropic expansion of the gas left in the container: fast releases
    Adiabatic,
}

/// Room geometry and ventilation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomParameters {
    /// Free air volume (m³)
    pub volume_m3: f64,
    /// Ceiling height (m)
    pub height_m: f64,
    /// Room temperature (°C)
    pub temperature_c: f64,
    /// Atmospheric pressure (hPa)
    pub pressure_hpa: f64,
    /// Mechanical ventilation running during the scenario
    pub mechanical_enabled: bool,
    /// Mechanical extraction flow (m³/h)
    pub mechanical_flow_m3h: f64,
    /// Natural air change rate / infiltration (vol/h), always active
    pub natural_ach: f64,
    /// Ventilation effectiveness (mixing factor), 0 < η ≤ 1
    pub ventilation_efficiency: f64,
    /// Exhaust or supply mechanical ventilation
    pub mechanical_mode: VentilationMode,
}

impl Default for RoomParameters {
    fn default() -> Self {
        Self {
            volume_m3: 60.0,
            height_m: 3.0,
            temperature_c: 20.0,
            pressure_hpa: 1013.25,
            mechanical_enabled: true,
            mechanical_flow_m3h: 120.0,
            natural_ach: 0.2,
            ventilation_efficiency: 0.6,
            mechanical_mode: VentilationMode::Extraction,
        }
    }
}

impl RoomParameters {
    /// Effective air renewal η·Q_air (m³/s) without release, for display.
    #[must_use]
    pub fn effective_ventilation_m3s(&self) -> f64 {
        self.ventilation_efficiency * self.nominal_fresh_air_m3s(0.0)
    }

    /// Fresh air entering the room (m³/s) while gas enters at `q_gas` (m³/s).
    ///
    /// - Exhaust fan: the extracted flow already contains the released gas,
    ///   so only `Q_extract − Q_gas` of fresh air is drawn in; it combines
    ///   with natural infiltration in quadrature (ASHRAE Fundamentals, ch. 16).
    /// - Supply / balanced: fan flow plus infiltration.
    #[must_use]
    pub fn nominal_fresh_air_m3s(&self, q_gas: f64) -> f64 {
        let mech = if self.mechanical_enabled { self.mechanical_flow_m3h / 3600.0 } else { 0.0 };
        let infiltration = self.natural_ach * self.volume_m3 / 3600.0;
        match self.mechanical_mode {
            VentilationMode::Supply => mech + infiltration,
            VentilationMode::Extraction => (mech - q_gas).max(0.0).hypot(infiltration),
        }
    }

    /// Effective outflow removing the mixture (m³/s): the ventilation
    /// effectiveness η applies to the whole outgoing flow (gas included),
    /// which can never be less than the gas inflow itself.
    #[must_use]
    pub fn effective_outflow_m3s(&self, q_gas: f64) -> f64 {
        q_gas.max(self.ventilation_efficiency * (q_gas + self.nominal_fresh_air_m3s(q_gas)))
    }

    /// Total nominal air change rate (vol/h), before the efficiency factor.
    #[must_use]
    pub fn nominal_ach(&self) -> f64 {
        let mech = if self.mechanical_enabled { self.mechanical_flow_m3h } else { 0.0 };
        self.natural_ach + mech / self.volume_m3
    }

    /// Floor area (m²).
    #[must_use]
    pub fn floor_area_m2(&self) -> f64 {
        self.volume_m3 / self.height_m
    }
}

/// Use of the two-zone (stratified) model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StratificationMode {
    /// Enabled automatically for heavy and light gases
    #[default]
    Auto,
    /// Always enabled
    On,
    /// Disabled (well-mixed model only)
    Off,
}

/// Location of the ventilation extraction relative to the accumulation zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExtractionLocation {
    /// Distributed over the height (proportional to zone volumes)
    #[default]
    Distributed,
    /// Concentrated in the accumulation zone (low extraction for a heavy gas)
    AccumulationZone,
    /// Opposite to the accumulation zone (high extraction for a heavy gas)
    OppositeZone,
}

impl ExtractionLocation {
    /// Share of the ventilation flow sweeping the accumulation zone, given the
    /// volume fraction of that zone.
    #[must_use]
    pub fn share(self, volume_fraction: f64) -> f64 {
        match self {
            Self::Distributed => volume_fraction,
            Self::AccumulationZone => volume_fraction.max(0.8),
            Self::OppositeZone => (volume_fraction * 0.25).min(0.2),
        }
    }
}

/// Advanced model options.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelOptions {
    /// Two-zone model usage
    pub stratification: StratificationMode,
    /// Thickness of the accumulation layer (m): floor layer for heavy gas,
    /// ceiling layer for light gas
    pub zone_height_m: f64,
    /// Fraction of the released gas entering the accumulation zone directly
    pub injection_fraction: f64,
    /// Inter-zone exchange rate, in volumes of the accumulation zone per hour
    pub exchange_ach: f64,
    /// Extraction location
    pub extraction: ExtractionLocation,
    /// Maximum simulated duration (h)
    pub max_duration_h: f64,
    /// Container blowdown model
    pub blowdown: Blowdown,
    /// Near field (exposure next to the release): classical near-field /
    /// far-field model (Nicas 1996, AIHA IH Mod, ODHMOD). Not used for an
    /// instantaneous release.
    pub near_field: bool,
    /// Radius of the near-field hemisphere around the release point (m)
    pub near_field_radius_m: f64,
    /// Random air speed at the near-field boundary (m/s); the exchange flow
    /// is β = ½ · FSA · s with FSA the free surface of the hemisphere
    pub near_field_air_speed_m_s: f64,
    /// Also classify on the near field (the room classification otherwise
    /// relies on the room models, which is what detectors measure)
    pub near_field_in_assessment: bool,
}

impl Default for ModelOptions {
    fn default() -> Self {
        Self {
            stratification: StratificationMode::Auto,
            zone_height_m: 1.0,
            injection_fraction: 0.8,
            exchange_ach: 2.0,
            extraction: ExtractionLocation::Distributed,
            max_duration_h: 24.0,
            blowdown: Blowdown::Isothermal,
            near_field: true,
            near_field_radius_m: 1.0,
            near_field_air_speed_m_s: 0.06,
            near_field_in_assessment: false,
        }
    }
}

/// Near-field geometry derived from the options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearFieldGeometry {
    /// Near-field volume (m³), hemisphere capped at half the room
    pub volume_m3: f64,
    /// Free surface area of the hemisphere (m²)
    pub free_surface_m2: f64,
    /// Interzonal exchange flow β = ½ · FSA · s (m³/s)
    pub exchange_m3s: f64,
}

impl ModelOptions {
    /// Near-field geometry for a room of `room_volume_m3`.
    #[must_use]
    pub fn near_field_geometry(&self, room_volume_m3: f64) -> NearFieldGeometry {
        let r = self.near_field_radius_m;
        let hemisphere = 2.0 / 3.0 * std::f64::consts::PI * r.powi(3);
        let volume_m3 = hemisphere.min(0.5 * room_volume_m3);
        let free_surface_m2 = 2.0 * std::f64::consts::PI * r * r;
        NearFieldGeometry {
            volume_m3,
            free_surface_m2,
            exchange_m3s: 0.5 * free_surface_m2 * self.near_field_air_speed_m_s,
        }
    }

    /// Whether the two-zone model is used for a given gas.
    #[must_use]
    pub fn two_zone_active(&self, buoyancy: Buoyancy) -> bool {
        match self.stratification {
            StratificationMode::On => true,
            StratificationMode::Off => false,
            StratificationMode::Auto => buoyancy != Buoyancy::Neutral,
        }
    }
}

/// Predefined alarm threshold sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThresholdPreset {
    /// French practice: alarm 1 at 19 %, alarm 2 at 17 % (INRS, EN 50104 detectors)
    #[default]
    France,
    /// US practice: oxygen-deficient below 19.5 % (OSHA 29 CFR 1910.146); the
    /// 16 % second level is a usual choice, not a requirement of 1910.146
    Osha,
    /// User-defined values
    Custom,
}

impl ThresholdPreset {
    /// All presets.
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::France, Self::Osha, Self::Custom]
    }

    /// Localised name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::France => lang.pick("France – INRS (19 % / 17 %)", "France – INRS (19% / 17%)"),
            Self::Osha => lang.pick(
                "États-Unis – OSHA 19,5 % / second niveau usuel 16 %",
                "USA – OSHA 19.5% / usual second level 16%",
            ),
            Self::Custom => lang.pick("Personnalisé", "Custom"),
        }
    }

    /// Alarm levels (alarm 1, alarm 2) in % O₂ for the preset.
    #[must_use]
    pub fn levels(self) -> Option<(f64, f64)> {
        match self {
            Self::France => Some((19.0, 17.0)),
            Self::Osha => Some((19.5, 16.0)),
            Self::Custom => None,
        }
    }
}

/// O₂ alarm thresholds used for classification.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThresholdProfile {
    /// Preset
    pub preset: ThresholdPreset,
    /// Low alarm (alarm 1), % O₂
    pub alarm1: f64,
    /// Low-low alarm (alarm 2), % O₂
    pub alarm2: f64,
}

impl Default for ThresholdProfile {
    fn default() -> Self {
        Self { preset: ThresholdPreset::France, alarm1: 19.0, alarm2: 17.0 }
    }
}

impl ThresholdProfile {
    /// Effective (alarm1, alarm2) values.
    #[must_use]
    pub fn levels(&self) -> (f64, f64) {
        self.preset.levels().unwrap_or((self.alarm1, self.alarm2))
    }
}

impl Configuration {
    /// `true` when both configurations give the same simulation: the
    /// identification, the author's report choices and the planned change are
    /// not part of the physics.
    #[must_use]
    pub fn same_physics(&self, other: &Self) -> bool {
        let strip = |c: &Self| Self {
            identification: Identification::default(),
            report: ReportChoices::default(),
            future: FutureChange::default(),
            ..c.clone()
        };
        strip(self) == strip(other)
    }

    /// Events sorted by time, simultaneous events in a fixed order (stops,
    /// starts, isolation): the timeline applied by the solver.
    #[must_use]
    pub fn timeline(&self) -> Vec<Event> {
        let mut v: Vec<Event> = self.events.iter().map(|e| Event { time_s: e.time_s.max(0.0), ..*e }).collect();
        v.sort_by(|a, b| a.time_s.total_cmp(&b.time_s).then(a.action.rank().cmp(&b.action.rank())));
        v
    }

    /// Time at which the release is isolated, if an isolation event exists.
    #[must_use]
    pub fn isolation_time(&self) -> Option<f64> {
        self.timeline().iter().find(|e| e.action == EventAction::ReleaseIsolated).map(|e| e.time_s)
    }

    /// The same scenario without the ventilation events (a total failure from
    /// the start is studied with the mechanical ventilation off and no backup);
    /// the isolation of the leak, if any, is kept.
    #[must_use]
    pub fn without_ventilation_events(&self) -> Self {
        let mut c = self.clone();
        c.events.retain(|e| !e.action.concerns_ventilation());
        c
    }

    /// Vertical behaviour of the released gas: cold gas from a cryogenic
    /// liquid is denser than air at first, whatever the gas.
    #[must_use]
    pub fn buoyancy(&self) -> Buoyancy {
        if self.release.cold_gas && self.release.kind == ReleaseKind::ConstantFlow {
            Buoyancy::Heavy
        } else {
            self.gas.buoyancy()
        }
    }

    /// Validate every input.
    ///
    /// # Errors
    /// Returns the first invalid field.
    pub fn validate(&self) -> Result<()> {
        let s = &self.source;
        check_range("source.volume_liters", s.volume_liters, 0.1, 100_000.0, "L")?;
        if s.count == 0 || s.count > 100 {
            return Err(AnoxiaError::InvalidConfiguration("source.count must be in 1..=100".into()));
        }
        check_range("source.temperature_c", s.temperature_c, -30.0, 60.0, "°C")?;
        if self.gas.is_liquefied() {
            check_range("source.co2_fill_ratio", s.co2_fill_ratio, 0.05, 0.75, "kg/L")?;
        } else {
            check_range("source.pressure_bar_g", s.pressure_bar_g, 0.1, 700.0, "bar")?;
        }

        let r = &self.release;
        if r.kind == ReleaseKind::ConstantFlow {
            check_range("release.constant_flow_m3h", r.constant_flow_m3h, 1e-4, 1e6, "m³/h")?;
            check_range("release.constant_duration_min", r.constant_duration_min, 0.01, 10_080.0, "min")?;
        } else if r.kind.uses_orifice() {
            check_range("release.diameter_mm", r.diameter_mm, 0.05, 100.0, "mm")?;
            check_range("release.discharge_coefficient", r.discharge_coefficient, 0.3, 1.0, "")?;
        }

        let room = &self.room;
        check_range("room.volume_m3", room.volume_m3, 0.5, 1_000_000.0, "m³")?;
        check_range("room.height_m", room.height_m, 0.5, 100.0, "m")?;
        check_range("room.temperature_c", room.temperature_c, -30.0, 60.0, "°C")?;
        check_range("room.pressure_hpa", room.pressure_hpa, 500.0, 1300.0, "hPa")?;
        check_range("room.mechanical_flow_m3h", room.mechanical_flow_m3h, 0.0, 10_000_000.0, "m³/h")?;
        check_range("room.natural_ach", room.natural_ach, 0.0, 50.0, "vol/h")?;
        check_range("room.ventilation_efficiency", room.ventilation_efficiency, 0.05, 1.0, "")?;

        let m = &self.model;
        check_range("model.zone_height_m", m.zone_height_m, 0.1, room.height_m * 0.9, "m").or_else(|e| {
            if m.two_zone_active(self.buoyancy()) {
                Err(e)
            } else {
                Ok(())
            }
        })?;
        check_range("model.injection_fraction", m.injection_fraction, 0.0, 1.0, "")?;
        check_range("model.exchange_ach", m.exchange_ach, 0.0, 1000.0, "vol/h")?;
        check_range("model.near_field_radius_m", m.near_field_radius_m, 0.3, 5.0, "m")?;
        check_range("model.near_field_air_speed_m_s", m.near_field_air_speed_m_s, 0.01, 1.0, "m/s")?;
        check_range("model.max_duration_h", m.max_duration_h, 0.1, 168.0, "h")?;

        let (a1, a2) = self.thresholds.levels();
        check_range("thresholds.alarm1", a1, 10.0, 20.8, "%")?;
        check_range("thresholds.alarm2", a2, 6.0, a1, "%")?;

        if self.events.len() > 20 {
            return Err(AnoxiaError::InvalidConfiguration("events: at most 20 events".into()));
        }
        for e in &self.events {
            check_range("events.time_s", e.time_s, 0.0, 168.0 * 3600.0, "s")?;
            if let EventAction::VentilationStarts { flow_m3h, .. } = e.action {
                check_range("events.flow_m3h", flow_m3h, 0.0, 10_000_000.0, "m³/h")?;
            }
        }
        Ok(())
    }

    /// Save to a pretty-printed JSON file.
    ///
    /// # Errors
    /// I/O or serialisation failure.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        fs::write(path, self.to_json()?)?;
        Ok(())
    }

    /// Load and validate a JSON file (current or legacy format).
    ///
    /// # Errors
    /// I/O failure, malformed JSON or invalid values.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_json(&fs::read_to_string(path)?)
    }

    /// Serialise to JSON.
    ///
    /// # Errors
    /// Serialisation failure.
    pub fn to_json(&self) -> Result<String> {
        let normalised = Self { schema_version: SCHEMA_VERSION, ..self.clone() };
        Ok(serde_json::to_string_pretty(&normalised)?)
    }

    /// Parse JSON (current or legacy v1 format) and validate it.
    ///
    /// # Errors
    /// Malformed JSON or invalid values.
    pub fn from_json(json: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        if let Some(v) = value.get("schema_version").and_then(serde_json::Value::as_u64) {
            if v > u64::from(SCHEMA_VERSION) {
                return Err(AnoxiaError::Serialization(format!(
                    "file written by a newer version of the software (format {v}, supported up to {SCHEMA_VERSION})"
                )));
            }
        }
        let config =
            if value.get("schema_version").is_some() { serde_json::from_value(value)? } else { migrate_v1(&value)? };
        config.validate()?;
        Ok(config)
    }
}

/// Convert a configuration file written by version 1.x.
fn migrate_v1(v: &serde_json::Value) -> Result<Configuration> {
    let mut c = Configuration::default();
    let num = |p: &str| v.pointer(p).and_then(serde_json::Value::as_f64);
    let text = |p: &str| v.pointer(p).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();

    c.identification.unit = text("/identification/unit");
    c.identification.building = text("/identification/building");
    c.identification.installation = text("/identification/installation");
    c.identification.user = text("/identification/user");
    c.identification.comments = text("/identification/comments");

    if let Some(g) = v.pointer("/gas/gas_type") {
        c.gas = serde_json::from_value(g.clone())?;
    } else {
        return Err(AnoxiaError::Serialization("missing gas type".into()));
    }

    if let Some(b) = v.pointer("/bottle/bottle_type") {
        c.source.bottle_type = serde_json::from_value(b.clone()).unwrap_or(BottleType::Custom);
    }
    if let Some(vol) = num("/bottle/volume_liters") {
        c.source.volume_liters = vol;
    }
    if let Some(p) = num("/bottle/pressure_bar") {
        c.source.pressure_bar_g = p;
    }

    // v1 stored the diameter in hundredths of a millimetre.
    if let Some(d) = num("/scenario/Leak/diameter_mm") {
        c.release.kind = ReleaseKind::Leak;
        c.release.diameter_mm = d / 100.0;
        c.release.discharge_coefficient = 0.62;
    } else if v.pointer("/scenario").and_then(serde_json::Value::as_str) == Some("FullDischarge") {
        c.release.kind = ReleaseKind::ValveRupture;
        let (d, cd) = ReleaseKind::ValveRupture.defaults();
        c.release.diameter_mm = d;
        c.release.discharge_coefficient = cd;
    }

    if let Some(vol) = num("/room/volume_m3") {
        c.room.volume_m3 = vol;
    }
    if let Some(h) = num("/room/height_m") {
        c.room.height_m = h;
    }
    let mechanical = v.pointer("/room/ventilation_type").and_then(serde_json::Value::as_str) == Some("Mechanical");
    c.room.mechanical_enabled = mechanical;
    if let Some(q) = num("/room/ventilation_flow_m3h") {
        c.room.mechanical_flow_m3h = q;
    }
    c.room.natural_ach = if mechanical { 0.0 } else { num("/room/renewal_rate").unwrap_or(0.2) };
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_choices_round_trip_and_default() {
        let mut c = Configuration::default();
        assert!(c.report.is_retained("o2_detection"));
        c.report.set_retained("o2_detection", false);
        c.report.set_note("o2_detection", "Détection existante, vérifiée le 12/03.");
        c.report.set_note("training", "   ");
        c.report.additional = "Consigne affichée à l'entrée.".into();
        let json = c.to_json().unwrap();
        let back = Configuration::from_json(&json).unwrap();
        assert!(!back.report.is_retained("o2_detection"));
        assert!(back.report.is_retained("training"));
        assert_eq!(back.report.note("o2_detection"), "Détection existante, vérifiée le 12/03.");
        assert!(!back.report.notes.contains_key("training"));
        assert_eq!(back.report.additional, "Consigne affichée à l'entrée.");
        // Files written before this field existed still load (serde default).
        let json = json.replace("\"report\"", "\"report_old\"");
        let old = Configuration::from_json(&json).unwrap();
        assert!(old.report.is_retained("o2_detection"));
        // Toggling back and forth leaves no duplicates.
        c.report.set_retained("o2_detection", false);
        c.report.set_retained("o2_detection", true);
        assert!(c.report.excluded.is_empty());
    }

    #[test]
    fn default_is_valid() {
        Configuration::default().validate().unwrap();
    }

    #[test]
    fn json_roundtrip() {
        let mut c = Configuration { gas: GasType::Helium, ..Configuration::default() };
        c.identification.comments = "Local <technique> \"B\"".into();
        c.release.diameter_mm = 3.5;
        let back = Configuration::from_json(&c.to_json().unwrap()).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn newer_schema_is_refused_and_saved_version_is_normalised() {
        assert!(Configuration::from_json(r#"{"schema_version":99,"gas":"Nitrogen"}"#).is_err());
        let c = Configuration { schema_version: 1, ..Configuration::default() };
        assert!(c.to_json().unwrap().contains(&format!("\"schema_version\": {SCHEMA_VERSION}")));
    }

    #[test]
    fn missing_fields_take_defaults() {
        let c = Configuration::from_json(r#"{"schema_version":2,"gas":"Nitrogen"}"#).unwrap();
        assert_eq!(c.gas, GasType::Nitrogen);
        assert_eq!(c.room, RoomParameters::default());
    }

    #[test]
    fn legacy_v1_file_is_migrated() {
        let legacy = r#"{
            "identification":{"unit":"Labo","building":"B1","installation":"Salle 3","user":"NB","comments":""},
            "scenario":{"Leak":{"diameter_mm":800}},
            "gas":{"gas_type":"Argon","properties":{"molar_mass":39.948,"gamma":1.667,"density":1.661}},
            "bottle":{"bottle_type":"B50","volume_liters":50.0,"pressure_bar":200.0},
            "room":{"volume_m3":150.0,"renewal_rate":1.33,"ventilation_type":"Mechanical","ventilation_flow_m3h":200.0,"height_m":3.0}
        }"#;
        let c = Configuration::from_json(legacy).unwrap();
        assert_eq!(c.gas, GasType::Argon);
        assert!((c.release.diameter_mm - 8.0).abs() < 1e-9);
        assert!((c.room.volume_m3 - 150.0).abs() < 1e-9);
        assert!(c.room.mechanical_enabled);
        assert_eq!(c.identification.unit, "Labo");
    }

    #[test]
    fn legacy_full_discharge() {
        let legacy = r#"{"scenario":"FullDischarge","gas":{"gas_type":"Nitrogen"},
            "bottle":{"bottle_type":"B20","volume_liters":20.0,"pressure_bar":150.0},
            "room":{"volume_m3":30.0,"renewal_rate":0.5,"ventilation_type":"Natural","ventilation_flow_m3h":0.0}}"#;
        let c = Configuration::from_json(legacy).unwrap();
        assert_eq!(c.release.kind, ReleaseKind::ValveRupture);
        assert!(!c.room.mechanical_enabled);
        assert!((c.room.natural_ach - 0.5).abs() < 1e-9);
    }

    #[test]
    fn invalid_values_rejected() {
        let mut c = Configuration::default();
        c.room.volume_m3 = f64::NAN;
        assert!(c.validate().is_err());
        let mut c = Configuration::default();
        c.release.diameter_mm = 0.0;
        assert!(c.validate().is_err());
        let c = Configuration {
            thresholds: ThresholdProfile { preset: ThresholdPreset::Custom, alarm1: 17.0, alarm2: 18.0 },
            ..Configuration::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn effective_ventilation() {
        let r = RoomParameters {
            volume_m3: 100.0,
            mechanical_enabled: true,
            mechanical_flow_m3h: 300.0,
            natural_ach: 0.5,
            ventilation_efficiency: 0.5,
            ..RoomParameters::default()
        };
        // Exhaust: fan flow and infiltration combine in quadrature (ASHRAE)
        assert!((r.effective_ventilation_m3s() - 0.5 * 300.0_f64.hypot(50.0) / 3600.0).abs() < 1e-12);
        let s = RoomParameters { mechanical_mode: VentilationMode::Supply, ..r };
        assert!((s.effective_ventilation_m3s() - 0.5 * 350.0 / 3600.0).abs() < 1e-12);
        // η applies to the whole outflow: steady state x = Qgas / (η·Q_extract)
        let q = 100.0 / 3600.0;
        let no_inf = RoomParameters { natural_ach: 0.0, ..r };
        assert!((no_inf.effective_outflow_m3s(q) - 0.5 * 300.0 / 3600.0).abs() < 1e-12);
        assert!((r.nominal_ach() - 3.5).abs() < 1e-12);
    }

    #[test]
    fn two_zone_auto() {
        let m = ModelOptions::default();
        assert!(m.two_zone_active(GasType::Argon.buoyancy()));
        assert!(m.two_zone_active(GasType::Helium.buoyancy()));
        assert!(!m.two_zone_active(GasType::Nitrogen.buoyancy()));
        let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        c.release.kind = ReleaseKind::ConstantFlow;
        c.release.cold_gas = true;
        assert_eq!(c.buoyancy(), Buoyancy::Heavy);
    }
}
