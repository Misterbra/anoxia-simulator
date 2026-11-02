//! Gas source model: orifice flow and container blowdown.
//!
//! ## Orifice flow (isentropic nozzle, real-gas upstream density)
//!
//! With upstream stagnation pressure `P0`, density `ρ0`, downstream pressure
//! `Pa` and `r = Pa/P0`:
//!
//! - choked (`r ≤ r* = (2/(γ+1))^(γ/(γ−1))`):
//!   `ṁ = Cd·A·√(γ·ρ0·P0)·(2/(γ+1))^((γ+1)/(2(γ−1)))`
//! - subcritical:
//!   `ṁ = Cd·A·√(2·ρ0·P0·γ/(γ−1)·(r^(2/γ) − r^((γ+1)/γ)))`
//!
//! For an ideal gas `ρ0·P0 = P0²·M/(R·T0)`, which gives the textbook forms
//! (Perry's 8th ed. §6; TNO Yellow Book CPR 14E §2.5). Both expressions meet
//! continuously at `r*`.
//!
//! ## Blowdown
//!
//! The container is treated as **isothermal** at the source temperature
//! (heat exchange with the walls). For fast releases this slightly
//! overestimates the flow (adiabatic cooling would lower the pressure), which
//! is conservative for oxygen depletion.
//!
//! ## Liquefied CO₂
//!
//! While liquid remains (`m/V > ρ_v,sat`) the pressure is the saturation
//! pressure. Vapour withdrawal uses the gas orifice equation with the
//! saturated-vapour density; liquid withdrawal (dip tube / inverted cylinder)
//! uses the non-flashing liquid orifice equation `ṁ = Cd·A·√(2·ρ_l·ΔP)`,
//! an upper bound of the real flashing two-phase flow.

use crate::config::{Blowdown, Co2Withdrawal, Configuration};
use crate::constants::{celsius_to_kelvin, R};
use crate::error::{AnoxiaError, Result};
use crate::gas::{gauge_bar_to_abs_pa, saturation_densities_co2, saturation_pressure_co2, GasType};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Flow regime through the orifice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowRegime {
    /// Sonic (choked) gas flow
    Choked,
    /// Subsonic gas flow
    Subcritical,
    /// Liquid flow (liquefied gas withdrawn from the liquid phase)
    Liquid,
    /// No flow (container at ambient pressure)
    None,
}

/// Critical pressure ratio `Pa/P0` below which the flow is choked.
#[must_use]
pub fn critical_pressure_ratio(gamma: f64) -> f64 {
    (2.0 / (gamma + 1.0)).powf(gamma / (gamma - 1.0))
}

/// Gas mass flow (kg/s) through an orifice of area `area` (m²).
///
/// `p0` and `rho0` are the upstream pressure (Pa) and density (kg/m³),
/// `pa` the downstream pressure (Pa).
#[must_use]
pub fn gas_orifice_flow(cd: f64, area: f64, gamma: f64, p0: f64, rho0: f64, pa: f64) -> (f64, FlowRegime) {
    if p0 <= pa || rho0 <= 0.0 {
        return (0.0, FlowRegime::None);
    }
    let r = pa / p0;
    if r <= critical_pressure_ratio(gamma) {
        let k = (2.0 / (gamma + 1.0)).powf((gamma + 1.0) / (2.0 * (gamma - 1.0)));
        (cd * area * (gamma * rho0 * p0).sqrt() * k, FlowRegime::Choked)
    } else {
        let term = r.powf(2.0 / gamma) - r.powf((gamma + 1.0) / gamma);
        let flow = cd * area * (2.0 * rho0 * p0 * gamma / (gamma - 1.0) * term.max(0.0)).sqrt();
        (flow, FlowRegime::Subcritical)
    }
}

/// Liquid mass flow (kg/s) through an orifice (Bernoulli, non-flashing).
#[must_use]
pub fn liquid_orifice_flow(cd: f64, area: f64, rho_l: f64, dp: f64) -> f64 {
    if dp <= 0.0 {
        0.0
    } else {
        cd * area * (2.0 * rho_l * dp).sqrt()
    }
}

/// Container + orifice model.
#[derive(Debug, Clone)]
pub struct Source {
    gas: GasType,
    gamma: f64,
    /// Internal volume (m³)
    volume: f64,
    /// Container temperature (K)
    temperature: f64,
    /// Ambient (room) pressure (Pa)
    p_ambient: f64,
    /// Cd·A (m²)
    cd_area: f64,
    withdrawal: Co2Withdrawal,
    /// Isentropic (adiabatic) expansion of the gas left in the container
    adiabatic: bool,
    /// Initial inventory (kg)
    pub initial_mass: f64,
    /// Mass left once the container is at ambient pressure and temperature (kg)
    pub residual_mass: f64,
    /// Mass at which the outflow stops in this model (kg); larger than
    /// `residual_mass` for an adiabatic blowdown (cold gas left behind)
    pub end_mass: f64,
    /// Initial absolute pressure (Pa)
    pub initial_pressure: f64,
    /// CO₂ only: container liquid-full at this temperature
    pub liquid_full: bool,
}

impl Source {
    /// Build the source from a configuration.
    ///
    /// # Errors
    /// Physically impossible states (e.g. CO₂ above its critical temperature,
    /// container pressure below ambient).
    pub fn from_config(config: &Configuration) -> Result<Self> {
        let gas = config.gas;
        let props = gas.properties();
        let volume = config.source.total_volume_m3();
        let temperature = celsius_to_kelvin(config.source.temperature_c);
        let p_ambient = config.room.pressure_hpa * 100.0;
        let d = config.release.diameter_mm / 1000.0;
        let cd_area = if config.release.kind.uses_orifice() {
            config.release.discharge_coefficient * PI * d * d / 4.0
        } else {
            0.0
        };

        let mut liquid_full = false;
        let (initial_mass, initial_pressure) = if gas.is_liquefied() {
            let psat = saturation_pressure_co2(temperature).ok_or_else(|| {
                AnoxiaError::InvalidConfiguration("CO₂ container above 31 °C (supercritical): not supported".into())
            })?;
            let (rho_l, _) = saturation_densities_co2(temperature).unwrap_or((773.0, 194.0));
            let mass = config.source.co2_fill_ratio * 1000.0 * volume;
            if mass / volume >= rho_l {
                liquid_full = true;
            }
            (mass, psat)
        } else {
            // Gauge pressure read on the regulator: relative to the room's atmosphere
            let p0 = gauge_bar_to_abs_pa(config.source.pressure_bar_g, p_ambient);
            (gas.density(temperature, p0) * volume, p0)
        };

        if initial_pressure <= p_ambient {
            return Err(AnoxiaError::InvalidConfiguration(
                "container pressure must exceed the ambient pressure".into(),
            ));
        }

        let residual_mass = gas.density(temperature, p_ambient) * volume;
        let mut source = Self {
            gas,
            gamma: props.gamma,
            volume,
            temperature,
            p_ambient,
            cd_area,
            withdrawal: config.source.co2_withdrawal,
            adiabatic: config.model.blowdown == Blowdown::Adiabatic,
            initial_mass,
            residual_mass,
            end_mass: residual_mass,
            initial_pressure,
            liquid_full,
        };
        if source.adiabatic {
            // Mass at which the cold gas left in the container reaches ambient pressure.
            let (mut lo, mut hi) = (residual_mass, initial_mass);
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                if source.pressure(mid) > p_ambient {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            source.end_mass = hi;
        }
        Ok(source)
    }

    /// Releasable inventory (kg): initial mass minus what stays at ambient pressure.
    #[must_use]
    pub fn releasable_mass(&self) -> f64 {
        (self.initial_mass - self.residual_mass).max(0.0)
    }

    /// `true` while liquid CO₂ remains in the container for mass `m`.
    fn two_phase(&self, m: f64) -> bool {
        self.gas.is_liquefied()
            && saturation_densities_co2(self.temperature).is_some_and(|(_, rho_v)| m / self.volume > rho_v)
    }

    /// Gas temperature (K) in the container for an inventory `m` (kg):
    /// constant (isothermal) or isentropic `T = T0·(ρ/ρ0)^(γ−1)` (adiabatic).
    #[must_use]
    pub fn gas_temperature(&self, m: f64) -> f64 {
        if self.adiabatic && !self.gas.is_liquefied() && m > 0.0 {
            // The gas would condense below its boiling point: stop the expansion there.
            (self.temperature * (m / self.initial_mass).powf(self.gamma - 1.0)).max(self.gas.boiling_point() + 5.0)
        } else {
            self.temperature
        }
    }

    /// Absolute container pressure (Pa) for an inventory `m` (kg).
    #[must_use]
    pub fn pressure(&self, m: f64) -> f64 {
        if self.two_phase(m) {
            saturation_pressure_co2(self.temperature).unwrap_or(self.initial_pressure)
        } else {
            self.gas.pressure_from_density(self.gas_temperature(m), (m / self.volume).max(0.0))
        }
    }

    /// Mass flow (kg/s) and regime for an inventory `m` (kg).
    #[must_use]
    pub fn mass_flow(&self, m: f64) -> (f64, FlowRegime) {
        if self.cd_area <= 0.0 || m <= self.end_mass {
            return (0.0, FlowRegime::None);
        }
        let p0 = self.pressure(m);
        if self.two_phase(m) {
            let (rho_l, rho_v) = saturation_densities_co2(self.temperature).unwrap_or((773.0, 194.0));
            // A liquid-full cylinder has no vapour space: it discharges liquid
            // even when upright, until a vapour space reappears.
            let liquid = self.withdrawal == Co2Withdrawal::Liquid || m / self.volume >= rho_l;
            return match if liquid { Co2Withdrawal::Liquid } else { Co2Withdrawal::Vapour } {
                Co2Withdrawal::Liquid => {
                    (liquid_orifice_flow(1.0, self.cd_area, rho_l, p0 - self.p_ambient), FlowRegime::Liquid)
                }
                Co2Withdrawal::Vapour => gas_orifice_flow(1.0, self.cd_area, self.gamma, p0, rho_v, self.p_ambient),
            };
        }
        gas_orifice_flow(1.0, self.cd_area, self.gamma, p0, m / self.volume, self.p_ambient)
    }

    /// Container temperature (K).
    #[must_use]
    pub fn temperature(&self) -> f64 {
        self.temperature
    }

    /// Ideal-gas volume (m³) of `mass` kg at room temperature `t_room` (K).
    #[must_use]
    pub fn volume_at(&self, mass: f64, t_room: f64) -> f64 {
        mass * R * t_room / (self.gas.properties().molar_mass * self.p_ambient)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::P_ATM;
    use approx::assert_relative_eq;

    fn area(d_mm: f64) -> f64 {
        PI * (d_mm / 1000.0).powi(2) / 4.0
    }

    #[test]
    fn critical_ratio_nitrogen() {
        assert_relative_eq!(1.0 / critical_pressure_ratio(1.4), 1.893, epsilon = 0.001);
    }

    /// Hand calculation: N₂, 8 mm, Cd = 0.72, 200 bar(a), 293.15 K, ideal gas
    /// ṁ = Cd·A·P0·√(γM/RT)·0.5787 = 1.68 kg/s (reference value).
    #[test]
    fn choked_flow_matches_hand_calculation() {
        let g = GasType::Nitrogen;
        let p0 = 200e5;
        let rho0 = g.ideal_density(293.15, p0);
        let (q, regime) = gas_orifice_flow(0.72, area(8.0), 1.4, p0, rho0, P_ATM);
        assert_eq!(regime, FlowRegime::Choked);
        assert_relative_eq!(q, 1.68, max_relative = 0.01);
    }

    #[test]
    fn flow_is_continuous_at_critical_ratio() {
        let gamma = 1.4;
        let pa = P_ATM;
        let p_crit = pa / critical_pressure_ratio(gamma);
        let rho = |p: f64| GasType::Nitrogen.ideal_density(293.15, p);
        let (below, r1) = gas_orifice_flow(0.62, area(5.0), gamma, p_crit * 0.9999, rho(p_crit * 0.9999), pa);
        let (above, r2) = gas_orifice_flow(0.62, area(5.0), gamma, p_crit * 1.0001, rho(p_crit * 1.0001), pa);
        assert_eq!(r1, FlowRegime::Subcritical);
        assert_eq!(r2, FlowRegime::Choked);
        assert_relative_eq!(below, above, max_relative = 0.001);
    }

    #[test]
    fn subcritical_flow_vanishes_at_ambient() {
        let (q, regime) = gas_orifice_flow(0.62, area(5.0), 1.4, P_ATM, 1.2, P_ATM);
        assert_eq!(q, 0.0);
        assert_eq!(regime, FlowRegime::None);
    }

    #[test]
    fn helium_flows_slower_in_mass_than_argon() {
        // Choked mass flow ∝ √M at equal P, T, γ.
        let p0 = 200e5;
        let he = gas_orifice_flow(0.62, area(4.0), 1.667, p0, GasType::Helium.ideal_density(293.15, p0), P_ATM).0;
        let ar = gas_orifice_flow(0.62, area(4.0), 1.667, p0, GasType::Argon.ideal_density(293.15, p0), P_ATM).0;
        assert_relative_eq!(ar / he, (39.948_f64 / 4.0026).sqrt(), max_relative = 1e-6);
    }

    #[test]
    fn b50_nitrogen_inventory() {
        // 50 L at 200 bar(g), 20 °C: ≈ 11.2 kg ≈ 9.6 m³ of gas (real gas, Z ≈ 1.04)
        let c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        let s = Source::from_config(&c).unwrap();
        assert_relative_eq!(s.initial_mass, 11.25, max_relative = 0.03);
        assert_relative_eq!(s.volume_at(s.releasable_mass(), 293.15), 9.6, max_relative = 0.04);
    }

    #[test]
    fn co2_b50_inventory_and_pressure() {
        let mut c = Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() };
        c.source.co2_fill_ratio = 0.75;
        let s = Source::from_config(&c).unwrap();
        assert_relative_eq!(s.initial_mass, 37.5, max_relative = 1e-9);
        assert_relative_eq!(s.pressure(30.0), 57.29e5, max_relative = 0.01);
        // ≈ 20 m³ of gas at 20 °C
        assert_relative_eq!(s.volume_at(s.releasable_mass(), 293.15), 20.4, max_relative = 0.03);
    }

    #[test]
    fn co2_liquid_withdrawal_is_much_faster() {
        let mut c = Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() };
        let vap = Source::from_config(&c).unwrap().mass_flow(30.0).0;
        c.source.co2_withdrawal = Co2Withdrawal::Liquid;
        let (liq, regime) = Source::from_config(&c).unwrap().mass_flow(30.0);
        assert_eq!(regime, FlowRegime::Liquid);
        assert!(liq > 2.0 * vap);
    }

    #[test]
    fn pressure_decreases_with_inventory() {
        let s = Source::from_config(&Configuration::default()).unwrap();
        let p_full = s.pressure(s.initial_mass);
        let p_half = s.pressure(s.initial_mass / 2.0);
        assert_relative_eq!(p_full, 201.0e5 + 1325.0, max_relative = 1e-3);
        assert!(p_half < 0.55 * p_full && p_half > 0.45 * p_full);
        assert_eq!(s.mass_flow(s.residual_mass).0, 0.0);
    }
}
