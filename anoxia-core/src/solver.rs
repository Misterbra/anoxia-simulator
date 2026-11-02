//! Time integration of the oxygen balance in the room.
//!
//! ## Well-mixed model
//!
//! With `x` the volume fraction of released gas, `Q_in` the released gas flow
//! expanded to room conditions and `Q_v` the effective ventilation flow
//! (fresh air in = mixture out), the species balance at constant room
//! pressure is
//!
//! ```text
//! V·dx/dt = Q_in·(1 − x) − Q_v·x          O₂ = 20.95 %·(1 − x)
//! ```
//!
//! The released gas displaces an equal volume of mixture, so the room
//! pressure stays atmospheric (leaky room assumption).
//!
//! ## Two-zone model (indicative)
//!
//! For heavy or light gases, the room is split into an accumulation zone
//! (floor layer for heavy gases, ceiling layer for light gases) and the rest
//! of the room, coupled by a symmetric exchange flow `Q_x`:
//!
//! ```text
//! V_a·dx_a/dt = Q_in,a(1 − x_a) − Q_v,a·x_a + Q_x(x_b − x_a)
//! V_b·dx_b/dt = Q_in,b(1 − x_b) − Q_v,b·x_b + Q_x(x_a − x_b)
//! ```
//!
//! ## Near field (exposure next to the release)
//!
//! Classical near-field / far-field model of industrial hygiene (Nicas 1996;
//! AIHA IH Mod; Stefana et al. 2021, ODHMOD): a hemisphere of radius `r`
//! around the release point exchanges air with the rest of the room at
//! `β = ½·FSA·s` (free surface `FSA = 2πr²`, random air speed `s`). The gas
//! enters the near field, the excess volume flows to the far field, which is
//! ventilated and vents to the outside:
//!
//! ```text
//! V_nf·dx_nf/dt = Q_in·(1 − x_nf) + β·(x_ff − x_nf)
//! V_ff·dx_ff/dt = (Q_in + β)·x_nf − β·x_ff − Q_out·x_ff
//! ```
//!
//! Summing both equations gives the room balance with the far-field
//! concentration at the outlet; it coincides with the well-mixed balance when
//! the expelled mixture is representative of the average (validation case O4
//! documents the difference). The model assumes a low-momentum release and
//! gives a zone average: for a pressurised jet the air entrainment is not
//! resolved, the real hemisphere average is probably lower, but the
//! concentration in the jet axis can be much higher. It is not a bound at
//! every point.
//!
//! ## Numerics
//!
//! Over each step the room equations are linear with constant coefficients
//! and are integrated **exactly** (matrix exponential), so the scheme is
//! unconditionally stable. The container inventory is integrated with RK4
//! and the gas injected in the room during a step is exactly the mass lost
//! by the container, so mass is conserved to round-off.

use crate::config::{Configuration, Event, EventAction, ReleaseKind, RoomParameters};
use crate::constants::{celsius_to_kelvin, O2_AMBIENT_PERCENT, R};
use crate::error::Result;
use crate::flow::{FlowRegime, Source};
use crate::gas::Buoyancy;
use serde::{Deserialize, Serialize};

/// Physiological / regulatory O₂ levels (% vol) tracked in every simulation,
/// in addition to the configured alarm thresholds.
pub const O2_REFERENCE_LEVELS: [f64; 9] = [19.5, 19.0, 18.0, 17.0, 16.0, 14.0, 12.0, 10.0, 6.0];

/// CO₂ levels (% vol) tracked when the released gas is CO₂:
/// 0.5 % VLEP-8h (FR, Code du travail R.4412-149) / OSHA PEL,
/// 3 % STEL (ACGIH), 4 % IDLH (NIOSH), 10 % rapid loss of consciousness.
pub const CO2_REFERENCE_LEVELS: [f64; 4] = [0.5, 3.0, 4.0, 10.0];

/// Background CO₂ in outdoor air (% vol).
const CO2_BACKGROUND_PERCENT: f64 = 0.04;

/// Crossing of an O₂ (downward) or CO₂ (upward) threshold.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Crossing {
    /// Threshold value (% vol)
    pub level: f64,
    /// First time the threshold is exceeded (s)
    pub first: Option<f64>,
    /// Time of the final return to the safe side of the threshold (s)
    pub back: Option<f64>,
    /// Cumulated time beyond the threshold (s)
    pub total: f64,
    /// Still beyond the threshold when the simulation ended: `total` is a
    /// lower bound and `back` is unknown (not "never")
    #[serde(default)]
    pub censored: bool,
}

impl Crossing {
    fn new(level: f64) -> Self {
        Self { level, first: None, back: None, total: 0.0, censored: false }
    }

    /// `true` if the threshold was exceeded at some point.
    #[must_use]
    pub fn reached(&self) -> bool {
        self.first.is_some()
    }

    /// Time beyond the threshold written for a sentence: "—" when never
    /// reached, "au moins 6 min" / "at least 6 min" when still exceeded at the
    /// end of the computation, the duration otherwise.
    #[must_use]
    pub fn total_text(&self, lang: crate::i18n::Language) -> String {
        if !self.reached() {
            return "—".to_string();
        }
        let d = crate::i18n::fmt_duration(self.total, lang);
        if self.censored {
            format!("{} {d}", lang.pick("au moins", "at least"))
        } else {
            d
        }
    }

    /// Same for a table cell: "≥ 6 min" when censored.
    #[must_use]
    pub fn total_cell(&self, lang: crate::i18n::Language) -> String {
        if !self.reached() {
            return "—".to_string();
        }
        let d = crate::i18n::fmt_duration(self.total, lang);
        if self.censored {
            format!("≥ {d}")
        } else {
            d
        }
    }
}

/// Scalar results of a simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// Initial inventory (kg)
    pub initial_mass: f64,
    /// Releasable inventory (kg)
    pub releasable_mass: f64,
    /// Mass actually released over the simulated period (kg)
    pub released_mass: f64,
    /// Releasable inventory expanded to room conditions (m³)
    pub releasable_volume_m3: f64,
    /// Initial absolute container pressure (bar)
    pub initial_pressure_bar_a: f64,
    /// Initial mass flow (kg/s); infinite for an instantaneous release
    #[serde(with = "f64_ext")]
    pub initial_mass_flow: f64,
    /// Initial gas flow at room conditions (m³/h); infinite for an instantaneous release
    #[serde(with = "f64_ext")]
    pub initial_volume_flow_m3h: f64,
    /// Initial flow regime
    pub initial_regime: FlowRegime,
    /// Time for 90 % of the releasable inventory to escape (s)
    pub time_90_percent: Option<f64>,
    /// Time for 99 % of the releasable inventory to escape (≈ release duration, s)
    pub release_duration: Option<f64>,
    /// Effective ventilation flow η·Q (m³/h)
    pub effective_ventilation_m3h: f64,
    /// Minimum O₂ in the assessment series (% vol)
    pub min_o2: f64,
    /// Time of the minimum (s)
    pub time_at_min: f64,
    /// Minimum O₂ of the well-mixed model (% vol)
    pub min_o2_mixed: f64,
    /// Minimum O₂ of the accumulation zone, two-zone model (% vol)
    pub min_o2_zone: Option<f64>,
    /// Maximum fraction of released gas in the assessment series
    pub max_gas_fraction: f64,
    /// Whole inventory released instantly, no ventilation, displacement
    /// method of INRS ED 155 / BCGA CP 52: O₂ = 20.95·(1 − Vg/V) (% O₂, conservative)
    pub instant_displacement_o2: f64,
    /// Same with the free-efflux (well-mixed) relation of NFPA 2001 / ISO 14520:
    /// O₂ = 20.95·exp(−Vg/V) (% O₂, best estimate)
    pub instant_free_efflux_o2: f64,
    /// Hand calculation of INRS ED 155 / BCGA CP 52 for cross-checking:
    /// Vg = P(bar)·V(L)/1000 (compressed gas) or 0.5344 m³/kg (CO₂),
    /// O₂ = 21·(1 − Vg/V) (% O₂)
    pub simplified_method_o2: f64,
    /// Gas volume used by the simplified method (m³)
    pub simplified_method_volume_m3: f64,
    /// Two-zone model used
    pub two_zone: bool,
    /// Buoyancy of the released gas
    pub buoyancy: Buoyancy,
    /// Simulation stopped at the maximum duration before the end of the event
    pub truncated: bool,
    /// CO₂ container liquid-full at the given temperature
    pub liquid_full: bool,
    /// Simulated duration (s)
    pub simulated_duration: f64,
    /// O₂ threshold crossings (assessment series), sorted by decreasing level
    pub o2_crossings: Vec<Crossing>,
    /// CO₂ threshold crossings (only for CO₂), sorted by increasing level
    pub co2_crossings: Vec<Crossing>,
    /// Peak CO₂ concentration (% vol, only for CO₂)
    pub max_co2_percent: Option<f64>,
    /// Minimum O₂ in the near field (% vol), when computed
    pub min_o2_near: Option<f64>,
    /// Time of the near-field minimum (s)
    pub time_at_min_near: Option<f64>,
    /// Near-field volume (m³), when computed
    pub near_field_volume_m3: Option<f64>,
    /// Near-field exchange flow β (m³/h), when computed
    pub near_field_exchange_m3h: Option<f64>,
    /// O₂ threshold crossings in the near field (empty when not computed)
    pub near_crossings: Vec<Crossing>,
    /// Series that sets the minimum O₂ of the assessment
    #[serde(default)]
    pub governing: Governing,
}

impl Summary {
    /// Crossing record for an O₂ level, if tracked.
    #[must_use]
    pub fn o2_crossing(&self, level: f64) -> Option<&Crossing> {
        self.o2_crossings.iter().find(|c| (c.level - level).abs() < 1e-9)
    }

    /// Near-field crossing record for an O₂ level, if computed.
    #[must_use]
    pub fn near_crossing(&self, level: f64) -> Option<&Crossing> {
        self.near_crossings.iter().find(|c| (c.level - level).abs() < 1e-9)
    }

    /// Crossing record for a CO₂ level, if tracked.
    #[must_use]
    pub fn co2_crossing(&self, level: f64) -> Option<&Crossing> {
        self.co2_crossings.iter().find(|c| (c.level - level).abs() < 1e-9)
    }

    /// Time window (s) focused on the event: release and minimum, plus the
    /// return above 19.5 % only when it happens within a comparable time (a
    /// slow recovery over hours would otherwise squeeze the dip to the left).
    #[must_use]
    pub fn event_window(&self) -> f64 {
        let core = (self.time_at_min * 2.5).max(self.release_duration.unwrap_or(0.0) * 1.2).max(60.0);
        let back = self.o2_crossing(19.5).and_then(|c| c.back).unwrap_or(0.0);
        let end = if back > 0.0 && back <= 4.0 * core { core.max(back * 1.1) } else { core };
        end.min(self.simulated_duration)
    }
}

/// Time series and summary of a simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Time (s)
    pub time: Vec<f64>,
    /// O₂, well-mixed model (% vol)
    pub o2_mixed: Vec<f64>,
    /// O₂ in the accumulation zone, two-zone model (% vol)
    pub o2_zone: Option<Vec<f64>>,
    /// O₂ in the rest of the room, two-zone model (% vol)
    pub o2_other: Option<Vec<f64>>,
    /// O₂ series used for the assessment: worst of the models (% vol)
    pub o2: Vec<f64>,
    /// O₂ in the near field around the release point (% vol), when computed
    pub o2_near: Option<Vec<f64>>,
    /// CO₂ in the assessment series (% vol), only for CO₂
    pub co2: Option<Vec<f64>>,
    /// Mass flow (kg/s)
    pub mass_flow: Vec<f64>,
    /// Container pressure (bar, gauge)
    pub source_pressure_bar_g: Vec<f64>,
    /// Cumulated released mass (kg)
    pub released_mass: Vec<f64>,
    /// Scalar results
    pub summary: Summary,
}

/// Linear system `dx/dt = A·x + b` with constant coefficients (1 or 2 zones).
#[derive(Debug, Clone, Copy)]
struct Linear2 {
    a: [[f64; 2]; 2],
    b: [f64; 2],
}

impl Linear2 {
    /// Exact solution after `dt`: x(dt) = E·x0 + P·b with E = exp(A·dt) and
    /// P = ∫₀^dt exp(A·u) du. Both come from the spectral formula of the 2 × 2
    /// matrix (real eigenvalues λ₁ ≥ λ₂ since a₁₂·a₂₁ ≥ 0):
    /// M(f) = [f(λ₁)(A − λ₂I) − f(λ₂)(A − λ₁I)] / (λ₁ − λ₂), with f = e^{λdt}
    /// for E and f = (e^{λdt} − 1)/λ for P. P is defined for a singular A
    /// (a zone without outflow keeps receiving its source), which A⁻¹(E − I)
    /// is not.
    fn step(&self, x: [f64; 2], dt: f64) -> [f64; 2] {
        let [[a11, a12], [a21, a22]] = self.a;
        let s = 0.5 * (a11 + a22);
        let det = a11 * a22 - a12 * a21;
        let disc = (s * s - det).max(0.0);
        let q = disc.sqrt();
        // φ(λ) = (e^{λ dt} − 1)/λ, continuous at λ = 0 (= dt)
        let phi = |l: f64| if (l * dt).abs() < 1e-12 { dt } else { (l * dt).exp_m1() / l };
        let (e, p) = if q * dt < 1e-8 {
            // Double eigenvalue (or nearly): E = e^{s dt}(I + dt·N), N = A − sI,
            // P = φ₀ I + φ₁ N with φ₀ = ∫e^{su}du and φ₁ = ∫u·e^{su}du.
            let es = (s * dt).exp();
            let phi0 = phi(s);
            let phi1 = if (s * dt).abs() < 1e-4 {
                0.5 * dt * dt * (1.0 + 2.0 / 3.0 * s * dt + 0.25 * (s * dt).powi(2))
            } else {
                dt * es / s - (es - 1.0) / (s * s)
            };
            let n = [[a11 - s, a12], [a21, a22 - s]];
            (
                [[es * (1.0 + dt * n[0][0]), es * dt * n[0][1]], [es * dt * n[1][0], es * (1.0 + dt * n[1][1])]],
                [[phi0 + phi1 * n[0][0], phi1 * n[0][1]], [phi1 * n[1][0], phi0 + phi1 * n[1][1]]],
            )
        } else {
            let (l1, l2) = (s + q, s - q);
            let k = 1.0 / (l1 - l2);
            let spectral = |f1: f64, f2: f64| {
                [
                    [k * (f1 * (a11 - l2) - f2 * (a11 - l1)), k * (f1 - f2) * a12],
                    [k * (f1 - f2) * a21, k * (f1 * (a22 - l2) - f2 * (a22 - l1))],
                ]
            };
            (spectral((l1 * dt).exp(), (l2 * dt).exp()), spectral(phi(l1), phi(l2)))
        };
        let out = [
            e[0][0] * x[0] + e[0][1] * x[1] + p[0][0] * self.b[0] + p[0][1] * self.b[1],
            e[1][0] * x[0] + e[1][1] * x[1] + p[1][0] * self.b[0] + p[1][1] * self.b[1],
        ];
        [out[0].clamp(0.0, 1.0), out[1].clamp(0.0, 1.0)]
    }
}

/// Exact single-zone update.
fn mixed_step(x: f64, q_in: f64, q_out: f64, volume: f64, dt: f64) -> f64 {
    let a = q_in / volume;
    let b = q_out / volume;
    if b <= 0.0 {
        return x;
    }
    let xss = a / b;
    (xss + (x - xss) * (-b * dt).exp()).clamp(0.0, 1.0)
}

/// Threshold tracker for a scalar series.
struct Tracker {
    crossings: Vec<Crossing>,
    /// `true` for O₂ (danger when below), `false` for CO₂ (danger when above)
    downward: bool,
}

impl Tracker {
    fn new(levels: &[f64], downward: bool) -> Self {
        Self { crossings: levels.iter().map(|&l| Crossing::new(l)).collect(), downward }
    }

    /// Register the initial value (an instantaneous release can start beyond a threshold).
    fn init(&mut self, v0: f64) {
        for c in &mut self.crossings {
            let bad = if self.downward { v0 < c.level } else { v0 > c.level };
            if bad {
                c.first = Some(0.0);
            }
        }
    }

    /// Register the step `t0 → t1`; `at(s)` gives the exact value at `t0 + s`
    /// (the room equations are solved exactly within a step), so crossing
    /// times are located by bisection instead of linear interpolation.
    fn update(&mut self, t0: f64, v0: f64, t1: f64, v1: f64, at: &dyn Fn(f64) -> f64) {
        let dt = t1 - t0;
        for c in &mut self.crossings {
            let (bad0, bad1) = if self.downward { (v0 < c.level, v1 < c.level) } else { (v0 > c.level, v1 > c.level) };
            if bad0 == bad1 {
                if bad0 {
                    c.total += dt;
                }
                continue;
            }
            // Bisection on s ∈ [0, dt]: the sign of (value − level) changes.
            let (mut lo, mut hi) = (0.0_f64, dt);
            let below0 = v0 < c.level;
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if (at(mid) < c.level) == below0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
                if hi - lo <= 1e-9 * dt.max(1e-9) {
                    break;
                }
            }
            let tc = t0 + 0.5 * (lo + hi);
            match (bad0, bad1) {
                (false, true) => {
                    if c.first.is_none() {
                        c.first = Some(tc);
                    }
                    c.back = None;
                    c.total += t1 - tc;
                }
                (true, false) => {
                    c.back = Some(tc);
                    c.total += tc - t0;
                }
                _ => {}
            }
        }
    }

    /// Mark the thresholds still exceeded when the simulation ends.
    fn finish(&mut self) {
        for c in &mut self.crossings {
            c.censored = c.first.is_some() && c.back.is_none();
        }
    }
}

/// Which series sets the minimum O₂ of the assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Governing {
    /// Well-mixed room average
    #[default]
    Mixed,
    /// Accumulation layer of the two-zone model
    AccumulationZone,
    /// Rest of the room in the two-zone model
    OtherZone,
    /// Near field (only when included in the assessment)
    NearField,
}

impl Governing {
    /// Localised label.
    #[must_use]
    pub fn label(self, lang: crate::i18n::Language) -> &'static str {
        match self {
            Self::Mixed => lang.pick("moyenne du local (bien mélangé)", "room average (well-mixed)"),
            Self::AccumulationZone => lang.pick("couche d'accumulation (bi-zone)", "accumulation layer (two-zone)"),
            Self::OtherZone => lang.pick("reste du local (bi-zone)", "rest of the room (two-zone)"),
            Self::NearField => lang.pick("champ proche", "near field"),
        }
    }
}

/// Non-finite values (instantaneous release: undefined flow) are written as
/// strings so that results survive a JSON round trip; `null` from older
/// files is read back as infinity.
pub(crate) mod f64_ext {
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            s.serialize_f64(*v)
        } else if v.is_nan() {
            s.serialize_str("NaN")
        } else if *v > 0.0 {
            s.serialize_str("Infinity")
        } else {
            s.serialize_str("-Infinity")
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(f64),
            Text(String),
            Null,
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Num(v) => v,
            Raw::Text(t) => match t.as_str() {
                "NaN" => f64::NAN,
                "-Infinity" => f64::NEG_INFINITY,
                _ => f64::INFINITY,
            },
            Raw::Null => f64::INFINITY,
        })
    }
}

/// Run the simulation for a validated configuration.
///
/// # Errors
/// Invalid configuration or impossible source state.
#[allow(clippy::too_many_lines)]
pub fn simulate(config: &Configuration) -> Result<SimulationResult> {
    config.validate()?;
    let gas = config.gas;
    let room = &config.room;
    let kind = config.release.kind;
    let constant = kind == ReleaseKind::ConstantFlow;
    // An imposed flow has no container: its parameters are not validated nor used.
    let source = if constant { None } else { Some(Source::from_config(config)?) };
    let src = || source.as_ref().expect("container source");
    let t_room = celsius_to_kelvin(room.temperature_c);
    let volume = room.volume_m3;
    let p_amb = room.pressure_hpa * 100.0;
    // Mass → volume at room conditions (ideal gas at low pressure)
    let to_volume = |mass: f64| mass * R * t_room / (gas.properties().molar_mass * p_amb);
    let eta = room.ventilation_efficiency;
    // Timeline: the mechanical ventilation can stop or (re)start and the
    // release can be isolated at given times. `room_now` holds the current
    // ventilation state; the step always lands on an event time, so the
    // coefficients stay constant inside a step and the state is continuous.
    let horizon = config.model.max_duration_h * 3600.0;
    // Events after the horizon cannot change the result and are not waited for.
    let timeline: Vec<Event> = config.timeline().into_iter().filter(|e| e.time_s <= horizon).collect();
    let t_iso = timeline.iter().find(|e| e.action == EventAction::ReleaseIsolated).map(|e| e.time_s);
    let mut room_now: RoomParameters = *room;
    let mut next_event = 0usize;
    let apply = |room_now: &mut RoomParameters, e: &Event| match e.action {
        EventAction::VentilationStops => room_now.mechanical_enabled = false,
        EventAction::VentilationStarts { flow_m3h, mode } => {
            room_now.mechanical_enabled = flow_m3h > 0.0;
            room_now.mechanical_flow_m3h = flow_m3h;
            room_now.mechanical_mode = mode;
        }
        EventAction::ReleaseIsolated => {}
    };
    while next_event < timeline.len() && timeline[next_event].time_s <= 1e-9 {
        apply(&mut room_now, &timeline[next_event]);
        next_event += 1;
    }
    // Ventilation credited at the start of the release (after the t = 0 events)
    let q_v = room_now.effective_ventilation_m3s();
    // Constant-flow release: mass flow from the imposed gas flow at room conditions.
    let mdot_const = if constant { config.release.constant_flow_m3h / 3600.0 / to_volume(1.0) } else { 0.0 };
    let t_const = config.release.constant_duration_min * 60.0;

    // Two-zone geometry
    let buoyancy = config.buoyancy();
    let two_zone = config.model.two_zone_active(buoyancy);
    let zone_h = config.model.zone_height_m.clamp(0.05 * room.height_m, 0.9 * room.height_m);
    let v_a = volume * zone_h / room.height_m;
    let v_b = volume - v_a;
    let f_in = config.model.injection_fraction;
    let share = config.model.extraction.share(v_a / volume);
    let q_x = config.model.exchange_ach * v_a / 3600.0;

    // Near field (not meaningful for an instantaneous release)
    let near = config.model.near_field && kind != ReleaseKind::Instantaneous;
    let near_in_assessment = near && config.model.near_field_in_assessment;
    let nf = config.model.near_field_geometry(volume);
    let (v_nf, v_ff, beta) = (nf.volume_m3, volume - nf.volume_m3, nf.exchange_m3s);

    let (alarm1, alarm2) = config.thresholds.levels();
    let mut o2_levels: Vec<f64> = O2_REFERENCE_LEVELS.to_vec();
    for l in [alarm1, alarm2] {
        if !o2_levels.iter().any(|&x| (x - l).abs() < 1e-9) {
            o2_levels.push(l);
        }
    }
    o2_levels.sort_by(|a, b| b.total_cmp(a));
    let mut o2_tracker = Tracker::new(&o2_levels, true);
    let mut near_tracker = Tracker::new(&o2_levels, true);
    let mut co2_tracker = Tracker::new(&CO2_REFERENCE_LEVELS, false);
    let is_co2 = gas.is_toxic();

    // `m` is the mass still to be released (container inventory, or remaining
    // constant-flow quantity); the release stops at `m_end`.
    let (initial_mass, releasable, m_end) = if constant {
        (mdot_const * t_const, mdot_const * t_const, 0.0)
    } else {
        (src().initial_mass, src().releasable_mass(), src().end_mass)
    };
    // Mass that can actually leave in this model (adiabatic: cold gas left behind).
    let leavable = (initial_mass - m_end).max(0.0);
    let mut m = initial_mass;
    let mut x_mixed = 0.0;
    let mut x_zone = [0.0, 0.0];
    // Near field: [x_nf, x_ff] of the near-field / far-field model
    let mut x_near = [0.0, 0.0];

    if kind == ReleaseKind::Instantaneous {
        // Conservative displacement method (INRS ED 155, BCGA CP 52): the gas
        // pushes out an equal volume of fresh air.
        let vg = to_volume(releasable);
        x_mixed = (vg / volume).min(1.0);
        let into_a = f_in * vg;
        let overflow = (into_a - v_a).max(0.0);
        x_zone = [(into_a / v_a).min(1.0), (((1.0 - f_in) * vg + overflow) / v_b).min(1.0)];
        m = m_end;
    }

    let o2_of = |x: f64| O2_AMBIENT_PERCENT * (1.0 - x);
    let co2_of = |x: f64| 100.0 * x + CO2_BACKGROUND_PERCENT * (1.0 - x);
    // Worst of the room models: both zones count (the "other" zone can be the
    // worse one with a low injection fraction or an extraction in the layer).
    let worst = |xm: f64, xz: [f64; 2], xn: f64| {
        let room_worst = if two_zone { xm.max(xz[0]).max(xz[1]) } else { xm };
        if near_in_assessment {
            room_worst.max(xn)
        } else {
            room_worst
        }
    };

    let (mdot0, regime0) = if constant { (mdot_const, FlowRegime::None) } else { src().mass_flow(initial_mass) };
    let t_max = config.model.max_duration_h * 3600.0;

    let mut time = vec![0.0];
    let mut o2_mixed = vec![o2_of(x_mixed)];
    let mut o2_zone = vec![o2_of(x_zone[0])];
    let mut o2_other = vec![o2_of(x_zone[1])];
    let mut o2_near = vec![o2_of(x_near[0])];
    let mut o2 = vec![o2_of(worst(x_mixed, x_zone, x_near[0]))];
    let mut co2 = vec![co2_of(worst(x_mixed, x_zone, x_near[0]))];
    let mut mass_flow = vec![if kind == ReleaseKind::Instantaneous { 0.0 } else { mdot0 }];
    let gauge = |p: f64| ((p - p_amb) / 1e5).max(0.0);
    let pressure_of = |mass: f64| if constant { 0.0 } else { gauge(src().pressure(mass)) };
    let mut pressure = vec![pressure_of(m)];
    let mut released = vec![initial_mass - m];
    o2_tracker.init(o2[0]);
    near_tracker.init(o2_near[0]);
    if is_co2 {
        co2_tracker.init(co2[0]);
    }

    let mut t = 0.0;
    let mut t90 = None;
    let mut t99 = None;
    let mut leak_end: Option<f64> = if kind == ReleaseKind::Instantaneous { Some(0.0) } else { None };
    let mut truncated = false;

    let flow_at = |mass: f64, time: f64| -> f64 {
        if t_iso.is_some_and(|ti| time >= ti - 1e-9) {
            return 0.0; // leak isolated: no more gas enters the room
        }
        if constant {
            if time < t_const - 1e-9 && mass > 0.0 {
                mdot_const
            } else {
                0.0
            }
        } else {
            src().mass_flow(mass).0
        }
    };

    loop {
        // Events due now (the previous step landed exactly on their time)
        while next_event < timeline.len() && timeline[next_event].time_s <= t + 1e-9 {
            apply(&mut room_now, &timeline[next_event]);
            next_event += 1;
        }
        let mdot = flow_at(m, t);
        let remaining = (m - m_end).max(0.0);
        let tau_src = if constant {
            f64::INFINITY
        } else if mdot > 0.0 {
            remaining / mdot
        } else {
            f64::INFINITY
        };
        let q_in_now = to_volume(mdot);
        let q_out_now = room_now.effective_outflow_m3s(q_in_now);
        let tau_room = if q_out_now > 0.0 { volume / q_out_now } else { f64::INFINITY };
        // The room equations are integrated exactly: the step only controls the
        // time resolution of the curves (≈ 20 000 points over the horizon at most).
        let dt_max = (tau_room / 40.0).max(t_max / 20_000.0).clamp(0.01, 30.0);
        let mut dt = if mdot > 0.0 && !constant { (0.02 * tau_src).clamp(1e-3, dt_max) } else { dt_max };
        if constant && t < t_const && t + dt > t_const {
            dt = t_const - t; // land exactly on the end of the release
        }
        if let Some(te) = timeline.get(next_event).map(|e| e.time_s) {
            if te > t + 1e-9 && t + dt > te {
                dt = te - t; // land exactly on the next event
            }
        }
        if t + dt > t_max {
            dt = (t_max - t).max(1e-6);
        }

        let mut m_new = m;
        if constant && mdot > 0.0 {
            let overlap = (t + dt).min(t_const) - t.min(t_const);
            m_new = (m - mdot_const * overlap.max(0.0)).max(0.0);
        } else if mdot > 0.0 {
            // RK4 on the container inventory
            let flow = |mass: f64| src().mass_flow(mass).0;
            let k1 = -flow(m);
            let k2 = -flow((m + 0.5 * dt * k1).max(m_end));
            let k3 = -flow((m + 0.5 * dt * k2).max(m_end));
            let k4 = -flow((m + dt * k3).max(m_end));
            m_new = (m + dt / 6.0 * (k1 + 2.0 * k2 + 2.0 * k3 + k4)).max(m_end);
        }
        let dm = m - m_new;
        let q_in = to_volume(dm) / dt;
        let q_out = room_now.effective_outflow_m3s(q_in);
        let (xm_prev, xz_prev, xn_prev) = (x_mixed, x_zone, x_near);
        x_mixed = mixed_step(x_mixed, q_in, q_out, volume, dt);
        let sys_zone = two_zone.then(|| {
            let (qa, qb) = (f_in * q_in, (1.0 - f_in) * q_in);
            let fresh = room_now.nominal_fresh_air_m3s(q_in);
            let (fa, fb) = (share * fresh, (1.0 - share) * fresh);
            // η applies to each zone's whole outflow, never below its gas inflow
            let (oa, ob) = (qa.max(eta * (qa + fa)), qb.max(eta * (qb + fb)));
            Linear2 { a: [[-(oa + q_x) / v_a, q_x / v_a], [q_x / v_b, -(ob + q_x) / v_b]], b: [qa / v_a, qb / v_b] }
        });
        if let Some(sys) = &sys_zone {
            x_zone = sys.step(x_zone, dt);
        }
        // Gas into the near field; the excess volume and the exchange flow go
        // to the far field, which carries the whole room outflow.
        let sys_near = near.then(|| Linear2 {
            a: [[-(q_in + beta) / v_nf, beta / v_nf], [(q_in + beta) / v_ff, -(beta + q_out) / v_ff]],
            b: [q_in / v_nf, 0.0],
        });
        if let Some(sys) = &sys_near {
            x_near = sys.step(x_near, dt);
        }
        m = m_new;
        let t_prev = t;
        t += dt;

        // Exact states inside the step (constant coefficients), for crossing times
        let state_at = |s: f64| {
            let xm = mixed_step(xm_prev, q_in, q_out, volume, s);
            let xz = sys_zone.as_ref().map_or(xz_prev, |sys| sys.step(xz_prev, s));
            let xn = sys_near.as_ref().map_or(xn_prev, |sys| sys.step(xn_prev, s));
            (xm, xz, xn)
        };
        let o2_at = |s: f64| {
            let (xm, xz, xn) = state_at(s);
            o2_of(worst(xm, xz, xn[0]))
        };
        let x_worst = worst(x_mixed, x_zone, x_near[0]);
        let o2_now = o2_of(x_worst);
        o2_tracker.update(t_prev, *o2.last().unwrap_or(&O2_AMBIENT_PERCENT), t, o2_now, &o2_at);
        let o2_near_now = o2_of(x_near[0]);
        if near {
            let near_at = |s: f64| o2_of(state_at(s).2[0]);
            near_tracker.update(t_prev, *o2_near.last().unwrap_or(&O2_AMBIENT_PERCENT), t, o2_near_now, &near_at);
        }
        let co2_now = co2_of(x_worst);
        if is_co2 {
            let co2_at = |s: f64| {
                let (xm, xz, xn) = state_at(s);
                co2_of(worst(xm, xz, xn[0]))
            };
            co2_tracker.update(t_prev, *co2.last().unwrap_or(&0.0), t, co2_now, &co2_at);
        }

        time.push(t);
        o2_mixed.push(o2_of(x_mixed));
        o2_zone.push(o2_of(x_zone[0]));
        o2_other.push(o2_of(x_zone[1]));
        o2_near.push(o2_near_now);
        o2.push(o2_now);
        co2.push(co2_now);
        mass_flow.push(flow_at(m, t));
        pressure.push(pressure_of(m));
        let out = initial_mass - m;
        released.push(out);

        if leavable > 0.0 {
            if t90.is_none() && out >= 0.9 * leavable {
                t90 = Some(t);
            }
            if t99.is_none() && out >= 0.99 * leavable {
                t99 = Some(t);
            }
        }
        let finished = if constant {
            t >= t_const - 1e-9
        } else {
            (m - m_end) <= 1e-4 * leavable.max(1e-12) || flow_at(m, t) < 1e-9
        };
        if finished && leak_end.is_none() {
            leak_end = Some(t);
        }

        if let Some(te) = leak_end {
            // Never stop before the last event: a backup fan may still start.
            let events_done = next_event >= timeline.len();
            let q_v_now = room_now.effective_ventilation_m3s();
            let recovered = o2_now >= O2_AMBIENT_PERCENT - 0.05;
            if events_done && q_v_now > 0.0 && recovered && t > te {
                break;
            }
            if events_done && q_v_now <= 0.0 && t >= te + (0.25 * te).max(600.0) {
                break;
            }
        }
        if t >= t_max - 1e-9 {
            // The minimum may still be ahead when O₂ is still falling, or when
            // the release goes on and the minimum sits at the last point. With a
            // non-increasing source flow (container blowdown) and a constant
            // ventilation, once O₂ has passed its minimum and rises it cannot
            // fall below it again (the steady state the room tends to only
            // rises), so an ongoing but decaying release does not by itself
            // make the minimum uncertain; thresholds still exceeded are then
            // reported as censored durations.
            let n = o2.len();
            let still_falling = n >= 2 && o2[n - 1] < o2[n - 2] - 1e-9;
            let min_at_end = o2.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).is_some_and(|(i, _)| i == n - 1);
            truncated = still_falling || (leak_end.is_none() && min_at_end);
            break;
        }
        if time.len() > 400_000 {
            truncated = true;
            break;
        }
    }

    // Thresholds still exceeded at the end: durations are lower bounds.
    o2_tracker.finish();
    near_tracker.finish();
    co2_tracker.finish();
    let (min_idx, &min_o2) =
        o2.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).unwrap_or((0, &O2_AMBIENT_PERCENT));
    let governing = {
        let close = |series: &[f64]| series.get(min_idx).is_some_and(|v| (v - min_o2).abs() < 1e-9);
        if near_in_assessment
            && close(&o2_near)
            && !close(&o2_mixed)
            && !(two_zone && (close(&o2_zone) || close(&o2_other)))
        {
            Governing::NearField
        } else if two_zone && close(&o2_zone) && !close(&o2_mixed) {
            Governing::AccumulationZone
        } else if two_zone && close(&o2_other) && !close(&o2_mixed) {
            Governing::OtherZone
        } else {
            Governing::Mixed
        }
    };
    let min_o2_mixed = o2_mixed.iter().copied().fold(O2_AMBIENT_PERCENT, f64::min);
    let min_o2_zone = two_zone.then(|| o2_zone.iter().copied().fold(O2_AMBIENT_PERCENT, f64::min));
    let (min_near_idx, &min_o2_near_val) =
        o2_near.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).unwrap_or((0, &O2_AMBIENT_PERCENT));
    let max_gas_fraction = 1.0 - min_o2 / O2_AMBIENT_PERCENT;
    let releasable_volume_m3 = to_volume(releasable);
    let simplified_volume = if constant {
        config.release.constant_flow_m3h * config.release.constant_duration_min / 60.0
    } else if gas.is_liquefied() {
        0.5344 * src().initial_mass
    } else {
        config.source.pressure_bar_g * config.source.volume_liters * f64::from(config.source.count.max(1)) / 1000.0
    };

    let summary = Summary {
        initial_mass,
        releasable_mass: releasable,
        released_mass: initial_mass - m,
        releasable_volume_m3,
        initial_pressure_bar_a: if constant { 0.0 } else { src().initial_pressure / 1e5 },
        initial_mass_flow: if config.release.kind == ReleaseKind::Instantaneous { f64::INFINITY } else { mdot0 },
        initial_volume_flow_m3h: if config.release.kind == ReleaseKind::Instantaneous {
            f64::INFINITY
        } else {
            to_volume(mdot0) * 3600.0
        },
        initial_regime: if config.release.kind == ReleaseKind::Instantaneous { FlowRegime::None } else { regime0 },
        time_90_percent: if config.release.kind == ReleaseKind::Instantaneous { Some(0.0) } else { t90 },
        release_duration: if config.release.kind == ReleaseKind::Instantaneous { Some(0.0) } else { t99 },
        effective_ventilation_m3h: q_v * 3600.0,
        min_o2,
        time_at_min: time[min_idx],
        min_o2_mixed,
        min_o2_zone,
        max_gas_fraction,
        instant_displacement_o2: o2_of((releasable_volume_m3 / volume).min(1.0)),
        instant_free_efflux_o2: o2_of(1.0 - (-releasable_volume_m3 / volume).exp()),
        simplified_method_o2: 21.0 * (1.0 - (simplified_volume / volume).min(1.0)),
        simplified_method_volume_m3: simplified_volume,
        two_zone,
        buoyancy,
        truncated,
        liquid_full: source.as_ref().is_some_and(|s| s.liquid_full),
        simulated_duration: t,
        o2_crossings: o2_tracker.crossings,
        co2_crossings: if is_co2 { co2_tracker.crossings } else { Vec::new() },
        max_co2_percent: is_co2.then(|| co2.iter().copied().fold(0.0, f64::max)),
        min_o2_near: near.then_some(min_o2_near_val),
        time_at_min_near: near.then(|| time[min_near_idx]),
        near_field_volume_m3: near.then_some(v_nf),
        near_field_exchange_m3h: near.then_some(beta * 3600.0),
        near_crossings: if near { near_tracker.crossings } else { Vec::new() },
        governing,
    };

    Ok(SimulationResult {
        time,
        o2_mixed,
        o2_zone: two_zone.then_some(o2_zone),
        o2_other: two_zone.then_some(o2_other),
        o2,
        o2_near: near.then_some(o2_near),
        co2: is_co2.then_some(co2),
        mass_flow,
        source_pressure_bar_g: pressure,
        released_mass: released,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExtractionLocation, StratificationMode, VentilationMode};
    use crate::gas::GasType;
    use approx::assert_relative_eq;

    fn base(gas: GasType) -> Configuration {
        let mut c = Configuration { gas, ..Configuration::default() };
        c.model.stratification = StratificationMode::Off;
        c
    }

    /// Shared set-up of the limit cases below (imposed flow, no ventilation).
    fn limit_base(gas: GasType) -> Configuration {
        let mut c = base(gas);
        c.release.kind = ReleaseKind::ConstantFlow;
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.room.ventilation_efficiency = 1.0;
        c
    }

    fn event(time_s: f64, action: EventAction) -> Event {
        Event { time_s, action }
    }

    #[test]
    fn ventilation_stop_at_start_equals_no_ventilation() {
        let mut off = base(GasType::Nitrogen);
        off.room.mechanical_enabled = false;
        let mut ev = base(GasType::Nitrogen);
        ev.room.mechanical_enabled = true;
        ev.room.mechanical_flow_m3h = 300.0;
        ev.events.push(event(0.0, EventAction::VentilationStops));
        let (a, b) = (simulate(&off).unwrap(), simulate(&ev).unwrap());
        assert_eq!(a.o2, b.o2);
        assert_eq!(a.time, b.time);
        assert_eq!(a.summary.min_o2, b.summary.min_o2);
        assert_eq!(a.summary.effective_ventilation_m3h, b.summary.effective_ventilation_m3h);
    }

    #[test]
    fn event_after_the_horizon_has_no_effect() {
        let mut c = base(GasType::Argon);
        c.model.max_duration_h = 1.0;
        let plain = simulate(&c).unwrap();
        c.events.push(event(2.0 * 3600.0, EventAction::VentilationStops));
        let with = simulate(&c).unwrap();
        assert_eq!(plain.o2, with.o2);
        assert_eq!(plain.time, with.time);
    }

    #[test]
    fn restart_after_the_release_gives_the_analytic_recovery() {
        // 10 m³/h of argon for 30 min into 50 m³ without ventilation, then a
        // 100 m³/h supply fan (η = 1) starts at t₀ = 60 min: the gas fraction
        // is frozen until t₀, then decays as exp(−Q·(t − t₀)/V).
        let mut c = limit_base(GasType::Argon);
        c.room.volume_m3 = 50.0;
        c.release.constant_flow_m3h = 10.0;
        c.release.constant_duration_min = 30.0;
        c.model.max_duration_h = 3.0;
        let t0 = 3600.0;
        c.events.push(event(t0, EventAction::VentilationStarts { flow_m3h: 100.0, mode: VentilationMode::Supply }));
        let r = simulate(&c).unwrap();
        let interp = |t: f64| {
            let i = r.time.iter().position(|&x| x >= t - 1e-9).expect("inside the series");
            if i == 0 || (r.time[i] - t).abs() < 1e-9 {
                return r.o2[i];
            }
            let f = (t - r.time[i - 1]) / (r.time[i] - r.time[i - 1]);
            r.o2[i - 1] + f * (r.o2[i] - r.o2[i - 1])
        };
        // The step lands exactly on the event
        assert!(r.time.iter().any(|&x| (x - t0).abs() < 1e-9));
        let x0 = 1.0 - interp(t0) / O2_AMBIENT_PERCENT;
        assert_relative_eq!(x0, 1.0 - (-5.0_f64 / 50.0).exp(), epsilon = 1e-6);
        let tau = 50.0 / 100.0 * 3600.0;
        let x_tau = 1.0 - interp(t0 + tau) / O2_AMBIENT_PERCENT;
        assert_relative_eq!(x_tau / x0, (-1.0_f64).exp(), max_relative = 1e-3);
        // Ventilation credited at the start: none (the fan starts later)
        assert_eq!(r.summary.effective_ventilation_m3h, 0.0);
        // The recovery is observed: the run does not stop before the fan starts
        assert!(r.o2.last().copied().unwrap() > O2_AMBIENT_PERCENT - 0.1);
    }

    #[test]
    fn simultaneous_events_are_applied_in_a_fixed_order() {
        // A stop and a start at the same time: the start wins whatever the input order.
        let start = EventAction::VentilationStarts { flow_m3h: 500.0, mode: VentilationMode::Extraction };
        let mut a = base(GasType::Nitrogen);
        a.room.mechanical_enabled = true;
        a.events = vec![event(60.0, start), event(60.0, EventAction::VentilationStops)];
        let mut b = a.clone();
        b.events.reverse();
        let (ra, rb) = (simulate(&a).unwrap(), simulate(&b).unwrap());
        assert_eq!(ra.o2, rb.o2);
        let mut c = a.clone();
        c.events = vec![event(60.0, start)];
        assert_eq!(simulate(&c).unwrap().o2, ra.o2);
    }

    #[test]
    fn isolation_stops_the_release_and_keeps_the_mass_balance() {
        let mut c = base(GasType::Nitrogen);
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.events.push(event(30.0, EventAction::ReleaseIsolated));
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        assert!(s.released_mass < 0.5 * s.releasable_mass);
        assert_eq!(s.released_mass, r.released_mass.last().copied().unwrap());
        let i = r.time.iter().position(|&x| x >= 30.0 - 1e-9).unwrap();
        assert!(r.mass_flow[i..].iter().all(|&f| f == 0.0));
        assert!(r.time.iter().any(|&x| (x - 30.0).abs() < 1e-9));
        // No ventilation: O₂ stays at its minimum once the leak is isolated
        assert!((r.o2.last().unwrap() - s.min_o2).abs() < 1e-9);
        assert_eq!(s.release_duration, None);
        c.events.clear();
        assert!(simulate(&c).unwrap().summary.min_o2 < s.min_o2);
    }

    #[test]
    fn isolated_zone_keeps_its_source_when_the_matrix_is_singular() {
        // Limit case: argon, 60 m³ of height 3 m, 1 m layer (20 m³), 12 m³/h
        // for 1 h all injected in the layer, no exchange, no ventilation. The
        // layer is an isolated zone: O₂ = 20.95·exp(−12/20) = 11.4976 %.
        let mut c = limit_base(GasType::Argon);
        c.room.volume_m3 = 60.0;
        c.room.height_m = 3.0;
        c.release.constant_flow_m3h = 12.0;
        c.release.constant_duration_min = 60.0;
        c.model.stratification = StratificationMode::On;
        c.model.zone_height_m = 1.0;
        c.model.injection_fraction = 1.0;
        c.model.exchange_ach = 0.0;
        let r = simulate(&c).unwrap();
        let expected = 20.95 * (-12.0_f64 / 20.0).exp();
        assert!((r.summary.min_o2_zone.unwrap() - expected).abs() < 1e-6, "{}", r.summary.min_o2_zone.unwrap());
        // The uncoupled other zone receives nothing
        let other_min = r.o2_other.as_ref().unwrap().iter().copied().fold(21.0, f64::min);
        assert!((other_min - O2_AMBIENT_PERCENT).abs() < 1e-9);
        // Continuity when the exchange tends to zero
        c.model.exchange_ach = 1e-6;
        let r2 = simulate(&c).unwrap();
        assert!((r2.summary.min_o2_zone.unwrap() - expected).abs() < 1e-4);
        // The room average is unchanged by the zone model
        assert!((r.summary.min_o2_mixed - 20.95 * (-12.0_f64 / 60.0).exp()).abs() < 1e-6);
    }

    #[test]
    fn classification_takes_the_worse_of_both_zones() {
        // Limit case: 20 % of the release into the layer, the rest of the
        // room (80 % of 12.5 m³/h into 40 m³) ends lower than the layer.
        let mut c = limit_base(GasType::Argon);
        c.room.volume_m3 = 60.0;
        c.room.height_m = 3.0;
        c.release.constant_flow_m3h = 12.5;
        c.release.constant_duration_min = 60.0;
        c.model.stratification = StratificationMode::On;
        c.model.zone_height_m = 1.0;
        c.model.injection_fraction = 0.2;
        c.model.exchange_ach = 0.0;
        let r = simulate(&c).unwrap();
        let other_min = r.o2_other.as_ref().unwrap().iter().copied().fold(21.0, f64::min);
        assert!((other_min - 20.95 * (-10.0_f64 / 40.0).exp()).abs() < 1e-6);
        assert!((r.summary.min_o2 - other_min).abs() < 1e-9, "assessed {} vs other zone {other_min}", r.summary.min_o2);
        assert_eq!(r.summary.governing, Governing::OtherZone);
        assert!(r.summary.o2_crossing(17.0).unwrap().reached());
        // Symmetric case: the layer governs
        c.model.injection_fraction = 0.8;
        let r = simulate(&c).unwrap();
        assert_eq!(r.summary.governing, Governing::AccumulationZone);
    }

    #[test]
    fn crossing_time_does_not_depend_on_the_output_step() {
        // Limit case: 1 m³, 3 600 m³/h for 1 min, no ventilation; the 19 %
        // crossing is at t = ln(20.95/19) s = 0.0977 s whatever the horizon.
        let mut c = limit_base(GasType::Nitrogen);
        c.room.volume_m3 = 1.0;
        c.release.constant_flow_m3h = 3600.0;
        c.release.constant_duration_min = 1.0;
        let exact = (20.95_f64 / 19.0).ln();
        let r = simulate(&c).unwrap();
        let t19 = r.summary.o2_crossing(19.0).unwrap().first.unwrap();
        assert!(((t19 - exact) / exact).abs() < 1e-4, "{t19} vs {exact}, first step {}", r.time[1]);
        c.model.max_duration_h = 0.1;
        let t19b = simulate(&c).unwrap().summary.o2_crossing(19.0).unwrap().first.unwrap();
        assert!(((t19b - exact) / exact).abs() < 1e-4);
        assert!((t19 - t19b).abs() < 1e-6 * exact);
    }

    #[test]
    fn imposed_flow_ignores_container_parameters() {
        // Limit case: an imposed CO₂ flow does not depend on a hidden cylinder at 40 °C
        let mut c = limit_base(GasType::CarbonDioxide);
        c.source.temperature_c = 40.0;
        let r = simulate(&c).expect("the container is not part of this scenario");
        c.source.temperature_c = 15.0;
        let r2 = simulate(&c).unwrap();
        assert!((r.summary.min_o2 - r2.summary.min_o2).abs() < 1e-12);
        assert!(!r.summary.liquid_full);
    }

    #[test]
    fn censored_durations_are_flagged() {
        // Limit case: instantaneous B50 of nitrogen in 60 m³, 0.01 vol/h, 6 min horizon
        let mut c = limit_base(GasType::Nitrogen);
        c.release.kind = ReleaseKind::Instantaneous;
        c.room.volume_m3 = 60.0;
        c.room.natural_ach = 0.01;
        c.model.max_duration_h = 0.1;
        let r = simulate(&c).unwrap();
        let x = r.summary.o2_crossing(19.0).unwrap();
        assert!(x.reached() && x.back.is_none() && x.censored, "{x:?}");
        assert!((x.total - r.summary.simulated_duration).abs() < 1e-6);
        // A crossing that recovers within the horizon is not censored
        c.room.mechanical_enabled = true;
        c.room.mechanical_flow_m3h = 3000.0;
        c.model.max_duration_h = 24.0;
        let r = simulate(&c).unwrap();
        let x = r.summary.o2_crossing(19.0).unwrap();
        assert!(x.back.is_some() && !x.censored, "{x:?}");
    }

    #[test]
    fn gauge_pressure_is_relative_to_the_room_atmosphere() {
        // Limit case: 0.1 barg read on the regulator in a 1 300 hPa atmosphere
        let mut c = limit_base(GasType::Nitrogen);
        c.release.kind = ReleaseKind::Leak;
        c.source.pressure_bar_g = 0.1;
        c.room.pressure_hpa = 1300.0;
        let r = simulate(&c).expect("0.1 barg above a 1.3 bar atmosphere is a valid source");
        assert!((r.summary.initial_pressure_bar_a - 1.4).abs() < 1e-9, "{}", r.summary.initial_pressure_bar_a);
        assert!(r.source_pressure_bar_g[0] <= 0.1 + 1e-9);
    }

    #[test]
    fn results_survive_a_json_round_trip_for_an_instantaneous_release() {
        // Limit case: the undefined (infinite) flow of an instantaneous release
        let mut c = limit_base(GasType::Nitrogen);
        c.release.kind = ReleaseKind::Instantaneous;
        let run = crate::run(&c).unwrap();
        assert!(run.result.summary.initial_mass_flow.is_infinite());
        let json = serde_json::to_string(&run).unwrap();
        assert!(json.contains("\"Infinity\""));
        let back: crate::SimulationRun = serde_json::from_str(&json).unwrap();
        assert!(back.result.summary.initial_mass_flow.is_infinite());
        assert_eq!(back.result.summary.o2_crossings, run.result.summary.o2_crossings);
        // Older files wrote `null`
        let legacy = json.replace("\"Infinity\"", "null");
        let back: crate::SimulationRun = serde_json::from_str(&legacy).unwrap();
        assert!(back.result.summary.initial_volume_flow_m3h.is_infinite());
    }

    #[test]
    fn exact_single_zone_matches_analytic_solution() {
        // Constant inflow: x(t) = a/b (1 − e^{−bt})
        let (q_in, q_v, v) = (0.01, 0.05, 30.0);
        let mut x = 0.0;
        for _ in 0..100 {
            x = mixed_step(x, q_in, q_in + q_v, v, 3.0);
        }
        let b = (q_in + q_v) / v;
        let expected = q_in / (q_in + q_v) * (1.0 - (-b * 300.0).exp());
        assert_relative_eq!(x, expected, max_relative = 1e-12);
    }

    #[test]
    fn near_field_with_huge_exchange_equals_mixed_and_is_capped_in_small_rooms() {
        let mut c = base(GasType::Nitrogen);
        c.release.kind = ReleaseKind::ConstantFlow;
        c.release.constant_flow_m3h = 360.0;
        c.release.constant_duration_min = 2.0;
        c.room.volume_m3 = 10.0;
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.model.near_field_radius_m = 1.0;
        c.model.near_field_air_speed_m_s = 1.0; // β = 3.1 m³/s ≫ 0.1 m³/s release
        let r = simulate(&c).unwrap();
        let near = r.o2_near.as_ref().unwrap();
        // With a strong exchange the near field follows the room average closely
        let worst_gap = near.iter().zip(&r.o2_mixed).map(|(n, m)| (n - m).abs()).fold(0.0, f64::max);
        assert!(worst_gap < 0.6, "gap {worst_gap}");
        assert!(r.summary.min_o2_near.unwrap() < r.summary.min_o2_mixed + 1e-9);
        // Hemisphere of 2.1 m³ in a 3 m³ room: capped at half the room
        c.room.volume_m3 = 3.0;
        c.release.constant_flow_m3h = 36.0;
        let r = simulate(&c).unwrap();
        assert!((r.summary.near_field_volume_m3.unwrap() - 1.5).abs() < 1e-9);
        // Instantaneous release: no near field
        c.release.kind = ReleaseKind::Instantaneous;
        let r = simulate(&c).unwrap();
        assert!(r.o2_near.is_none() && r.summary.min_o2_near.is_none());
    }

    #[test]
    fn near_field_reproduces_odhmod_confined_space_example() {
        // ODHMOD example 10 (Stefana et al. 2021, AIHA): 10 m³, pure N₂ at
        // 0.10 m³/s (102 kPa) for 120 s, half-sphere of 1 m, air speed 0.06 m/s.
        // Fine-step re-computation of the published scheme: near field reaches
        // 12 % after 26.2 s and bottoms at 4.36 %; the room average bottoms at 6.3 %.
        let mut c = base(GasType::Nitrogen);
        c.release.kind = ReleaseKind::ConstantFlow;
        c.release.constant_flow_m3h = 0.10 * 102.0 / 101.325 * 3600.0;
        c.release.constant_duration_min = 2.0;
        c.room.volume_m3 = 10.0;
        c.room.height_m = 2.5;
        c.room.temperature_c = 25.0;
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.room.ventilation_efficiency = 1.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        let t12 = s.near_crossing(12.0).and_then(|x| x.first).unwrap();
        assert!((t12 - 26.2).abs() < 1.0, "near field reaches 12 % at {t12} s");
        assert!((s.min_o2_near.unwrap() - 4.36).abs() < 0.2, "near-field minimum {}", s.min_o2_near.unwrap());
        assert!((s.min_o2_mixed - 6.31).abs() < 0.1, "room minimum {}", s.min_o2_mixed);
        // Not in the assessment by default: the classification follows the room
        assert!((s.min_o2 - s.min_o2_mixed).abs() < 1e-9);
        c.model.near_field_in_assessment = true;
        let r = simulate(&c).unwrap();
        assert!((r.summary.min_o2 - r.summary.min_o2_near.unwrap()).abs() < 1e-9);
    }

    #[test]
    fn two_zone_with_infinite_exchange_equals_mixed() {
        let (v, va) = (100.0, 30.0);
        let q_in = 0.02;
        let q_v = 0.05;
        let qx = 1e3;
        let sys = Linear2 {
            a: [
                [-(0.8 * q_in + 0.3 * q_v + qx) / va, qx / va],
                [qx / (v - va), -(0.2 * q_in + 0.7 * q_v + qx) / (v - va)],
            ],
            b: [0.8 * q_in / va, 0.2 * q_in / (v - va)],
        };
        let mut z = [0.0, 0.0];
        let mut x = 0.0;
        for _ in 0..200 {
            z = sys.step(z, 5.0);
            x = mixed_step(x, q_in, q_in + q_v, v, 5.0);
        }
        let avg = (z[0] * va + z[1] * (v - va)) / v;
        assert_relative_eq!(avg, x, max_relative = 1e-3);
    }

    #[test]
    fn mass_is_conserved_without_ventilation() {
        // No ventilation, small leak: final gas fraction must equal 1 − exp(−Vg/V)
        let mut c = base(GasType::Nitrogen);
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.room.volume_m3 = 50.0;
        c.release.diameter_mm = 1.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        assert!(!s.truncated);
        let expected = O2_AMBIENT_PERCENT * (-s.releasable_volume_m3 / 50.0).exp();
        assert_relative_eq!(s.min_o2, expected, max_relative = 2e-3);
        assert_relative_eq!(s.released_mass, s.releasable_mass, max_relative = 1e-3);
    }

    #[test]
    fn instantaneous_release_is_the_no_ventilation_bound() {
        let mut c = base(GasType::Argon);
        c.release.kind = ReleaseKind::Instantaneous;
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        let r = simulate(&c).unwrap();
        assert_relative_eq!(r.summary.min_o2, r.summary.instant_displacement_o2, max_relative = 1e-9);
        assert_relative_eq!(r.o2[0], r.summary.min_o2, max_relative = 1e-9);
    }

    #[test]
    fn faster_release_is_never_less_severe_than_slow_one() {
        let mut c = base(GasType::Nitrogen);
        c.release.diameter_mm = 0.5;
        let slow = simulate(&c).unwrap().summary.min_o2;
        c.release.diameter_mm = 8.0;
        let fast = simulate(&c).unwrap().summary.min_o2;
        c.release.kind = ReleaseKind::Instantaneous;
        let instant = simulate(&c).unwrap().summary.min_o2;
        assert!(fast <= slow + 1e-9);
        assert!(instant <= fast + 1e-6);
    }

    #[test]
    fn more_ventilation_gives_more_oxygen() {
        let mut c = base(GasType::Nitrogen);
        c.release.diameter_mm = 1.0;
        let mut last = 0.0;
        for q in [0.0, 50.0, 200.0, 1000.0] {
            c.room.mechanical_flow_m3h = q;
            let min = simulate(&c).unwrap().summary.min_o2;
            assert!(min >= last - 1e-9, "q={q}: {min} < {last}");
            last = min;
        }
    }

    #[test]
    fn oxygen_stays_within_physical_bounds() {
        let mut c = base(GasType::Helium);
        c.room.volume_m3 = 1.0;
        c.release.kind = ReleaseKind::ValveRupture;
        c.release.diameter_mm = 20.0;
        let r = simulate(&c).unwrap();
        assert!(r.o2.iter().all(|&v| (0.0..=O2_AMBIENT_PERCENT + 1e-9).contains(&v) && v.is_finite()));
        assert!(r.summary.min_o2 < 1.0);
    }

    #[test]
    fn blowdown_pressure_decreases_monotonically() {
        let mut c = base(GasType::Nitrogen);
        c.release.diameter_mm = 2.0;
        let r = simulate(&c).unwrap();
        assert!(r.source_pressure_bar_g.windows(2).all(|w| w[1] <= w[0] + 1e-9));
        assert!(*r.source_pressure_bar_g.last().unwrap() < 0.05);
        assert!(r.summary.release_duration.is_some());
    }

    #[test]
    fn isothermal_choked_blowdown_time_constant() {
        // Ideal-gas choked isothermal blowdown: P = P0·exp(−t/τ), τ = m0/ṁ0.
        // Time to drop from 200 to 100 bar(a) must be τ·ln 2 (Z effects < 3 %).
        let mut c = base(GasType::Nitrogen);
        c.release.diameter_mm = 1.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        let tau = s.initial_mass / s.initial_mass_flow;
        let p0 = r.source_pressure_bar_g[0] + 1.013;
        let idx = r.source_pressure_bar_g.iter().position(|&p| p + 1.013 <= p0 / 2.0).unwrap();
        assert_relative_eq!(r.time[idx], tau * std::f64::consts::LN_2, max_relative = 0.05);
    }

    #[test]
    fn threshold_crossings_are_consistent() {
        let mut c = base(GasType::Nitrogen);
        c.room.volume_m3 = 20.0;
        c.release.diameter_mm = 3.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        let t19 = s.o2_crossing(19.0).unwrap().first.unwrap();
        let t17 = s.o2_crossing(17.0).unwrap().first.unwrap();
        assert!(t19 < t17);
        assert!(t17 <= s.time_at_min);
        // Interpolated crossing time agrees with the series
        let idx = r.o2.iter().position(|&v| v < 19.0).unwrap();
        assert!(r.time[idx - 1] <= t19 && t19 <= r.time[idx]);
        // Recovery with ventilation
        assert!(s.o2_crossing(19.0).unwrap().back.is_some());
    }

    #[test]
    fn co2_is_tracked_and_toxic_before_o2_alarm() {
        let mut c = base(GasType::CarbonDioxide);
        c.room.volume_m3 = 400.0;
        c.release.diameter_mm = 1.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        let co2_max = s.max_co2_percent.unwrap();
        assert!(co2_max > 4.0, "CO2 max {co2_max}");
        assert!(s.co2_crossing(4.0).unwrap().reached());
        // IDLH CO2 reached while O2 still above 19 %
        assert!(s.min_o2 > 19.0 || s.co2_crossing(4.0).unwrap().first < s.o2_crossing(19.0).unwrap().first);
    }

    #[test]
    fn heavy_gas_zone_is_worse_than_mixed() {
        let mut c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        c.model.stratification = StratificationMode::On;
        c.model.extraction = ExtractionLocation::OppositeZone;
        c.release.diameter_mm = 2.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        assert!(s.two_zone);
        assert!(s.min_o2_zone.unwrap() < s.min_o2_mixed);
        assert_relative_eq!(s.min_o2, s.min_o2_zone.unwrap(), max_relative = 1e-12);
    }

    #[test]
    fn low_extraction_helps_heavy_gas() {
        let mut c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        c.model.stratification = StratificationMode::On;
        c.model.extraction = ExtractionLocation::AccumulationZone;
        let low = simulate(&c).unwrap().summary.min_o2;
        c.model.extraction = ExtractionLocation::OppositeZone;
        let high = simulate(&c).unwrap().summary.min_o2;
        assert!(low > high);
    }

    #[test]
    fn long_slow_leak_is_not_truncated_early() {
        let mut c = base(GasType::Nitrogen);
        c.release.diameter_mm = 0.1;
        let r = simulate(&c).unwrap();
        // 0.1 mm leak on a B50 lasts many hours: must run beyond the old 1 h cap
        assert!(r.summary.simulated_duration > 3600.0);
    }

    #[test]
    fn extreme_flow_ratios_give_no_nan() {
        // Huge ventilation in a tiny room with a two-zone model: no NaN, no truncation.
        let mut c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        c.model.stratification = StratificationMode::On;
        c.room.volume_m3 = 2.0;
        c.room.height_m = 2.0;
        c.room.mechanical_flow_m3h = 10_000.0;
        c.release.diameter_mm = 0.1;
        c.model.max_duration_h = 168.0;
        let r = simulate(&c).unwrap();
        assert!(r.o2.iter().chain(r.o2_zone.as_ref().unwrap()).all(|v| v.is_finite()));
        assert!(!r.summary.truncated);
        assert!(r.time.len() < 50_000, "{} points", r.time.len());
    }

    #[test]
    fn adiabatic_blowdown_pressure_is_monotonic() {
        for gas in [GasType::Argon, GasType::Nitrogen, GasType::Helium] {
            let mut c = base(gas);
            c.model.blowdown = crate::config::Blowdown::Adiabatic;
            c.release.diameter_mm = 2.0;
            let r = simulate(&c).unwrap();
            assert!(r.source_pressure_bar_g.windows(2).all(|w| w[1] <= w[0] + 1e-6), "{gas:?}");
            assert!(r.summary.release_duration.is_some(), "{gas:?}");
        }
    }

    /// Deterministic pseudo-random robustness check over the validated input
    /// ranges (a 6 000-configuration campaign was run with
    /// the same invariants).
    #[test]
    fn random_configurations_respect_invariants() {
        use crate::config::{Blowdown, Co2Withdrawal, ReleaseKind, StratificationMode, VentilationMode};
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            f64::from((seed >> 33) as u32) / f64::from(u32::MAX)
        };
        let mut pick = |lo: f64, hi: f64, log: bool| {
            let u = next();
            if log {
                lo * (hi / lo).powf(u)
            } else {
                lo + (hi - lo) * u
            }
        };
        for i in 0..150 {
            let mut c = Configuration { gas: GasType::all()[i % 5], ..Configuration::default() };
            c.source.volume_liters = pick(1.0, 5000.0, true);
            c.source.count = 1 + (pick(0.0, 1.0, false) * 12.0) as u32;
            c.source.pressure_bar_g = pick(1.0, 300.0, true);
            c.source.temperature_c = pick(-20.0, 28.0, false);
            c.source.co2_fill_ratio = pick(0.3, 0.75, false);
            c.source.co2_withdrawal =
                if pick(0.0, 1.0, false) < 0.5 { Co2Withdrawal::Vapour } else { Co2Withdrawal::Liquid };
            c.release.kind = ReleaseKind::all()[i % 4];
            c.release.diameter_mm = pick(0.1, 30.0, true);
            c.release.discharge_coefficient = pick(0.5, 1.0, false);
            c.release.constant_flow_m3h = pick(0.1, 500.0, true);
            c.release.constant_duration_min = pick(1.0, 600.0, true);
            c.room.volume_m3 = pick(2.0, 5000.0, true);
            c.room.height_m = pick(2.0, 8.0, false);
            c.room.mechanical_enabled = pick(0.0, 1.0, false) < 0.7;
            c.room.mechanical_mode =
                if pick(0.0, 1.0, false) < 0.5 { VentilationMode::Extraction } else { VentilationMode::Supply };
            c.room.mechanical_flow_m3h = pick(10.0, 20_000.0, true);
            c.room.natural_ach = pick(0.0, 3.0, false);
            c.room.ventilation_efficiency = pick(0.2, 1.0, false);
            c.model.stratification = [StratificationMode::Auto, StratificationMode::On, StratificationMode::Off][i % 3];
            c.model.zone_height_m = pick(0.3, 1.5, false);
            c.model.exchange_ach = pick(0.0, 20.0, false);
            c.model.blowdown = if pick(0.0, 1.0, false) < 0.3 { Blowdown::Adiabatic } else { Blowdown::Isothermal };
            c.model.max_duration_h = pick(1.0, 48.0, true);
            let r = match simulate(&c) {
                Ok(r) => r,
                Err(_) => continue, // e.g. CO₂ container above the critical temperature
            };
            let s = &r.summary;
            let all_finite = r.o2.iter().chain(&r.o2_mixed).chain(&r.mass_flow).all(|v| v.is_finite());
            assert!(all_finite, "non-finite output for config #{i}");
            assert!(r.o2.iter().all(|&v| (-1e-9..=O2_AMBIENT_PERCENT + 1e-9).contains(&v)), "O₂ out of bounds #{i}");
            assert!((s.min_o2 - r.o2.iter().copied().fold(f64::INFINITY, f64::min)).abs() < 1e-9, "min #{i}");
            assert!(s.released_mass <= s.releasable_mass * (1.0 + 1e-6) + 1e-9, "mass #{i}");
            if c.release.kind.uses_orifice() {
                assert!(r.source_pressure_bar_g.windows(2).all(|w| w[1] <= w[0] + 1e-6), "pressure #{i}");
            }
            let first = |l: f64| s.o2_crossing(l).and_then(|x| x.first);
            if let (Some(a), Some(b)) = (first(19.0), first(17.0)) {
                assert!(a <= b + 1e-9, "crossing order #{i}");
            }
            assert!(r.time.len() < 60_000, "too many points #{i}: {}", r.time.len());
        }
    }

    #[test]
    fn reference_case_values() {
        // Ar B50 200 bar, 8 mm, Cd 0.72, 150 m³, 200 m³/h, η = 0.6, well-mixed.
        // Reference: exact well-mixed minimum ≈ 19.6 % (blowdown).
        let mut c = base(GasType::Argon);
        c.release.diameter_mm = 8.0;
        c.release.discharge_coefficient = 0.72;
        c.room.volume_m3 = 150.0;
        c.room.mechanical_flow_m3h = 200.0;
        c.room.natural_ach = 0.0;
        let r = simulate(&c).unwrap();
        let s = &r.summary;
        assert_relative_eq!(s.initial_mass_flow, 2.13, max_relative = 0.05);
        assert_relative_eq!(s.min_o2, 19.55, epsilon = 0.1);
    }
}
