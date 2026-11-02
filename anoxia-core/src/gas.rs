//! Gas properties and equations of state for the supported inert gases.
//!
//! Data sources:
//! - Molar masses: IUPAC 2016 standard atomic weights.
//! - Critical constants and acentric factors: NIST Chemistry WebBook
//!   (Lemmon et al.), Poling, Prausnitz & O'Connell, *The Properties of
//!   Gases and Liquids*, 5th ed.
//! - CO₂ saturation curve: Span & Wagner (1996), *J. Phys. Chem. Ref. Data*
//!   25, 1509 — ancillary equations (3.13)–(3.15).
//!
//! The compressibility factor `Z` is interpolated in a table generated from
//! the NIST Chemistry WebBook (REFPROP reference equations, −40 to +70 °C,
//! 1 to 750 bar; CO₂ vapour up to saturation). Outside the table it falls back
//! to Peng–Robinson (Ar, N₂, CO₂) or a second-virial expansion (He, Ne).

use crate::constants::R;
use crate::i18n::Language;
use serde::{Deserialize, Serialize};

/// Inert gases supported by the simulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GasType {
    /// Argon (Ar)
    Argon,
    /// Nitrogen (N₂)
    Nitrogen,
    /// Carbon dioxide (CO₂) — stored liquefied, toxic in its own right
    CarbonDioxide,
    /// Helium (He)
    Helium,
    /// Neon (Ne)
    Neon,
}

impl GasType {
    /// All supported gases, in display order.
    #[must_use]
    pub fn all() -> [Self; 5] {
        [Self::Nitrogen, Self::Argon, Self::CarbonDioxide, Self::Helium, Self::Neon]
    }

    /// Localised display name.
    #[must_use]
    pub fn name(self, lang: Language) -> &'static str {
        match (self, lang) {
            (Self::Argon, _) => "Argon (Ar)",
            (Self::Nitrogen, Language::French) => "Azote (N₂)",
            (Self::Nitrogen, Language::English) => "Nitrogen (N₂)",
            (Self::CarbonDioxide, Language::French) => "Dioxyde de carbone (CO₂)",
            (Self::CarbonDioxide, Language::English) => "Carbon dioxide (CO₂)",
            (Self::Helium, Language::French) => "Hélium (He)",
            (Self::Helium, Language::English) => "Helium (He)",
            (Self::Neon, Language::French) => "Néon (Ne)",
            (Self::Neon, Language::English) => "Neon (Ne)",
        }
    }

    /// Chemical formula.
    #[must_use]
    pub fn formula(self) -> &'static str {
        match self {
            Self::Argon => "Ar",
            Self::Nitrogen => "N₂",
            Self::CarbonDioxide => "CO₂",
            Self::Helium => "He",
            Self::Neon => "Ne",
        }
    }

    /// Physical properties of the gas.
    #[must_use]
    pub fn properties(self) -> GasProperties {
        match self {
            Self::Argon => GasProperties {
                molar_mass: 39.948e-3,
                gamma: 1.667,
                tc: 150.687,
                pc: 4.863e6,
                omega: -0.002,
                virial_b: 0.0,
            },
            Self::Nitrogen => GasProperties {
                molar_mass: 28.0134e-3,
                gamma: 1.3996,
                tc: 126.192,
                pc: 3.3958e6,
                omega: 0.0372,
                virial_b: 0.0,
            },
            Self::CarbonDioxide => GasProperties {
                molar_mass: 44.0095e-3,
                gamma: 1.2908,
                tc: 304.128,
                pc: 7.3773e6,
                omega: 0.2239,
                virial_b: 0.0,
            },
            // Second virial coefficients at ~293 K (Dymond & Smith; NIST):
            // He ≈ 11.8 cm³/mol, Ne ≈ 11.3 cm³/mol.
            Self::Helium => GasProperties {
                molar_mass: 4.002_602e-3,
                gamma: 1.667,
                tc: 5.1953,
                pc: 0.227_46e6,
                omega: -0.3836,
                virial_b: 11.8e-6,
            },
            Self::Neon => GasProperties {
                molar_mass: 20.1797e-3,
                gamma: 1.667,
                tc: 44.4918,
                pc: 2.6786e6,
                omega: -0.0387,
                virial_b: 11.3e-6,
            },
        }
    }

    /// Normal boiling point (K) at 1 atm (NIST).
    #[must_use]
    pub fn boiling_point(self) -> f64 {
        match self {
            Self::Nitrogen => 77.36,
            Self::Argon => 87.30,
            Self::CarbonDioxide => 194.7,
            Self::Helium => 4.22,
            Self::Neon => 27.10,
        }
    }

    /// `true` if the gas is stored as a liquefied gas at ambient temperature.
    #[must_use]
    pub fn is_liquefied(self) -> bool {
        matches!(self, Self::CarbonDioxide)
    }

    /// `true` if the gas is toxic in its own right (independently of O₂ depletion).
    #[must_use]
    pub fn is_toxic(self) -> bool {
        matches!(self, Self::CarbonDioxide)
    }

    /// Density relative to air at the same temperature and pressure (ideal gas).
    #[must_use]
    pub fn relative_density(self) -> f64 {
        self.properties().molar_mass / crate::constants::M_AIR
    }

    /// Buoyancy class of the gas.
    #[must_use]
    pub fn buoyancy(self) -> Buoyancy {
        let d = self.relative_density();
        if d > 1.1 {
            Buoyancy::Heavy
        } else if d < 0.9 {
            Buoyancy::Light
        } else {
            Buoyancy::Neutral
        }
    }

    /// Typical full-cylinder gauge pressure (bar) used as a default.
    #[must_use]
    pub fn typical_pressure_bar(self) -> f64 {
        match self {
            Self::CarbonDioxide => saturation_pressure_co2(293.15).map_or(56.3, |p| p / 1e5 - 1.013),
            _ => 200.0,
        }
    }

    /// Gas density at temperature `t` (K) and pressure `p` (Pa), ideal gas (kg/m³).
    #[must_use]
    pub fn ideal_density(self, t: f64, p: f64) -> f64 {
        p * self.properties().molar_mass / (R * t)
    }

    /// Compressibility factor Z(T, P) for the gas phase.
    #[must_use]
    pub fn compressibility(self, t: f64, p: f64) -> f64 {
        if let Some(z) = self.compressibility_nist(t, p) {
            return z;
        }
        // Outside the table: equation of state shifted to join the table at its
        // nearest edge, so that Z(T, P) stays continuous (needed by the solvers).
        let temps = &crate::nist_data::TEMPS;
        let p_max = match self {
            Self::CarbonDioxide => crate::nist_data::CO2_PRESS[crate::nist_data::CO2_PRESS.len() - 1],
            _ => crate::nist_data::PRESS[crate::nist_data::PRESS.len() - 1],
        } * 1e5;
        let te = t.clamp(temps[0], temps[temps.len() - 1]);
        let pe = p.min(p_max);
        match self.compressibility_nist(te, pe) {
            Some(z_edge) => self.compressibility_eos(t, p) + (z_edge - self.compressibility_eos(te, pe)),
            None => self.compressibility_eos(t, p),
        }
    }

    /// Z from the cubic / virial equation of state (fallback outside the NIST grid).
    #[must_use]
    pub fn compressibility_eos(self, t: f64, p: f64) -> f64 {
        let props = self.properties();
        if props.virial_b > 0.0 {
            1.0 + props.virial_b * p / (R * t)
        } else {
            peng_robinson_z(&props, t, p)
        }
    }

    /// Z interpolated (bilinear in T and P) in the NIST REFPROP table, if the
    /// point lies inside the tabulated gas-phase domain.
    #[must_use]
    pub fn compressibility_nist(self, t: f64, p: f64) -> Option<f64> {
        use crate::nist_data as n;
        let p_bar = p / 1e5;
        match self {
            Self::Nitrogen => interp_table(&n::Z_N2, &n::PRESS, t, p_bar),
            Self::Argon => interp_table(&n::Z_AR, &n::PRESS, t, p_bar),
            Self::Helium => interp_table(&n::Z_HE, &n::PRESS, t, p_bar),
            Self::Neon => interp_table(&n::Z_NE, &n::PRESS, t, p_bar),
            Self::CarbonDioxide => co2_vapour_z(t, p_bar).or_else(|| interp_table(&n::Z_CO2, &n::CO2_PRESS, t, p_bar)),
        }
    }

    /// Real-gas density of the gas phase (kg/m³).
    #[must_use]
    pub fn density(self, t: f64, p: f64) -> f64 {
        self.ideal_density(t, p) / self.compressibility(t, p)
    }

    /// Pressure (Pa) of a gas-phase mass `m` (kg) in volume `v` (m³) at
    /// temperature `t` (K). Inverts the equation of state by Newton/secant.
    #[must_use]
    pub fn pressure_from_density(self, t: f64, rho: f64) -> f64 {
        let m = self.properties().molar_mass;
        let ideal = rho * R * t / m;
        if ideal <= 0.0 {
            return 0.0;
        }
        // Solve g(P) = P − ρRT·Z(P)/M = 0 by the secant method from the ideal-gas value.
        let g = |p: f64| p - ideal * self.compressibility(t, p);
        let mut p0 = ideal;
        let mut p1 = ideal * self.compressibility(t, ideal);
        let mut g0 = g(p0);
        for _ in 0..30 {
            let g1 = g(p1);
            if g1.abs() < 1e-9 * p1.max(1.0) || (g1 - g0).abs() < f64::MIN_POSITIVE {
                return p1;
            }
            let p2 = (p1 - g1 * (p1 - p0) / (g1 - g0)).max(0.1 * ideal);
            p0 = p1;
            g0 = g1;
            p1 = p2;
        }
        p1
    }
}

/// Vertical behaviour of a released gas relative to air.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Buoyancy {
    /// Heavier than air: accumulates in low points (pits, trenches, floor).
    Heavy,
    /// Close to air density: mixes relatively uniformly.
    Neutral,
    /// Lighter than air: accumulates at the ceiling.
    Light,
}

impl Buoyancy {
    /// Localised description.
    #[must_use]
    pub fn description(self, lang: Language) -> &'static str {
        match (self, lang) {
            (Self::Heavy, Language::French) => "Gaz lourd : s'accumule en partie basse (sol, fosses, caniveaux)",
            (Self::Heavy, Language::English) => "Heavy gas: accumulates in low points (floor, pits, trenches)",
            (Self::Neutral, Language::French) => "Densité proche de l'air : mélange relativement homogène",
            (Self::Neutral, Language::English) => "Density close to air: relatively uniform mixing",
            (Self::Light, Language::French) => "Gaz léger : s'accumule en partie haute (plafond, mezzanines)",
            (Self::Light, Language::English) => "Light gas: accumulates at high points (ceiling, mezzanines)",
        }
    }
}

/// Physical constants of a gas (SI units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GasProperties {
    /// Molar mass (kg/mol)
    pub molar_mass: f64,
    /// Ideal-gas heat capacity ratio Cp/Cv at ~20 °C
    pub gamma: f64,
    /// Critical temperature (K)
    pub tc: f64,
    /// Critical pressure (Pa)
    pub pc: f64,
    /// Acentric factor
    pub omega: f64,
    /// Second virial coefficient (m³/mol), used instead of Peng–Robinson when > 0
    pub virial_b: f64,
}

/// CO₂ vapour Z below the critical temperature, interpolated in reduced
/// pressure P/Psat(T) between NIST rows, each row being closed by its
/// saturated-vapour point (Span & Wagner). Avoids falling back to the cubic
/// equation of state in the vapour tail left once the liquid is exhausted.
fn co2_vapour_z(t: f64, p_bar: f64) -> Option<f64> {
    use crate::nist_data as n;
    use std::sync::OnceLock;
    // Per sub-critical row: (Psat in bar, saturated-vapour Z).
    static ROWS: OnceLock<Vec<Option<(f64, f64)>>> = OnceLock::new();
    let m = GasType::CarbonDioxide.properties().molar_mass;
    let z_sat = |ti: f64| -> Option<(f64, f64)> {
        let psat = saturation_pressure_co2(ti)?;
        let (_, rho_v) = saturation_densities_co2(ti)?;
        Some((psat / 1e5, psat * m / (rho_v * R * ti)))
    };
    let rows = ROWS.get_or_init(|| n::TEMPS.iter().map(|&ti| z_sat(ti)).collect());
    let temps = &n::TEMPS;
    let last_sub = rows.iter().rposition(Option::is_some)?;
    if t < temps[0] || t > temps[last_sub] {
        return None;
    }
    let (psat_q, zsat_q) = z_sat(t)?;
    let r = p_bar / psat_q;
    if !(0.0..=1.0).contains(&r) {
        return None;
    }
    let row_z = |i: usize, r: f64| -> Option<f64> {
        let (psat, zsat) = rows[i]?;
        // Points (P/Psat, Z): (0, 1), grid points below Psat, (1, Zsat)
        let (mut x0, mut z0) = (0.0, 1.0);
        for (j, &pg) in n::CO2_PRESS.iter().enumerate() {
            let z = n::Z_CO2[i][j];
            if !(z.is_finite() && pg < psat * 0.999) {
                continue;
            }
            let x = pg / psat;
            if x >= r {
                return Some(z0 + (z - z0) * (r - x0) / (x - x0));
            }
            (x0, z0) = (x, z);
        }
        Some(z0 + (zsat - z0) * ((r - x0) / (1.0 - x0)).clamp(0.0, 1.0))
    };
    let i = temps.iter().rposition(|&g| g <= t)?.min(last_sub.saturating_sub(1));
    let f = ((t - temps[i]) / (temps[i + 1] - temps[i])).clamp(0.0, 1.0);
    let blend = |r: f64| -> Option<f64> { Some(row_z(i, r)? * (1.0 - f) + row_z(i + 1, r)? * f) };
    // Correct the blend so that it ends exactly on the saturated vapour at T.
    Some(blend(r)? + r * (zsat_q - blend(1.0)?))
}

/// Bilinear interpolation in a Z(T, P) table; `None` outside the grid or
/// next to a liquid (NaN) cell.
fn interp_table<const N: usize>(table: &[[f64; N]], press: &[f64; N], t: f64, p_bar: f64) -> Option<f64> {
    let temps = &crate::nist_data::TEMPS;
    if !(temps[0]..=temps[temps.len() - 1]).contains(&t) || p_bar > press[N - 1] || p_bar < 0.0 {
        return None;
    }
    let locate = |grid: &[f64], x: f64| -> (usize, f64) {
        let i = grid.iter().rposition(|&g| g <= x).unwrap_or(0).min(grid.len() - 2);
        (i, (x - grid[i]) / (grid[i + 1] - grid[i]))
    };
    let (it, ft) = locate(temps, t);
    let z_at = |row: &[f64; N]| -> Option<f64> {
        if p_bar < press[0] {
            let z0 = row[0];
            return z0.is_finite().then(|| 1.0 + (z0 - 1.0) * p_bar / press[0]);
        }
        let (ip, fp) = locate(press, p_bar);
        let (a, b) = (row[ip], row[ip + 1]);
        (a.is_finite() && b.is_finite()).then(|| a + (b - a) * fp.clamp(0.0, 1.0))
    };
    let z1 = z_at(&table[it])?;
    let z2 = z_at(&table[it + 1])?;
    Some(z1 + (z2 - z1) * ft.clamp(0.0, 1.0))
}

/// Peng–Robinson compressibility factor (largest real root = vapour phase).
fn peng_robinson_z(props: &GasProperties, t: f64, p: f64) -> f64 {
    let tr = t / props.tc;
    let kappa = 0.374_64 + 1.542_26 * props.omega - 0.269_92 * props.omega * props.omega;
    let alpha = (1.0 + kappa * (1.0 - tr.sqrt())).powi(2);
    let a = 0.457_235 * R * R * props.tc * props.tc / props.pc * alpha;
    let b = 0.077_796 * R * props.tc / props.pc;
    let big_a = a * p / (R * R * t * t);
    let big_b = b * p / (R * t);
    // Z³ − (1−B)Z² + (A − 3B² − 2B)Z − (AB − B² − B³) = 0
    let c2 = -(1.0 - big_b);
    let c1 = big_a - 3.0 * big_b * big_b - 2.0 * big_b;
    let c0 = -(big_a * big_b - big_b * big_b - big_b.powi(3));
    let f = |z: f64| ((z + c2) * z + c1) * z + c0;
    let df = |z: f64| (3.0 * z + 2.0 * c2) * z + c1;
    // Newton from the ideal-gas side converges to the vapour root.
    let mut z: f64 = 1.0_f64.max(big_b * 1.5);
    for _ in 0..100 {
        let d = df(z);
        if d.abs() < 1e-14 {
            break;
        }
        let step = f(z) / d;
        z -= step;
        if step.abs() < 1e-12 {
            break;
        }
    }
    if z.is_finite() && z > big_b {
        z
    } else {
        1.0
    }
}

/// CO₂ saturation (vapour) pressure in Pa, Span & Wagner ancillary eq. (3.13).
/// Returns `None` above the critical temperature (304.13 K).
#[must_use]
pub fn saturation_pressure_co2(t: f64) -> Option<f64> {
    const TC: f64 = 304.128;
    const PC: f64 = 7.3773e6;
    if !(216.59..TC).contains(&t) {
        return None;
    }
    let tau = 1.0 - t / TC;
    let s = -7.060_208_7 * tau + 1.939_121_8 * tau.powf(1.5) - 1.646_359_7 * tau.powi(2) - 3.299_563_4 * tau.powi(4);
    Some(PC * (TC / t * s).exp())
}

/// CO₂ saturated liquid and vapour densities (kg/m³), Span & Wagner eqs. (3.14)–(3.15).
#[must_use]
pub fn saturation_densities_co2(t: f64) -> Option<(f64, f64)> {
    const TC: f64 = 304.128;
    const RHOC: f64 = 467.6;
    if !(216.59..TC).contains(&t) {
        return None;
    }
    let tau = 1.0 - t / TC;
    let liq = 1.924_510_8 * tau.powf(0.34) - 0.623_855_55 * tau.powf(0.5) - 0.327_311_27 * tau.powf(10.0 / 6.0)
        + 0.392_451_42 * tau.powf(11.0 / 6.0);
    let vap = -1.707_487_9 * tau.powf(0.34)
        - 0.822_746_7 * tau.powf(0.5)
        - 4.600_854_9 * tau
        - 10.111_178 * tau.powf(7.0 / 3.0)
        - 29.742_252 * tau.powf(14.0 / 3.0);
    Some((RHOC * liq.exp(), RHOC * vap.exp()))
}

/// Volume (m³) occupied by `mass` kg of gas once expanded to room conditions.
#[must_use]
pub fn expanded_volume(gas: GasType, mass: f64, t_room: f64, p_room: f64) -> f64 {
    mass / gas.ideal_density(t_room, p_room)
}

/// Absolute pressure (Pa) from a gauge reading (bar).
#[must_use]
pub fn gauge_bar_to_abs_pa(p_bar_g: f64, p_ambient_pa: f64) -> f64 {
    p_bar_g * 1e5 + p_ambient_pa
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::P_ATM;
    use approx::assert_relative_eq;

    #[test]
    fn relative_densities() {
        assert_relative_eq!(GasType::Argon.relative_density(), 1.379, epsilon = 0.01);
        assert_relative_eq!(GasType::CarbonDioxide.relative_density(), 1.519, epsilon = 0.01);
        assert_relative_eq!(GasType::Helium.relative_density(), 0.138, epsilon = 0.01);
        assert_eq!(GasType::Argon.buoyancy(), Buoyancy::Heavy);
        assert_eq!(GasType::CarbonDioxide.buoyancy(), Buoyancy::Heavy);
        assert_eq!(GasType::Nitrogen.buoyancy(), Buoyancy::Neutral);
        assert_eq!(GasType::Helium.buoyancy(), Buoyancy::Light);
        assert_eq!(GasType::Neon.buoyancy(), Buoyancy::Light);
    }

    #[test]
    fn ideal_density_at_20c() {
        // NIST: Ar 1.661, N2 1.165 kg/m³ at 20 °C, 1 atm.
        assert_relative_eq!(GasType::Argon.density(293.15, P_ATM), 1.661, epsilon = 0.01);
        assert_relative_eq!(GasType::Nitrogen.density(293.15, P_ATM), 1.165, epsilon = 0.01);
    }

    /// Compressibility at 200 bar(a), 20 °C vs NIST WebBook (REFPROP) values.
    #[test]
    fn compressibility_vs_nist_at_200_bar() {
        let t = 293.15;
        let p = 200e5;
        assert_relative_eq!(GasType::Nitrogen.compressibility(t, p), 1.05180, max_relative = 1e-4);
        assert_relative_eq!(GasType::Argon.compressibility(t, p), 0.94427, max_relative = 1e-4);
        assert_relative_eq!(GasType::Helium.compressibility(t, p), 1.09517, max_relative = 1e-4);
        assert_relative_eq!(GasType::Neon.compressibility(t, p), 1.09833, max_relative = 1e-4);
    }

    /// Off-grid points (interpolated) vs NIST at 15 °C, 200 and 300 bar (tolerance 0.3 %).
    #[test]
    fn compressibility_interpolation_vs_nist_15c() {
        let t = 288.15;
        assert_relative_eq!(GasType::Nitrogen.compressibility(t, 200e5), 1.04779, max_relative = 3e-3);
        assert_relative_eq!(GasType::Argon.compressibility(t, 200e5), 0.93677, max_relative = 3e-3);
        assert_relative_eq!(GasType::Helium.compressibility(t, 300e5), 1.14398, max_relative = 3e-3);
        assert_relative_eq!(GasType::Neon.compressibility(t, 300e5), 1.15227, max_relative = 3e-3);
    }

    /// CO₂ vapour close to saturation (vapour tail of a cylinder) vs NIST.
    #[test]
    fn co2_vapour_near_saturation() {
        // NIST/REFPROP: Z(293.15 K, 55 bar) = 0.5727 ; saturated vapour Z = 0.5520
        assert_relative_eq!(GasType::CarbonDioxide.compressibility(293.15, 55e5), 0.5727, max_relative = 0.01);
        // pressure(ρ) must be monotonic in the vapour tail (no jump at the table edge)
        let mut last = 0.0;
        for k in 0..400 {
            let rho = 100.0 + 0.235 * f64::from(k);
            let p = GasType::CarbonDioxide.pressure_from_density(293.15, rho);
            assert!(p > last, "non-monotonic at rho = {rho}");
            last = p;
        }
    }

    /// The fallback equations of state stay within 3 % of NIST at 200 bar.
    #[test]
    fn eos_fallback_accuracy() {
        let (t, p) = (293.15, 200e5);
        assert_relative_eq!(GasType::Nitrogen.compressibility_eos(t, p), 1.0518, max_relative = 0.03);
        assert_relative_eq!(GasType::Argon.compressibility_eos(t, p), 0.9443, max_relative = 0.03);
        assert_relative_eq!(GasType::Helium.compressibility_eos(t, p), 1.0952, max_relative = 0.03);
    }

    #[test]
    fn compressibility_tends_to_one_at_low_pressure() {
        for g in GasType::all() {
            assert_relative_eq!(g.compressibility(293.15, P_ATM), 1.0, epsilon = 0.01);
        }
    }

    #[test]
    fn pressure_density_roundtrip() {
        for g in [GasType::Nitrogen, GasType::Argon, GasType::Helium] {
            let p = 250e5;
            let rho = g.density(293.15, p);
            let p_back = g.pressure_from_density(293.15, rho);
            assert_relative_eq!(p_back, p, max_relative = 1e-4);
        }
    }

    #[test]
    fn co2_saturation_curve() {
        // NIST: Psat(20 °C) = 57.29 bar, ρl = 773.4, ρv = 194.2 kg/m³
        let p = saturation_pressure_co2(293.15).unwrap();
        assert_relative_eq!(p, 57.29e5, max_relative = 0.005);
        let (rl, rv) = saturation_densities_co2(293.15).unwrap();
        assert_relative_eq!(rl, 773.4, max_relative = 0.01);
        assert_relative_eq!(rv, 194.2, max_relative = 0.01);
        assert!(saturation_pressure_co2(310.0).is_none());
    }

    #[test]
    fn expanded_volume_of_one_kg_nitrogen() {
        // 1 kg N2 at 20 °C, 1 atm ≈ 0.858 m³
        assert_relative_eq!(expanded_volume(GasType::Nitrogen, 1.0, 293.15, P_ATM), 0.858, epsilon = 0.005);
    }
}
