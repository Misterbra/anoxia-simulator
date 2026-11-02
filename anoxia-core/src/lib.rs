//! # anoxia-core
//!
//! Oxygen-depletion (anoxia) simulation for rooms and confined spaces exposed
//! to an inert-gas release (N₂, Ar, CO₂, He, Ne).
//!
//! The library is split into:
//!
//! - [`gas`]: gas data, real-gas equation of state, CO₂ saturation curve;
//! - [`flow`]: orifice flow and container blowdown;
//! - [`solver`]: exact integration of the room oxygen balance (well-mixed and
//!   indicative two-zone models);
//! - [`safety`]: risk classification (O₂ and CO₂) and recommendations;
//! - [`analysis`]: parametric studies, sensitivity and required-ventilation sizing;
//! - [`report`]: self-contained HTML report;
//! - [`config`]: versioned configuration with legacy-file migration.
//!
//! The model is a screening tool: it does not replace a risk assessment by a
//! competent person nor continuous atmospheric monitoring.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod analysis;
pub mod config;
pub mod error;
pub mod flow;
pub mod gas;
pub mod i18n;
mod nist_data;
pub mod report;
pub mod safety;
pub mod solver;
pub mod validation;

pub use config::Configuration;
pub use error::{AnoxiaError, Result};
pub use gas::GasType;
pub use i18n::Language;
pub use safety::Assessment;
pub use solver::SimulationResult;

use serde::{Deserialize, Serialize};

/// Version of the software, embedded in reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Source code and releases.
pub const REPOSITORY_URL: &str = "https://github.com/Misterbra/anoxia-simulator";

/// Page of the latest published version.
pub const RELEASES_URL: &str = "https://github.com/Misterbra/anoxia-simulator/releases/latest";

/// Author of the software.
pub const AUTHOR: &str = "Nabil Brag";

/// Professional profile of the author.
pub const AUTHOR_LINKEDIN_URL: &str = "https://www.linkedin.com/in/nabil-brg/";

/// Licence of the software (SPDX identifier).
pub const LICENCE: &str = "AGPL-3.0-or-later";

/// Physical constants and reference conditions.
pub mod constants {
    /// Molar gas constant, J/(mol·K) — CODATA 2018 (exact).
    pub const R: f64 = 8.314_462_618;
    /// Standard atmosphere, Pa.
    pub const P_ATM: f64 = 101_325.0;
    /// Molar mass of dry air, kg/mol.
    pub const M_AIR: f64 = 28.9647e-3;
    /// O₂ in dry ambient air, % vol (20.946 %, rounded to 20.95).
    pub const O2_AMBIENT_PERCENT: f64 = 20.95;
    /// 0 °C in kelvin.
    pub const ZERO_CELSIUS: f64 = 273.15;

    /// Convert °C to K.
    #[must_use]
    pub fn celsius_to_kelvin(t: f64) -> f64 {
        t + ZERO_CELSIUS
    }
}

/// A simulation together with the inputs that produced it and its assessment.
///
/// Reports are always generated from a `SimulationRun`, never from the
/// current (possibly edited) inputs, so results and parameters stay consistent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationRun {
    /// Inputs
    pub config: Configuration,
    /// Time series and summary
    pub result: SimulationResult,
    /// Risk assessment
    pub assessment: Assessment,
}

/// Validate the configuration, simulate and assess.
///
/// # Errors
/// Invalid configuration or impossible physical state.
pub fn run(config: &Configuration) -> Result<SimulationRun> {
    let result = solver::simulate(config)?;
    let assessment = Assessment::new(config, &result);
    Ok(SimulationRun { config: config.clone(), result, assessment })
}
