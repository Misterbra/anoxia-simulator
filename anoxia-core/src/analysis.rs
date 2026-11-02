//! Parametric studies built on top of the solver: parameter sweeps,
//! one-at-a-time sensitivity (tornado), required ventilation and maximum
//! tolerable leak diameter.

use crate::config::{Configuration, ReleaseKind};
use crate::error::Result;
use crate::i18n::Language;
use crate::solver::{simulate, SimulationResult};
use serde::{Deserialize, Serialize};

/// Input parameter that can be varied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Parameter {
    /// Leak diameter (mm)
    LeakDiameter,
    /// Discharge coefficient (–)
    DischargeCoefficient,
    /// Container pressure (bar g)
    Pressure,
    /// Container water capacity (L)
    ContainerVolume,
    /// Room volume (m³)
    RoomVolume,
    /// Mechanical ventilation flow (m³/h)
    MechanicalFlow,
    /// Natural air change rate (vol/h)
    NaturalAch,
    /// Ventilation effectiveness (–)
    Efficiency,
    /// Imposed gas flow (m³/h), constant-flow releases
    ReleaseFlow,
}

impl Parameter {
    /// All parameters.
    #[must_use]
    pub fn all() -> [Self; 9] {
        [
            Self::ReleaseFlow,
            Self::LeakDiameter,
            Self::DischargeCoefficient,
            Self::Pressure,
            Self::ContainerVolume,
            Self::RoomVolume,
            Self::MechanicalFlow,
            Self::NaturalAch,
            Self::Efficiency,
        ]
    }

    /// Localised name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::LeakDiameter => lang.pick("Diamètre de fuite", "Leak diameter"),
            Self::DischargeCoefficient => lang.pick("Coefficient de décharge", "Discharge coefficient"),
            Self::Pressure => lang.pick("Pression bouteille", "Container pressure"),
            Self::ContainerVolume => lang.pick("Capacité bouteille", "Container capacity"),
            Self::RoomVolume => lang.pick("Volume du local", "Room volume"),
            Self::MechanicalFlow => lang.pick("Débit de ventilation mécanique", "Mechanical ventilation flow"),
            Self::NaturalAch => lang.pick("Renouvellement naturel", "Natural air change"),
            Self::Efficiency => lang.pick("Efficacité de ventilation", "Ventilation effectiveness"),
            Self::ReleaseFlow => lang.pick("Débit de gaz imposé", "Imposed gas flow"),
        }
    }

    /// Unit symbol.
    #[must_use]
    pub fn unit(self) -> &'static str {
        match self {
            Self::LeakDiameter => "mm",
            Self::DischargeCoefficient | Self::Efficiency => "",
            Self::Pressure => "bar",
            Self::ContainerVolume => "L",
            Self::RoomVolume => "m³",
            Self::MechanicalFlow | Self::ReleaseFlow => "m³/h",
            Self::NaturalAch => "vol/h",
        }
    }

    /// Current value in a configuration.
    #[must_use]
    pub fn get(self, c: &Configuration) -> f64 {
        match self {
            Self::LeakDiameter => c.release.diameter_mm,
            Self::DischargeCoefficient => c.release.discharge_coefficient,
            Self::Pressure => c.source.pressure_bar_g,
            Self::ContainerVolume => c.source.volume_liters,
            Self::RoomVolume => c.room.volume_m3,
            Self::MechanicalFlow => c.room.mechanical_flow_m3h,
            Self::NaturalAch => c.room.natural_ach,
            Self::Efficiency => c.room.ventilation_efficiency,
            Self::ReleaseFlow => c.release.constant_flow_m3h,
        }
    }

    /// Set the value (clamped to its valid range).
    pub fn set(self, c: &mut Configuration, v: f64) {
        match self {
            Self::LeakDiameter => c.release.diameter_mm = v.clamp(0.05, 100.0),
            Self::DischargeCoefficient => c.release.discharge_coefficient = v.clamp(0.3, 1.0),
            Self::Pressure => c.source.pressure_bar_g = v.clamp(0.1, 700.0),
            Self::ContainerVolume => c.source.volume_liters = v.clamp(0.1, 100_000.0),
            Self::RoomVolume => c.room.volume_m3 = v.clamp(0.5, 1_000_000.0),
            Self::MechanicalFlow => {
                c.room.mechanical_enabled = true;
                c.room.mechanical_flow_m3h = v.clamp(0.0, 10_000_000.0);
            }
            Self::NaturalAch => c.room.natural_ach = v.clamp(0.0, 50.0),
            Self::Efficiency => c.room.ventilation_efficiency = v.clamp(0.05, 1.0),
            Self::ReleaseFlow => c.release.constant_flow_m3h = v.clamp(1e-4, 1e6),
        }
    }

    /// Whether the parameter influences the given configuration.
    #[must_use]
    pub fn applicable(self, c: &Configuration) -> bool {
        match self {
            Self::LeakDiameter | Self::DischargeCoefficient => c.release.kind.uses_orifice(),
            Self::Pressure => !c.gas.is_liquefied() && c.release.kind != ReleaseKind::ConstantFlow,
            Self::ContainerVolume => c.release.kind != ReleaseKind::ConstantFlow,
            Self::ReleaseFlow => c.release.kind == ReleaseKind::ConstantFlow,
            Self::MechanicalFlow => c.room.mechanical_enabled,
            _ => true,
        }
    }

    /// Default sweep range around the current value.
    #[must_use]
    pub fn default_range(self, c: &Configuration) -> (f64, f64) {
        let v = self.get(c);
        match self {
            Self::LeakDiameter => ((v * 0.25).max(0.1), (v * 4.0).min(50.0)),
            Self::DischargeCoefficient => (0.5, 1.0),
            Self::Pressure => ((v * 0.25).max(1.0), (v * 1.5).min(700.0)),
            Self::ContainerVolume | Self::RoomVolume => ((v * 0.25).max(0.5), v * 4.0),
            Self::MechanicalFlow => (0.0, (v * 4.0).max(100.0)),
            Self::NaturalAch => (0.0, (v * 4.0).max(2.0)),
            Self::Efficiency => (0.2, 1.0),
            Self::ReleaseFlow => (v * 0.25, v * 4.0),
        }
    }
}

/// Key results for one point of a study.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Parameter value
    pub value: f64,
    /// Minimum O₂ (% vol)
    pub min_o2: f64,
    /// Time to alarm 1 (s)
    pub t_alarm1: Option<f64>,
    /// Time to alarm 2 (s)
    pub t_alarm2: Option<f64>,
    /// Peak CO₂ (% vol), CO₂ only
    pub max_co2: Option<f64>,
}

fn point(c: &Configuration, value: f64) -> Option<Point> {
    let r = simulate(c).ok()?;
    let (a1, a2) = c.thresholds.levels();
    let first = |l: f64| r.summary.o2_crossing(l).and_then(|x| x.first);
    Some(Point {
        value,
        min_o2: r.summary.min_o2,
        t_alarm1: first(a1),
        t_alarm2: first(a2),
        max_co2: r.summary.max_co2_percent,
    })
}

/// Evenly spaced (or logarithmic when the range spans > 1 decade) values.
#[must_use]
pub fn range_values(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let n = n.max(2);
    #[allow(clippy::cast_precision_loss)]
    let denom = (n - 1) as f64;
    if lo > 0.0 && hi / lo > 10.0 {
        let (a, b) = (lo.ln(), hi.ln());
        #[allow(clippy::cast_precision_loss)]
        (0..n).map(|i| (a + (b - a) * i as f64 / denom).exp()).collect()
    } else {
        #[allow(clippy::cast_precision_loss)]
        (0..n).map(|i| lo + (hi - lo) * i as f64 / denom).collect()
    }
}

/// Run `f` on every item in parallel (scoped threads), preserving order.
fn par_map<T: Sync, U: Send>(items: &[T], f: impl Fn(&T) -> U + Sync) -> Vec<U> {
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get).min(items.len().max(1));
    let chunk = items.len().div_ceil(workers).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> =
            items.chunks(chunk).map(|part| s.spawn(|| part.iter().map(&f).collect::<Vec<U>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    })
}

/// Sweep one parameter over `values`.
#[must_use]
pub fn sweep(base: &Configuration, param: Parameter, values: &[f64]) -> Vec<Point> {
    par_map(values, |&v| {
        let mut c = base.clone();
        param.set(&mut c, v);
        point(&c, v)
    })
    .into_iter()
    .flatten()
    .collect()
}

/// One bar of a tornado (one-at-a-time sensitivity) chart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TornadoBar {
    /// Parameter
    pub parameter: Parameter,
    /// Low input value
    pub low_value: f64,
    /// High input value
    pub high_value: f64,
    /// Minimum O₂ with the low value
    pub low_min_o2: f64,
    /// Minimum O₂ with the high value
    pub high_min_o2: f64,
}

impl TornadoBar {
    /// Swing of the output (percentage points of O₂).
    #[must_use]
    pub fn swing(&self) -> f64 {
        (self.high_min_o2 - self.low_min_o2).abs()
    }
}

/// One-at-a-time sensitivity of the minimum O₂ to ±`relative` variations.
#[must_use]
pub fn tornado(base: &Configuration, relative: f64) -> (f64, Vec<TornadoBar>) {
    let base_min = simulate(base).map_or(f64::NAN, |r| r.summary.min_o2);
    let params: Vec<Parameter> =
        Parameter::all().into_iter().filter(|p| p.applicable(base) && p.get(base) > 0.0).collect();
    let mut bars: Vec<TornadoBar> = par_map(&params, |&p| {
        let v = p.get(base);
        let (lo, hi) = match p {
            Parameter::DischargeCoefficient | Parameter::Efficiency => {
                ((v * (1.0 - relative)).max(0.05), (v * (1.0 + relative)).min(1.0))
            }
            _ => (v * (1.0 - relative), v * (1.0 + relative)),
        };
        let eval = |x: f64| {
            let mut c = base.clone();
            p.set(&mut c, x);
            simulate(&c).map_or(f64::NAN, |r| r.summary.min_o2)
        };
        TornadoBar { parameter: p, low_value: lo, high_value: hi, low_min_o2: eval(lo), high_min_o2: eval(hi) }
    });
    bars.sort_by(|a, b| b.swing().total_cmp(&a.swing()));
    (base_min, bars)
}

/// Design criterion for the sizing calculations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    /// O₂ must stay at or above this value (% vol)
    pub min_o2: f64,
    /// CO₂ must stay at or below this value (% vol), CO₂ releases only
    pub max_co2: f64,
}

impl Criterion {
    /// Alarm 1 for O₂ and the 3 % short-term limit for CO₂.
    #[must_use]
    pub fn from_config(c: &Configuration) -> Self {
        Self { min_o2: c.thresholds.levels().0, max_co2: 3.0 }
    }

    fn met(&self, r: &SimulationResult) -> bool {
        r.summary.min_o2 >= self.min_o2 && r.summary.max_co2_percent.is_none_or(|c| c <= self.max_co2)
    }

    /// `Some(true)` met, `Some(false)` not met, `None` met so far but the
    /// simulation stopped before the end of the event: nothing is demonstrated.
    fn verdict(&self, r: &SimulationResult) -> Option<bool> {
        if !self.met(r) {
            Some(false)
        } else if r.summary.truncated {
            None
        } else {
            Some(true)
        }
    }
}

/// Outcome of a sizing calculation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Sizing {
    /// Criterion already met with the minimum value
    AlreadyMet,
    /// Required value
    Value(f64),
    /// Not achievable within the search range
    NotAchievable,
    /// Not demonstrated: even the longest allowed computation horizon does
    /// not cover the event (a favourable but truncated result is not a result)
    Undetermined,
}

/// The base scenario with a horizon long enough to cover the event: the
/// maximum duration is extended until the simulation is no longer truncated
/// (up to the allowed maximum), so that a sizing never concludes on a cut
/// computation.
fn with_sufficient_horizon(base: &Configuration) -> Result<Configuration> {
    let mut c = base.clone();
    for _ in 0..8 {
        if !simulate(&c)?.summary.truncated || c.model.max_duration_h >= 168.0 {
            break;
        }
        c.model.max_duration_h = (c.model.max_duration_h * 4.0).min(168.0);
    }
    Ok(c)
}

/// Mechanical extraction flow (m³/h) needed to meet the criterion, with the
/// configured ventilation effectiveness.
///
/// # Errors
/// Invalid base configuration.
pub fn required_ventilation(base: &Configuration, crit: Criterion) -> Result<Sizing> {
    let ext = with_sufficient_horizon(base)?;
    // The ventilation mode of the scenario is kept: the result is a flow in
    // that mode (callers label it accordingly).
    let eval = |q: f64| -> Result<Option<bool>> {
        let mut c = ext.clone();
        c.room.mechanical_enabled = q > 0.0;
        c.room.mechanical_flow_m3h = q;
        Ok(crit.verdict(&simulate(&c)?))
    };
    match eval(0.0)? {
        Some(true) => return Ok(Sizing::AlreadyMet),
        None => return Ok(Sizing::Undetermined),
        Some(false) => {}
    }
    let q_max = (base.room.volume_m3 * 200.0).clamp(1e5, 10_000_000.0);
    match eval(q_max)? {
        Some(false) => return Ok(Sizing::NotAchievable),
        None => return Ok(Sizing::Undetermined),
        Some(true) => {}
    }
    let (mut lo, mut hi) = (0.0, q_max);
    // Geometric bisection: relative precision ~1 %
    let mut probe = (base.room.volume_m3).max(10.0);
    while probe < hi {
        match eval(probe)? {
            Some(false) => {
                lo = probe;
                probe *= 4.0;
            }
            Some(true) => break,
            None => return Ok(Sizing::Undetermined),
        }
    }
    hi = hi.min(probe);
    for _ in 0..40 {
        if hi - lo <= 0.01 * hi.max(1.0) {
            break;
        }
        let mid = if lo > 0.0 { (lo * hi).sqrt() } else { hi / 2.0 };
        match eval(mid)? {
            Some(true) => hi = mid,
            Some(false) => lo = mid,
            None => return Ok(Sizing::Undetermined),
        }
    }
    Ok(Sizing::Value(hi))
}

/// Largest leak diameter (mm) meeting the criterion — useful to size a
/// restriction orifice / flow limiter.
///
/// # Errors
/// Invalid base configuration.
pub fn max_leak_diameter(base: &Configuration, crit: Criterion) -> Result<Sizing> {
    if !base.release.kind.uses_orifice() {
        return Ok(Sizing::NotAchievable);
    }
    let ext = with_sufficient_horizon(base)?;
    let eval = |d: f64| -> Result<Option<bool>> {
        let mut c = ext.clone();
        c.release.diameter_mm = d;
        Ok(crit.verdict(&simulate(&c)?))
    };
    let (d_min, d_max) = (0.05, 50.0);
    match eval(d_max)? {
        Some(true) => return Ok(Sizing::AlreadyMet),
        None => return Ok(Sizing::Undetermined),
        Some(false) => {}
    }
    match eval(d_min)? {
        Some(false) => return Ok(Sizing::NotAchievable),
        None => return Ok(Sizing::Undetermined),
        Some(true) => {}
    }
    let (mut lo, mut hi) = (d_min, d_max);
    for _ in 0..40 {
        if hi / lo < 1.01 {
            break;
        }
        let mid = (lo * hi).sqrt();
        match eval(mid)? {
            Some(true) => lo = mid,
            Some(false) => hi = mid,
            None => return Ok(Sizing::Undetermined),
        }
    }
    Ok(Sizing::Value(lo))
}

/// A labelled scenario variant for comparison plots.
#[derive(Debug, Clone)]
pub struct Variant {
    /// Label
    pub label: String,
    /// Parameter value (1 = ventilation running, 0 = failed for the failure study)
    pub value: f64,
    /// Result
    pub result: SimulationResult,
}

/// Simulate `base` with `param` set to each value (curves for overlay plots).
#[must_use]
pub fn compare(base: &Configuration, param: Parameter, values: &[f64], lang: Language) -> Vec<Variant> {
    par_map(values, |&v| {
        let mut c = base.clone();
        param.set(&mut c, v);
        simulate(&c).ok().map(|result| Variant {
            value: v,
            label: format!(
                "{} = {}",
                param.name(lang),
                crate::i18n::fmt_unit(v, if v < 10.0 { 1 } else { 0 }, param.unit(), lang)
            ),
            result,
        })
    })
    .into_iter()
    .flatten()
    .collect()
}

/// Before / after study: the current installation and the planned one
/// (`base.future`), each with the mechanical ventilation running (value 0 and
/// 2) and failed (value 1 and 3). Only the change differs between "current"
/// and "future"; the leak hypothesis is the one of the scenario.
///
/// # Errors
/// The future configuration is validated like the current one; an invalid
/// value (too many containers, pressure or diameter out of range) is reported
/// instead of silently dropping the future cases.
pub fn before_after(base: &Configuration, lang: Language) -> Result<Vec<Variant>> {
    let future = base.future.apply(base);
    future.validate()?;
    let on = |c: &Configuration| {
        let mut c = c.clone();
        c.room.mechanical_enabled = c.room.mechanical_flow_m3h > 0.0 && c.room.mechanical_enabled;
        c
    };
    // Total failure from the start: no mechanical ventilation and no backup
    // fan; the isolation of the leak, if planned, is kept.
    let off = |c: &Configuration| {
        let mut c = c.without_ventilation_events();
        c.room.mechanical_enabled = false;
        c
    };
    let cases = [
        (on(base), lang.pick("Actuel – ventilation en service", "Current – ventilation running"), 0.0),
        (off(base), lang.pick("Actuel – panne de ventilation", "Current – ventilation failure"), 1.0),
        (on(&future), lang.pick("Futur – ventilation en service", "Future – ventilation running"), 2.0),
        (off(&future), lang.pick("Futur – panne de ventilation", "Future – ventilation failure"), 3.0),
    ];
    par_map(&cases, |(c, label, value)| {
        simulate(c).map(|result| Variant { label: (*label).to_string(), value: *value, result })
    })
    .into_iter()
    .collect()
}

/// Same scenario with the mechanical ventilation running and failed.
#[must_use]
pub fn ventilation_failure(base: &Configuration, lang: Language) -> Vec<Variant> {
    let mut on = base.clone();
    on.room.mechanical_enabled = true;
    // Total failure from the start: no mechanical ventilation and no backup fan
    let mut off = base.without_ventilation_events();
    off.room.mechanical_enabled = false;
    let cases = [
        (on, lang.pick("Ventilation en service", "Ventilation running"), 1.0),
        (off, lang.pick("Panne de ventilation", "Ventilation failure"), 0.0),
    ];
    par_map(&cases, |(c, label, value)| {
        simulate(c).ok().map(|result| Variant { label: (*label).to_string(), value: *value, result })
    })
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{StratificationMode, VentilationMode};
    use crate::gas::GasType;

    fn base() -> Configuration {
        let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        c.model.stratification = StratificationMode::Off;
        c.room.volume_m3 = 40.0;
        c.release.diameter_mm = 2.0;
        c
    }

    #[test]
    fn sizing_never_concludes_on_a_truncated_horizon() {
        // Limit case: 1 m³/h for 24 h in 10 m³ without ventilation, computed
        // over 6 min only: a favourable but cut computation must not become
        // "already met". The horizon is extended and the result equals the one
        // obtained with a sufficient horizon.
        let mut c = base();
        c.release.kind = ReleaseKind::ConstantFlow;
        c.release.constant_flow_m3h = 1.0;
        c.release.constant_duration_min = 1440.0;
        c.room.volume_m3 = 10.0;
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.room.ventilation_efficiency = 1.0;
        c.model.max_duration_h = 0.1;
        let crit = Criterion::from_config(&c);
        let short = required_ventilation(&c, crit).unwrap();
        assert!(!matches!(short, Sizing::AlreadyMet), "{short:?}");
        c.model.max_duration_h = 30.0;
        let long = required_ventilation(&c, crit).unwrap();
        match (short, long) {
            (Sizing::Value(a), Sizing::Value(b)) => assert!((a - b).abs() <= 0.02 * b, "{a} vs {b}"),
            other => panic!("unexpected {other:?}"),
        }
        // The diameter search has the same guard
        c.release.kind = ReleaseKind::Leak;
        c.model.max_duration_h = 0.1;
        assert!(
            !matches!(max_leak_diameter(&c, crit).unwrap(), Sizing::AlreadyMet)
                || !simulate(&c).unwrap().summary.truncated
        );
    }

    #[test]
    fn sizing_flow_is_in_the_scenario_ventilation_mode() {
        // Limit case: with a supply scenario, the flow found is a supply flow;
        // applied as an extraction it would miss the criterion.
        let mut c = base();
        c.release.kind = ReleaseKind::ConstantFlow;
        c.release.constant_flow_m3h = 100.0;
        c.release.constant_duration_min = 60.0;
        c.room.volume_m3 = 60.0;
        c.room.mechanical_enabled = false;
        c.room.natural_ach = 0.0;
        c.room.ventilation_efficiency = 1.0;
        c.room.mechanical_mode = VentilationMode::Supply;
        let crit = Criterion::from_config(&c);
        let Sizing::Value(q) = required_ventilation(&c, crit).unwrap() else { panic!("a flow is expected") };
        c.room.mechanical_enabled = true;
        c.room.mechanical_flow_m3h = q;
        assert!(simulate(&c).unwrap().summary.min_o2 >= crit.min_o2 - 1e-9);
        c.room.mechanical_mode = VentilationMode::Extraction;
        assert!(simulate(&c).unwrap().summary.min_o2 < crit.min_o2, "the mode matters: the label must say which one");
        // Sizing in extraction mode gives a larger flow that meets the criterion in that mode
        let Sizing::Value(q_ext) = required_ventilation(&c, crit).unwrap() else { panic!() };
        assert!(q_ext > q);
        c.room.mechanical_flow_m3h = q_ext;
        assert!(simulate(&c).unwrap().summary.min_o2 >= crit.min_o2 - 1e-9);
    }

    #[test]
    fn sweep_is_monotonic_in_diameter() {
        let pts = sweep(&base(), Parameter::LeakDiameter, &[0.5, 1.0, 2.0, 4.0]);
        assert_eq!(pts.len(), 4);
        assert!(pts.windows(2).all(|w| w[1].min_o2 <= w[0].min_o2 + 1e-9));
    }

    #[test]
    fn required_ventilation_meets_criterion() {
        let c = base();
        let crit = Criterion { min_o2: 19.0, max_co2: 3.0 };
        match required_ventilation(&c, crit).unwrap() {
            Sizing::Value(q) => {
                let mut ok = c.clone();
                ok.room.mechanical_flow_m3h = q;
                assert!(simulate(&ok).unwrap().summary.min_o2 >= 19.0);
                let mut ko = c.clone();
                ko.room.mechanical_flow_m3h = q * 0.9;
                assert!(simulate(&ko).unwrap().summary.min_o2 < 19.0);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn max_diameter_meets_criterion() {
        let mut c = base();
        c.room.mechanical_flow_m3h = 300.0;
        let crit = Criterion { min_o2: 19.0, max_co2: 3.0 };
        if let Sizing::Value(d) = max_leak_diameter(&c, crit).unwrap() {
            let mut ok = c.clone();
            ok.release.diameter_mm = d;
            assert!(simulate(&ok).unwrap().summary.min_o2 >= 19.0);
        } else {
            panic!("expected a value");
        }
    }

    #[test]
    fn tornado_ranks_by_swing() {
        let (base_min, bars) = tornado(&base(), 0.3);
        assert!(base_min.is_finite());
        assert!(!bars.is_empty());
        assert!(bars.windows(2).all(|w| w[0].swing() >= w[1].swing()));
    }

    #[test]
    fn log_range() {
        let v = range_values(0.1, 100.0, 4);
        assert!((v[1] - 1.0).abs() < 1e-9 && (v[2] - 10.0).abs() < 1e-9);
    }
}
