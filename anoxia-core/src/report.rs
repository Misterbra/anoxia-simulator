//! Self-contained HTML report (no external resources: works offline, prints
//! to PDF and opens in Word/LibreOffice). Charts are inline SVG.

use crate::analysis::{max_leak_diameter, required_ventilation, Criterion, Sizing};
use crate::config::{
    Co2Withdrawal, Configuration, ExtractionLocation, ReleaseKind, StratificationMode, VentilationMode,
};
use crate::constants::O2_AMBIENT_PERCENT;
use crate::flow::FlowRegime;
use crate::i18n::{escape_html as esc, fmt_clock, fmt_duration, fmt_num, fmt_pct, fmt_unit, Language};
use crate::safety::{co2_effects_table, disclaimer, model_limitations, o2_effects_table, typo, Flag};
use crate::solver::Summary;
use crate::SimulationRun;
use std::fmt::Write as _;

/// Optional sizing results to include in the report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizingSummary {
    /// Required mechanical ventilation flow (m³/h), in `mode`
    pub required_ventilation: Sizing,
    /// Maximum tolerable leak diameter (mm)
    pub max_diameter: Sizing,
    /// O₂ criterion used (% vol)
    pub criterion_o2: f64,
    /// Ventilation mode the flow was computed for (extraction or supply)
    pub mode: VentilationMode,
}

impl SizingSummary {
    /// Compute the sizing for `config` with the O₂ criterion `criterion_o2`
    /// (and CO₂ ≤ 3 % for CO₂). A few tens of simulations: well under a second.
    ///
    /// # Errors
    /// Invalid configuration.
    pub fn compute(config: &Configuration, criterion_o2: f64) -> crate::Result<Self> {
        let crit = Criterion { min_o2: criterion_o2, max_co2: 3.0 };
        Ok(Self {
            required_ventilation: required_ventilation(config, crit)?,
            max_diameter: max_leak_diameter(config, crit)?,
            criterion_o2,
            mode: config.room.mechanical_mode,
        })
    }
}

/// Ventilation-failure check for the report: the scenario with the mechanical
/// ventilation running and the same scenario with it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct FailureSummary {
    /// Mechanical ventilation running
    pub running: Summary,
    /// Mechanical ventilation failed (infiltration only)
    pub failed: Summary,
}

impl FailureSummary {
    /// Compute the check; `None` when the scenario has no mechanical
    /// ventilation (nothing to lose) or when a simulation fails.
    #[must_use]
    pub fn compute(config: &Configuration) -> Option<Self> {
        if !config.room.mechanical_enabled || config.room.mechanical_flow_m3h <= 0.0 {
            return None;
        }
        // Total failure from the start: no mechanical ventilation and no backup fan
        let mut failed = config.without_ventilation_events();
        failed.room.mechanical_enabled = false;
        Some(Self {
            running: crate::solver::simulate(config).ok()?.summary,
            failed: crate::solver::simulate(&failed).ok()?.summary,
        })
    }
}

/// Plain-language key messages: what happens, when, and what changes if the
/// mechanical ventilation fails. Shared by the Results tab and the report so
/// that the same sentences are read on screen and on paper.
#[must_use]
pub fn key_messages(run: &SimulationRun, failure: Option<&FailureSummary>, lang: Language) -> Vec<String> {
    let s = &run.result.summary;
    let a = &run.assessment;
    let fr = lang == Language::French;
    let after = if fr { "après le début du rejet" } else { "after the start of the release" };
    let dur = |v: f64| fmt_duration(v, lang);
    // "immédiatement" rather than "0 s après le début du rejet"
    let when = |v: f64| {
        if v < 1.0 {
            (if fr { "immédiatement" } else { "immediately" }).to_string()
        } else {
            format!("{} {after}", fmt_duration(v, lang))
        }
    };
    let a1 = fmt_pct(a.alarm1, 1, lang);
    let a2 = fmt_pct(a.alarm2, 1, lang);
    let min = fmt_pct(s.min_o2, 1, lang);
    let mut out = Vec::new();

    // Alarms
    out.push(match (a.t_alarm1, a.t_alarm2) {
        (Some(t1), Some(t2)) => {
            let (t1, t2) = (when(t1), when(t2));
            if fr {
                format!("Le seuil d'alarme 1 ({a1} d'O₂) est atteint {t1} ; le seuil d'alarme 2 ({a2}) {t2}.")
            } else {
                format!("Alarm level 1 ({a1} O₂) is reached {t1}; alarm level 2 ({a2}) {t2}.")
            }
        }
        (Some(t1), None) => {
            let t1 = when(t1);
            if fr {
                format!(
                    "Le seuil d'alarme 1 ({a1} d'O₂) est atteint {t1} ; le seuil d'alarme 2 ({a2}) n'est pas atteint."
                )
            } else {
                format!("Alarm level 1 ({a1} O₂) is reached {t1}; alarm level 2 ({a2}) is not reached.")
            }
        }
        (None, _) => {
            if fr {
                format!("Aucun seuil d'alarme n'est atteint : l'oxygène reste au-dessus de {a1}.")
            } else {
                format!("No alarm level is reached: oxygen stays above {a1}.")
            }
        }
    });

    // Life-threatening level
    match a.t_10 {
        Some(t10) => {
            let t10 = when(t10);
            out.push(if fr {
                format!("Le niveau dangereux pour la vie (10 % d'O₂) est atteint {t10}.")
            } else {
                format!("The life-threatening level (10% O₂) is reached {t10}.")
            });
        }
        None if a.t_alarm1.is_some() => out.push(if fr {
            "Le niveau dangereux pour la vie (10 % d'O₂) n'est pas atteint.".to_string()
        } else {
            "The life-threatening level (10% O₂) is not reached.".to_string()
        }),
        None => {}
    }

    // Minimum and recovery
    let tmin = when(s.time_at_min);
    let min = match s.governing {
        crate::solver::Governing::Mixed => min,
        g => format!("{min} ({})", g.label(lang)),
    };
    let back = s.o2_crossing(19.5).filter(|x| x.reached()).map(|x| x.back);
    out.push(match back {
        Some(Some(b)) => {
            let b = when(b);
            if fr {
                format!("L'oxygène descend au plus bas à {min} ({tmin}) et repasse au-dessus de 19,5 % {b}.")
            } else {
                format!("Oxygen falls to a minimum of {min} ({tmin}) and rises back above 19.5% {b}.")
            }
        }
        Some(None) => {
            if fr {
                format!("L'oxygène descend au plus bas à {min} ({tmin}) et ne repasse pas au-dessus de 19,5 % dans la durée simulée.")
            } else {
                format!("Oxygen falls to a minimum of {min} ({tmin}) and does not rise back above 19.5% within the simulated time.")
            }
        }
        None => {
            if fr {
                format!("L'oxygène descend au plus bas à {min} ({tmin}).")
            } else {
                format!("Oxygen falls to a minimum of {min} ({tmin}).")
            }
        }
    });

    // Partial pressure: what the body actually receives (altitude matters)
    {
        let pmin = fmt_unit(a.po2_min_kpa, 1, "kPa", lang);
        let pamb = fmt_unit(a.po2_ambient_kpa, 1, "kPa", lang);
        let p = fmt_unit(a.room_pressure_hpa, 0, "hPa", lang);
        out.push(if fr {
            format!("En pression partielle, l'oxygène descend à {pmin} contre {pamb} dans l'air ambiant à {p}.")
        } else {
            format!("In partial pressure, oxygen falls to {pmin} against {pamb} in ambient air at {p}.")
        });
    }

    // Timeline: what was assumed to happen during the release
    if !a.events.is_empty() {
        let list = a.events.iter().map(|e| e.describe(lang)).collect::<Vec<_>>().join(" ; ");
        let mut sentence =
            if fr { format!("Chronologie prise en compte — {list}") } else { format!("Timeline applied — {list}") };
        if run.config.isolation_time().is_some() && s.releasable_mass > 0.0 {
            let released = fmt_unit(s.released_mass / s.releasable_mass * s.releasable_volume_m3, 1, "m³", lang);
            let total = fmt_unit(s.releasable_volume_m3, 1, "m³", lang);
            sentence.push_str(&if fr {
                format!(" ; quantité effectivement rejetée : {released} sur {total} libérables")
            } else {
                format!("; quantity actually released: {released} of {total} releasable")
            });
        }
        out.push(format!("{sentence}."));
    }

    // Near field
    if let Some(n) = a.near {
        let r = fmt_num(n.radius_m, 1, lang);
        let x = fmt_pct(n.min_o2, 1, lang);
        let tm = when(n.time_at_min);
        let alarm = match n.t_alarm1 {
            Some(ta) => {
                let ta = when(ta);
                if fr {
                    format!("le seuil d'alarme 1 y est atteint {ta}")
                } else {
                    format!("alarm level 1 is reached there {ta}")
                }
            }
            None => {
                if fr {
                    "le seuil d'alarme 1 n'y est pas atteint".to_string()
                } else {
                    "alarm level 1 is not reached there".to_string()
                }
            }
        };
        out.push(if fr {
            format!("Près de la fuite (à moins de {r} m), l'oxygène descend à {x} ({tm}) ; {alarm}.")
        } else {
            format!("Next to the release (within {r} m), oxygen falls to {x} ({tm}); {alarm}.")
        });
    }

    // CO₂
    if let Some(peak) = s.max_co2_percent {
        let peak = fmt_pct(peak, 1, lang);
        out.push(match a.t_co2_idlh {
            Some(ti) => {
                let ti = when(ti);
                if fr {
                    format!("Le CO₂ atteint {peak} ; la valeur DIVS (4 %) est dépassée {ti}.")
                } else {
                    format!("CO₂ reaches {peak}; the IDLH value (4%) is exceeded {ti}.")
                }
            }
            None => {
                if fr {
                    format!("Le CO₂ atteint {peak} ; la valeur DIVS (4 %) n'est pas dépassée.")
                } else {
                    format!("CO₂ reaches {peak}; the IDLH value (4%) is not exceeded.")
                }
            }
        });
    }

    // Ventilation failure
    if let Some(f) = failure {
        let below = |x: &Summary| x.o2_crossing(a.alarm1).filter(|x| x.reached()).map_or(0.0, |x| x.total);
        let (on, off) = (below(&f.running), below(&f.failed));
        let fmin = fmt_pct(f.failed.min_o2, 1, lang);
        let same_min = (f.failed.min_o2 - f.running.min_o2).abs() < 0.05;
        // "au moins 6 min" when the room is still below the level at the end
        let text = |x: &Summary| x.o2_crossing(a.alarm1).map_or_else(|| dur(0.0), |c| c.total_text(lang));
        let (don, doff) = (text(&f.running), text(&f.failed));
        out.push(if off <= 0.0 {
            if fr {
                format!("En cas de panne de la ventilation mécanique, l'oxygène descend à {fmin} et le seuil d'alarme 1 n'est toujours pas atteint.")
            } else {
                format!("If the mechanical ventilation fails, oxygen falls to {fmin} and alarm level 1 is still not reached.")
            }
        } else if on <= 0.0 {
            if fr {
                format!("En cas de panne de la ventilation mécanique, l'oxygène descend à {fmin} et le local reste sous l'alarme 1 pendant {doff}, alors que ce seuil n'est pas franchi avec la ventilation en service.")
            } else {
                format!("If the mechanical ventilation fails, oxygen falls to {fmin} and the room stays below alarm 1 for {doff}, whereas this level is not crossed with the ventilation running.")
            }
        } else if same_min {
            if fr {
                format!("En cas de panne de la ventilation mécanique, le minimum ne change pas ({fmin}) mais le local reste sous l'alarme 1 pendant {doff} au lieu de {don}.")
            } else {
                format!("If the mechanical ventilation fails, the minimum does not change ({fmin}) but the room stays below alarm 1 for {doff} instead of {don}.")
            }
        } else if fr {
            format!("En cas de panne de la ventilation mécanique, l'oxygène descend à {fmin} au lieu de {min} et le local reste sous l'alarme 1 pendant {doff} au lieu de {don}.")
        } else {
            format!("If the mechanical ventilation fails, oxygen falls to {fmin} instead of {min} and the room stays below alarm 1 for {doff} instead of {don}.")
        });
    }

    // Regulatory bounding method
    if a.has(Flag::DisplacementMoreSevere) {
        let d = fmt_pct(a.displacement_o2, 1, lang);
        out.push(if fr {
            format!("Méthode réglementaire INRS/BCGA (tout l'inventaire d'un coup, sans ventilation) : {d} d'O₂, classement plus sévère à retenir si la ventilation ou le mélange ne peuvent pas être garantis.")
        } else {
            format!("Regulatory INRS/BCGA method (whole inventory at once, no ventilation): {d} O₂, a more severe class to retain if ventilation or mixing cannot be guaranteed.")
        });
    }
    out.into_iter().map(|m| typo(&m, lang)).collect()
}

/// Time series as CSV text. Readable headers with units in `lang`; French
/// uses ";" and the decimal comma (Excel FR), English "," and the point. A
/// byte-order mark lets Excel detect UTF-8 (O₂, accents).
#[must_use]
pub fn csv_text(run: &SimulationRun, lang: Language) -> String {
    let r = &run.result;
    let (sep, fr) = match lang {
        Language::French => (';', true),
        Language::English => (',', false),
    };
    let f = |v: f64| {
        let s = format!("{v:.6}");
        if fr {
            s.replace('.', ",")
        } else {
            s
        }
    };
    let col = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let mut header = vec![
        col("Temps (s)", "Time (s)"),
        col("O₂ bien mélangé (% vol)", "O₂ well-mixed (% vol)"),
        col("O₂ retenu pour le classement (% vol)", "O₂ used for classification (% vol)"),
    ];
    if r.o2_zone.is_some() {
        header.extend([
            col("O₂ zone d'accumulation (% vol)", "O₂ accumulation zone (% vol)"),
            col("O₂ reste du local (% vol)", "O₂ rest of room (% vol)"),
        ]);
    }
    if r.co2.is_some() {
        header.push(col("CO₂ (% vol)", "CO₂ (% vol)"));
    }
    header.extend([
        col("Débit massique rejeté (kg/s)", "Released mass flow (kg/s)"),
        col("Pression du récipient (bar rel.)", "Container pressure (bar g)"),
        col("Masse rejetée cumulée (kg)", "Cumulated released mass (kg)"),
    ]);
    let sep = sep.to_string();
    let mut out = String::from('\u{feff}');
    out.push_str(&header.join(&sep));
    out.push('\n');
    for i in 0..r.time.len() {
        let mut cols = vec![f(r.time[i]), f(r.o2_mixed[i]), f(r.o2[i])];
        if let (Some(z), Some(o)) = (&r.o2_zone, &r.o2_other) {
            cols.push(f(z[i]));
            cols.push(f(o[i]));
        }
        if let Some(c) = &r.co2 {
            cols.push(f(c[i]));
        }
        cols.push(f(r.mass_flow[i]));
        cols.push(f(r.source_pressure_bar_g[i]));
        cols.push(f(r.released_mass[i]));
        out.push_str(&cols.join(&sep));
        out.push('\n');
    }
    out
}

/// Before / after study for the report: the four configurations
/// (current / future × ventilation running / failed).
#[derive(Debug, Clone, PartialEq)]
pub struct BeforeAfterSummary {
    /// Description of the change (differences, one per line) and free label
    pub label: String,
    /// Differences between current and future (one per changed value)
    pub differences: Vec<String>,
    /// Labelled summaries, in the order current-on, current-off, future-on, future-off
    pub cases: Vec<(String, Summary)>,
}

impl BeforeAfterSummary {
    /// Compute the study when it is enabled and defines at least one change.
    /// `Some(Err(cause))` when the study was requested but cannot be computed
    /// (invalid future configuration): the report then says so instead of
    /// omitting the section.
    #[must_use]
    pub fn compute(config: &Configuration, lang: Language) -> Option<Result<Self, String>> {
        if !config.future.enabled || config.future.is_empty() {
            return None;
        }
        Some(crate::analysis::before_after(config, lang).map_err(|e| e.to_string()).map(|variants| Self {
            label: config.future.label.trim().to_string(),
            differences: config.future.differences(config, lang),
            cases: variants.into_iter().map(|v| (v.label, v.result.summary)).collect(),
        }))
    }
}

/// Report options.
#[derive(Debug, Clone)]
pub struct ReportOptions {
    /// Language
    pub lang: Language,
    /// Generation date/time, already formatted
    pub generated_at: String,
    /// Optional sizing results
    pub sizing: Option<SizingSummary>,
    /// Optional ventilation-failure check
    pub failure: Option<FailureSummary>,
    /// Optional before / after study; `Err` carries the cause when the study
    /// was requested but could not be computed
    pub before_after: Option<Result<BeforeAfterSummary, String>>,
}

/// A data series for [`svg_line_chart`].
pub struct Series<'a> {
    /// Legend label
    pub label: String,
    /// X values
    pub x: &'a [f64],
    /// Y values
    pub y: &'a [f64],
    /// Stroke colour (CSS)
    pub color: &'a str,
    /// Stroke width (px)
    pub width: f64,
    /// Dash pattern (SVG `stroke-dasharray`), empty for solid
    pub dash: &'a str,
}

/// Horizontal reference line.
pub struct HLine {
    /// Y value
    pub y: f64,
    /// Label
    pub label: String,
    /// Colour (CSS)
    pub color: &'static str,
}

fn nice_step(range: f64, target_ticks: f64) -> f64 {
    let raw = range / target_ticks;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Render a line chart as inline SVG. Time axis in seconds, displayed in
/// s, min or h depending on the span.
#[must_use]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn svg_line_chart(
    series: &[Series<'_>],
    hlines: &[HLine],
    y_min: f64,
    y_max: f64,
    y_label: &str,
    lang: Language,
    width: f64,
    height: f64,
    x_end: Option<f64>,
) -> String {
    svg_line_chart_marked(series, hlines, &[], y_min, y_max, y_label, lang, width, height, x_end)
}

/// Vertical marker of a dated event on a chart.
#[derive(Debug, Clone, PartialEq)]
pub struct VLine {
    /// Time (s)
    pub x: f64,
    /// Short label, written along the line
    pub label: String,
}

/// Markers of the scenario timeline for the charts of a run.
#[must_use]
pub fn event_markers(run: &SimulationRun, lang: Language) -> Vec<VLine> {
    run.config.timeline().iter().map(|e| VLine { x: e.time_s, label: e.action.short(lang) }).collect()
}

/// [`svg_line_chart`] with vertical event markers.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn svg_line_chart_marked(
    series: &[Series<'_>],
    hlines: &[HLine],
    vlines: &[VLine],
    y_min: f64,
    y_max: f64,
    y_label: &str,
    lang: Language,
    width: f64,
    height: f64,
    x_end: Option<f64>,
) -> String {
    let (ml, mr, mt, mb) = (64.0, 16.0, 16.0, 70.0);
    let pw = width - ml - mr;
    let ph = height - mt - mb;
    let x_full = series.iter().filter_map(|s| s.x.last().copied()).fold(1.0_f64, f64::max);
    let x_max = x_end.map_or(x_full, |e| e.clamp(1.0, x_full));
    let (t_div, t_unit) = if x_max > 3.0 * 3600.0 {
        (3600.0, "h")
    } else if x_max > 600.0 {
        (60.0, "min")
    } else {
        (1.0, "s")
    };
    let sx = |x: f64| ml + x / x_max * pw;
    let sy = |y: f64| mt + (1.0 - (y - y_min) / (y_max - y_min)) * ph;
    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="100%" role="img" font-family="Segoe UI, Arial, sans-serif" font-size="12">"#
    );
    let _ = write!(svg, r##"<rect x="{ml}" y="{mt}" width="{pw}" height="{ph}" fill="#fff" stroke="#9e9e9e"/>"##);

    // Y grid
    let ystep = nice_step(y_max - y_min, 6.0);
    let mut y = (y_min / ystep).ceil() * ystep;
    while y <= y_max + 1e-9 {
        let py = sy(y);
        let _ = write!(
            svg,
            r##"<line x1="{ml}" x2="{}" y1="{py:.1}" y2="{py:.1}" stroke="#e0e0e0"/><text x="{}" y="{:.1}" text-anchor="end" fill="#424242">{}</text>"##,
            ml + pw,
            ml - 6.0,
            py + 4.0,
            fmt_num(y, if ystep < 1.0 { 1 } else { 0 }, lang)
        );
        y += ystep;
    }
    // X grid
    let xstep = nice_step(x_max / t_div, 8.0);
    let mut x = 0.0;
    while x <= x_max / t_div + 1e-9 {
        let px = sx(x * t_div);
        let _ = write!(
            svg,
            r##"<line x1="{px:.1}" x2="{px:.1}" y1="{mt}" y2="{}" stroke="#eeeeee"/><text x="{px:.1}" y="{}" text-anchor="middle" fill="#424242">{}</text>"##,
            mt + ph,
            mt + ph + 16.0,
            fmt_num(x, if xstep < 1.0 { 1 } else { 0 }, lang)
        );
        x += xstep;
    }
    let _ = write!(
        svg,
        r##"<text x="{}" y="{}" text-anchor="middle" fill="#212121">{} ({t_unit})</text>"##,
        ml + pw / 2.0,
        mt + ph + 34.0,
        lang.pick("Temps", "Time")
    );
    let _ = write!(
        svg,
        r##"<text transform="translate(16 {}) rotate(-90)" text-anchor="middle" fill="#212121">{}</text>"##,
        mt + ph / 2.0,
        esc(y_label)
    );

    for h in hlines {
        if h.y < y_min || h.y > y_max {
            continue;
        }
        let py = sy(h.y);
        let _ = write!(
            svg,
            r#"<line x1="{ml}" x2="{}" y1="{py:.1}" y2="{py:.1}" stroke="{}" stroke-dasharray="6 4" stroke-width="1.2"/><text x="{}" y="{:.1}" text-anchor="end" fill="{}" font-size="11">{}</text>"#,
            ml + pw,
            h.color,
            ml + pw - 4.0,
            py - 3.0,
            h.color,
            esc(&h.label)
        );
    }

    // Dated events (ventilation failure, backup fan, isolation): dotted
    // vertical line with its label written upwards along the line.
    for v in vlines {
        if v.x < 0.0 || v.x > x_max {
            continue;
        }
        let px = sx(v.x);
        let _ = write!(
            svg,
            r##"<line x1="{px:.1}" x2="{px:.1}" y1="{mt}" y2="{}" stroke="#546e7a" stroke-dasharray="2 3" stroke-width="1.2"/><text x="{:.1}" y="{:.1}" fill="#546e7a" font-size="11" text-anchor="end" transform="rotate(-90 {:.1} {:.1})">{}</text>"##,
            mt + ph,
            px - 4.0,
            mt + 4.0,
            px - 4.0,
            mt + 4.0,
            esc(&v.label)
        );
    }

    for s in series {
        if s.x.is_empty() {
            continue;
        }
        // Points inside the window, decimated to ≤ 1500 buckets while keeping
        // both extremes of each bucket (dips and peaks are never clipped).
        let n = s.x.iter().position(|&x| x > x_max).map_or(s.x.len(), |k| (k + 1).min(s.x.len()));
        let bucket = n.div_ceil(1500).max(1);
        let mut d = String::new();
        let mut first = true;
        let mut i = 0;
        let mut point = |d: &mut String, k: usize| {
            let _ = write!(
                d,
                "{}{:.1},{:.1} ",
                if first { "M" } else { "L" },
                sx(s.x[k].min(x_max)),
                sy(s.y[k].clamp(y_min, y_max))
            );
            first = false;
        };
        while i < n {
            let end = (i + bucket).min(n);
            let kmin = (i..end).min_by(|&a, &b| s.y[a].total_cmp(&s.y[b])).unwrap_or(i);
            let kmax = (i..end).max_by(|&a, &b| s.y[a].total_cmp(&s.y[b])).unwrap_or(i);
            let (k1, k2) = if kmin <= kmax { (kmin, kmax) } else { (kmax, kmin) };
            point(&mut d, k1);
            if k2 != k1 {
                point(&mut d, k2);
            }
            i = end;
        }
        let _ = write!(
            svg,
            r#"<path d="{d}" fill="none" stroke="{}" stroke-width="{}" stroke-dasharray="{}" stroke-linejoin="round"/>"#,
            s.color, s.width, s.dash
        );
    }

    // Legend
    let mut lx = ml;
    let ly = height - 14.0;
    for s in series {
        let _ = write!(
            svg,
            r##"<line x1="{lx}" x2="{}" y1="{ly}" y2="{ly}" stroke="{}" stroke-width="{}" stroke-dasharray="{}"/><text x="{}" y="{}" fill="#212121">{}</text>"##,
            lx + 24.0,
            s.color,
            s.width,
            s.dash,
            lx + 30.0,
            ly + 4.0,
            esc(&s.label)
        );
        #[allow(clippy::cast_precision_loss)]
        let w = 50.0 + 6.5 * s.label.chars().count() as f64;
        lx += w;
    }
    svg.push_str("</svg>");
    svg
}

/// O₂ chart of a run (shared by the report and exports); `x_end` limits the
/// time axis (s), e.g. to the event window.
#[must_use]
pub fn o2_chart_svg(run: &SimulationRun, lang: Language, width: f64, height: f64, x_end: Option<f64>) -> String {
    let r = &run.result;
    let a = &run.assessment;
    let mut series = Vec::new();
    if let (Some(zone), Some(other)) = (&r.o2_zone, &r.o2_other) {
        series.push(Series {
            label: lang.pick("Zone d'accumulation (bi-zone)", "Accumulation zone (two-zone)").to_string(),
            x: &r.time,
            y: zone,
            color: "#c62828",
            width: 2.2,
            dash: "",
        });
        series.push(Series {
            label: lang.pick("Reste du local (bi-zone)", "Rest of room (two-zone)").to_string(),
            x: &r.time,
            y: other,
            color: "#6a1b9a",
            width: 1.4,
            dash: "2 3",
        });
        series.push(Series {
            label: lang.pick("Modèle bien mélangé", "Well-mixed model").to_string(),
            x: &r.time,
            y: &r.o2_mixed,
            color: "#1565c0",
            width: 1.8,
            dash: "8 4",
        });
    } else {
        series.push(Series {
            label: lang.pick("O₂ – modèle bien mélangé", "O₂ – well-mixed model").to_string(),
            x: &r.time,
            y: &r.o2_mixed,
            color: "#1565c0",
            width: 2.4,
            dash: "",
        });
    }
    if let Some(near) = &r.o2_near {
        let radius = fmt_num(run.config.model.near_field_radius_m, 1, lang);
        series.push(Series {
            label: match lang {
                Language::French => format!("Près de la fuite (champ proche, r = {radius} m)"),
                Language::English => format!("Next to the release (near field, r = {radius} m)"),
            },
            x: &r.time,
            y: near,
            color: "#e65100",
            width: 1.6,
            dash: "4 3",
        });
    }
    let near_min = r.summary.min_o2_near.unwrap_or(r.summary.min_o2);
    let y_min = (r.summary.min_o2.min(near_min) - 1.0).floor().clamp(0.0, 15.0);
    let hlines = vec![
        HLine {
            y: a.alarm1,
            label: format!("{} {}", lang.pick("Alarme 1", "Alarm 1"), fmt_pct(a.alarm1, 1, lang)),
            color: "#b45309",
        },
        HLine {
            y: a.alarm2,
            label: format!("{} {}", lang.pick("Alarme 2", "Alarm 2"), fmt_pct(a.alarm2, 1, lang)),
            color: "#b71c1c",
        },
        HLine {
            y: 10.0,
            label: format!("{} 10 %", lang.pick("Danger vital", "Life-threatening"))
                .replace("10 %", &fmt_pct(10.0, 0, lang)),
            color: "#4a148c",
        },
    ];
    svg_line_chart_marked(
        &series,
        &hlines,
        &event_markers(run, lang),
        y_min,
        21.5,
        lang.pick("O₂ (% vol)", "O₂ (% vol)"),
        lang,
        width,
        height,
        x_end,
    )
}

/// CO₂ chart of a run (CO₂ releases only). `y_cap` clips the vertical axis
/// (e.g. 10.5 %) so that the exposure limits stay legible under a very high peak.
#[must_use]
pub fn co2_chart_svg(
    run: &SimulationRun,
    lang: Language,
    width: f64,
    height: f64,
    x_end: Option<f64>,
    y_cap: Option<f64>,
) -> Option<String> {
    let co2 = run.result.co2.as_ref()?;
    let max = co2.iter().copied().fold(0.0, f64::max);
    let label = match y_cap {
        Some(_) => format!("CO₂ ({} {})", lang.pick("pic", "peak"), fmt_pct(max, 1, lang)),
        None => lang.pick("CO₂ (série d'évaluation)", "CO₂ (assessment series)").to_string(),
    };
    let series = [Series { label, x: &run.result.time, y: co2, color: "#37474f", width: 2.2, dash: "" }];
    let hlines = vec![
        HLine { y: 0.5, label: lang.pick("VLEP 0,5 %", "OEL 0.5%").to_string(), color: "#b45309" },
        HLine { y: 3.0, label: lang.pick("VLCT 3 %", "STEL 3%").to_string(), color: "#b71c1c" },
        HLine { y: 4.0, label: lang.pick("DIVS 4 %", "IDLH 4%").to_string(), color: "#4a148c" },
    ];
    let y_max = y_cap.unwrap_or_else(|| (max * 1.15).max(5.0).ceil());
    Some(svg_line_chart_marked(
        &series,
        &hlines,
        &event_markers(run, lang),
        0.0,
        y_max,
        lang.pick("CO₂ (% vol)", "CO₂ (% vol)"),
        lang,
        width,
        height,
        x_end,
    ))
}

fn opt_time(t: Option<f64>, lang: Language) -> String {
    t.map_or_else(
        || lang.pick("non atteint", "not reached").to_string(),
        |v| format!("{} ({})", fmt_clock(v), fmt_duration(v, lang)),
    )
}

fn row(out: &mut String, lang: Language, k: &str, v: &str) {
    let _ = write!(out, "<tr><th>{}</th><td>{}</td></tr>", esc(&typo(k, lang)), esc(&typo(v, lang)));
}

/// Generate the full HTML report.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn html(run: &SimulationRun, opts: &ReportOptions) -> String {
    let lang = opts.lang;
    let c = &run.config;
    let s = &run.result.summary;
    let a = &run.assessment;
    let id = &c.identification;
    let (rr, gg, bb) = a.overall.color();
    let t = |fr: &str, en: &str| lang.pick(fr, en).to_string();

    let mut h = String::with_capacity(64 * 1024);
    let _ = write!(
        h,
        r#"<!DOCTYPE html><html lang="{}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{}</title><style>
body{{font-family:"Segoe UI",Arial,sans-serif;color:#212121;max-width:960px;margin:24px auto;padding:0 24px;line-height:1.45;font-size:14px}}
h1{{font-size:22px;margin:0}}h2{{font-size:17px;border-bottom:2px solid #37474f;padding-bottom:4px;margin-top:28px;break-after:avoid}}h3{{font-size:14px;color:#37474f;margin:14px 0 4px;break-after:avoid}}
.meta{{color:#546e7a;font-size:12px}}header{{display:flex;justify-content:space-between;align-items:flex-end;border-bottom:3px solid #37474f;padding-bottom:8px}}
table{{border-collapse:collapse;width:100%;margin:8px 0;break-inside:avoid}}th,td{{border:1px solid #cfd8dc;padding:5px 8px;text-align:left;vertical-align:top}}
th{{background:#eceff1;font-weight:600;width:42%}}table.grid th{{width:auto}}
.badge{{background:rgb({rr},{gg},{bb});color:#fff;padding:14px 18px;border-radius:6px;margin:14px 0}}.badge b{{font-size:20px;letter-spacing:.5px}}
.kpis{{display:grid;grid-template-columns:repeat(4,1fr);gap:8px}}.kpi{{border:1px solid #cfd8dc;border-radius:6px;padding:8px}}.kpi .v{{font-size:20px;font-weight:700}}.kpi .l{{font-size:12px;color:#546e7a}}.kpi .s{{font-size:11px;color:#546e7a;margin-top:2px}}
.key{{background:#f1f5f9;border-left:4px solid #1565c0;padding:8px 14px;margin:10px 0;break-inside:avoid}}.key ul{{margin:4px 0 0 18px;padding:0}}.key li{{margin:3px 0}}
.rec .note{{font-size:12px;color:#37474f;margin-top:3px;font-style:italic}}
.warn{{border-left:4px solid #b45309;background:#fff8e1;padding:6px 10px;margin:6px 0}}.warn.danger{{border-color:#b71c1c;background:#fdecea}}.warn.info{{border-color:#90a4ae;background:#f5f7f8}}
.rec{{border-left:4px solid #90a4ae;padding:6px 10px;margin:6px 0;break-inside:avoid}}.rec.Critical{{border-color:#b71c1c}}.rec.High{{border-color:#b45309}}.rec .p{{font-size:11px;text-transform:uppercase;color:#546e7a}}
.sign td{{height:56px}}footer{{margin-top:28px;font-size:11px;color:#546e7a;border-top:1px solid #cfd8dc;padding-top:8px}}
@media print{{body{{margin:0;max-width:none}}.badge{{-webkit-print-color-adjust:exact;print-color-adjust:exact}}h2{{break-after:avoid}}}}
</style></head><body>"#,
        lang.code(),
        esc(&t("Rapport de simulation d'anoxie", "Anoxia simulation report"))
    );

    // Header
    let _ = write!(
        h,
        r#"<header><div><h1>{}</h1><div class="meta">{}</div></div><div class="meta" style="text-align:right">{}<br>{} {}<br>{}</div></header>"#,
        esc(&t("Rapport d'évaluation du risque d'anoxie", "Anoxia risk assessment report")),
        esc(&[id.unit.as_str(), id.building.as_str(), id.installation.as_str()]
            .iter()
            .filter(|x| !x.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" — ")),
        if id.reference.is_empty() {
            String::new()
        } else {
            format!("{} {}", esc(&t("Réf.", "Ref.")), esc(&id.reference))
        },
        esc(&t("Édité le", "Issued on")),
        esc(&opts.generated_at),
        esc(&format!("Anoxia Simulator v{}", crate::VERSION)),
    );

    // Summary
    let _ = write!(h, "<h2>1. {}</h2>", esc(&t("Synthèse", "Summary")));
    let _ = write!(h, r#"<div class="badge"><b>{}</b><br>{}</div>"#, esc(a.headline(lang)), esc(&a.conclusion(lang)));
    // Tiles: times are written in words ("4 min 31 s") with their reference
    // ("after the start of the release"), never as a bare clock value.
    let after = t("après le début du rejet", "after the start of the release");
    let not_reached = t("dans la durée simulée", "within the simulated time");
    let immediately = t("immédiatement", "immediately");
    let time_tile = |v: Option<f64>, none: String| -> (String, String) {
        match v {
            None => (none, not_reached.clone()),
            Some(v) if v < 1.0 => (immediately.clone(), String::new()),
            Some(v) => (fmt_duration(v, lang), after.clone()),
        }
    };
    let (ta1, sa1) = time_tile(a.t_alarm1, t("non atteinte", "not reached"));
    let (ta2, sa2) = time_tile(a.t_alarm2, t("non atteinte", "not reached"));
    let (t10, s10) = time_tile(a.t_10, t("non atteint", "not reached"));
    let mut kpis = vec![
        (
            t("O₂ minimal", "Minimum O₂"),
            fmt_pct(s.min_o2, 1, lang),
            if s.time_at_min < 1.0 {
                t("atteint immédiatement", "reached immediately")
            } else {
                format!("{} {} {after}", t("atteint", "reached"), fmt_duration(s.time_at_min, lang))
            },
        ),
        (
            t("O₂ près de la fuite", "O₂ next to the release"),
            a.near.map_or_else(|| t("non calculé", "not computed"), |n| fmt_pct(n.min_o2, 1, lang)),
            a.near.map_or_else(String::new, |n| {
                format!("{} {}", t("champ proche, rayon", "near field, radius"), fmt_unit(n.radius_m, 1, "m", lang))
            }),
        ),
        (format!("{} ({})", t("Alarme 1", "Alarm 1"), fmt_pct(a.alarm1, 1, lang)), ta1, sa1),
        (format!("{} ({})", t("Alarme 2", "Alarm 2"), fmt_pct(a.alarm2, 1, lang)), ta2, sa2),
        (t("Danger vital (10 %)", "Life-threatening (10%)"), t10, s10),
    ];
    if let Some(co2) = s.max_co2_percent {
        kpis.push((t("CO₂ maximal", "Peak CO₂"), fmt_pct(co2, 2, lang), String::new()));
        let (tc, sc) = time_tile(a.t_co2_idlh, t("non atteinte", "not reached"));
        kpis.push((t("CO₂ : DIVS 4 %", "CO₂: IDLH 4%"), tc, sc));
    }
    kpis.push((
        t("Volume de gaz libérable", "Releasable gas volume"),
        fmt_unit(s.releasable_volume_m3, 1, "m³", lang),
        String::new(),
    ));
    kpis.push((
        t("Durée de vidange (99 %)", "Release duration (99%)"),
        s.release_duration.map_or_else(|| t("> durée simulée", "> simulated time"), |v| fmt_duration(v, lang)),
        String::new(),
    ));
    h.push_str(r#"<div class="kpis">"#);
    for (l, v, sub) in &kpis {
        let _ = write!(
            h,
            r#"<div class="kpi"><div class="l">{}</div><div class="v">{}</div><div class="s">{}</div></div>"#,
            esc(&typo(l, lang)),
            esc(v),
            esc(&typo(sub, lang))
        );
    }
    h.push_str("</div>");
    // Key messages in plain language
    let _ = write!(h, r#"<div class="key"><b>{}</b><ul>"#, esc(&t("En clair", "In plain terms")));
    for m in key_messages(run, opts.failure.as_ref(), lang) {
        let _ = write!(h, "<li>{}</li>", esc(&m));
    }
    h.push_str("</ul></div>");
    for (level, w) in a.warnings_ranked(lang) {
        let class = match level {
            crate::safety::WarningLevel::Danger => "warn danger",
            crate::safety::WarningLevel::Warning => "warn",
            crate::safety::WarningLevel::Info => "warn info",
        };
        let _ = write!(h, r#"<div class="{class}">{}</div>"#, esc(&w));
    }

    // Charts
    let _ = write!(
        h,
        "<h2>2. {}</h2>",
        esc(&t("Évolution de la concentration en oxygène", "Oxygen concentration over time"))
    );
    // Event-focused chart first when the simulation is much longer than the event.
    let window = s.event_window();
    let long = s.simulated_duration > 3.0 * window;
    if long {
        let _ = write!(
            h,
            "<h3>{} ({})</h3>{}<h3>{} ({})</h3>",
            esc(&t("Zoom sur l'événement", "Event window")),
            esc(&fmt_duration(window, lang)),
            o2_chart_svg(run, lang, 900.0, 380.0, Some(window)),
            esc(&t("Durée complète", "Full duration")),
            esc(&fmt_duration(s.simulated_duration, lang))
        );
        h.push_str(&o2_chart_svg(run, lang, 900.0, 300.0, None));
    } else {
        h.push_str(&o2_chart_svg(run, lang, 900.0, 380.0, None));
    }
    let _ = write!(
        h,
        "<p class=\"meta\">{}</p>",
        esc(&typo(
            &match lang {
                Language::French => format!(
                    "Temps exprimé depuis le début de la fuite. Cas enveloppe sans ventilation (tout l'inventaire libéré d'un coup) : {} O₂ selon la méthode par déplacement INRS ED 155 / BCGA CP 52 (prudente), {} O₂ selon la relation à efflux libre NFPA 2001 / ISO 14520 (meilleure estimation).",
                    fmt_pct(s.instant_displacement_o2, 1, lang),
                    fmt_pct(s.instant_free_efflux_o2, 1, lang)
                ),
                Language::English => format!(
                    "Time from the start of the release. Bounding case without ventilation (whole inventory released at once): {} O₂ with the INRS ED 155 / BCGA CP 52 displacement method (conservative), {} O₂ with the NFPA 2001 / ISO 14520 free-efflux relation (best estimate).",
                    fmt_pct(s.instant_displacement_o2, 1, lang),
                    fmt_pct(s.instant_free_efflux_o2, 1, lang)
                ),
            },
            lang
        ))
    );
    if let Some(peak) = s.max_co2_percent {
        let _ = write!(h, "<h2>2b. {}</h2>", esc(&t("Concentration en CO₂", "CO₂ concentration")));
        let x_end = long.then_some(window);
        if peak > 12.0 {
            // Very high peak: a clipped view keeps the exposure limits legible.
            let _ = write!(
                h,
                "<h3>{}</h3>{}<h3>{}</h3>{}",
                esc(&t("Échelle des valeurs limites (0–10 %)", "Exposure-limit scale (0–10%)")),
                co2_chart_svg(run, lang, 900.0, 300.0, x_end, Some(10.5)).unwrap_or_default(),
                esc(&t("Échelle complète", "Full scale")),
                co2_chart_svg(run, lang, 900.0, 260.0, x_end, None).unwrap_or_default()
            );
        } else {
            h.push_str(&co2_chart_svg(run, lang, 900.0, 300.0, x_end, None).unwrap_or_default());
        }
    }

    // Inputs
    let _ = write!(h, "<h2>3. {}</h2><table>", esc(&t("Données d'entrée", "Input data")));
    row(&mut h, lang, &t("Gaz", "Gas"), c.gas.name(lang));
    let src = &c.source;
    let constant = c.release.kind == ReleaseKind::ConstantFlow;
    let container = if src.count > 1 {
        format!("{} × {}", src.count, src.bottle_type.name(lang))
    } else {
        src.bottle_type.name(lang)
    };
    if !constant {
        row(
            &mut h,
            lang,
            &t("Récipient", "Container"),
            &format!("{container} — {}", fmt_unit(src.volume_liters * f64::from(src.count), 1, "L", lang)),
        );
    }
    if constant {
    } else if c.gas.is_liquefied() {
        row(
            &mut h,
            lang,
            &t("Taux de remplissage CO₂", "CO₂ filling ratio"),
            &fmt_unit(src.co2_fill_ratio, 3, "kg/L", lang),
        );
        row(
            &mut h,
            lang,
            &t("Phase soutirée", "Withdrawal phase"),
            match src.co2_withdrawal {
                Co2Withdrawal::Vapour => lang.pick("Gazeuse (bouteille debout)", "Vapour (upright cylinder)"),
                Co2Withdrawal::Liquid => {
                    lang.pick("Liquide (tube plongeur / bouteille inversée)", "Liquid (dip tube / inverted)")
                }
            },
        );
    } else {
        row(
            &mut h,
            lang,
            &t("Pression initiale (manomètre)", "Initial pressure (gauge)"),
            &fmt_unit(src.pressure_bar_g, 0, "bar", lang),
        );
    }
    if !constant {
        row(
            &mut h,
            lang,
            &t("Température du récipient", "Container temperature"),
            &fmt_unit(src.temperature_c, 0, "°C", lang),
        );
    }
    row(
        &mut h,
        lang,
        &t("Inventaire initial", "Initial inventory"),
        &format!(
            "{} ({} {})",
            fmt_unit(s.initial_mass, 2, "kg", lang),
            fmt_unit(s.releasable_volume_m3, 2, "m³", lang),
            t("de gaz libérable", "of releasable gas")
        ),
    );
    row(&mut h, lang, &t("Scénario de rejet", "Release scenario"), c.release.kind.name(lang));
    if constant {
        row(
            &mut h,
            lang,
            &t("Débit de gaz imposé", "Imposed gas flow"),
            &format!(
                "{} {} {}",
                fmt_unit(c.release.constant_flow_m3h, 3, "m³/h", lang),
                t("pendant", "for"),
                fmt_duration(c.release.constant_duration_min * 60.0, lang)
            ),
        );
    }
    if c.release.kind.uses_orifice() {
        row(
            &mut h,
            lang,
            &t("Diamètre équivalent", "Equivalent diameter"),
            &fmt_unit(c.release.diameter_mm, 2, "mm", lang),
        );
        row(
            &mut h,
            lang,
            &t("Coefficient de décharge Cd", "Discharge coefficient Cd"),
            &fmt_num(c.release.discharge_coefficient, 2, lang),
        );
        let regime = match s.initial_regime {
            FlowRegime::Choked => t("sonique (critique)", "choked (sonic)"),
            FlowRegime::Subcritical => t("subsonique", "subsonic"),
            FlowRegime::Liquid => t("liquide", "liquid"),
            FlowRegime::None => "—".to_string(),
        };
        row(
            &mut h,
            lang,
            &t("Débit initial", "Initial flow"),
            &format!(
                "{} — {} ({regime})",
                fmt_unit(s.initial_mass_flow * 3600.0, 1, "kg/h", lang),
                fmt_unit(s.initial_volume_flow_m3h, 1, "m³/h", lang)
            ),
        );
        row(
            &mut h,
            lang,
            &t("Temps de vidange à 90 % / 99 %", "Time to release 90% / 99%"),
            &format!("{} / {}", opt_time(s.time_90_percent, lang), opt_time(s.release_duration, lang)),
        );
        row(
            &mut h,
            lang,
            &t("Modèle de vidange", "Blowdown model"),
            match c.model.blowdown {
                crate::config::Blowdown::Isothermal => lang.pick("isotherme (prudent)", "isothermal (conservative)"),
                crate::config::Blowdown::Adiabatic => lang.pick("adiabatique", "adiabatic"),
            },
        );
    }
    let room = &c.room;
    row(&mut h, lang, &t("Volume libre du local", "Free room volume"), &fmt_unit(room.volume_m3, 1, "m³", lang));
    row(&mut h, lang, &t("Hauteur sous plafond", "Ceiling height"), &fmt_unit(room.height_m, 2, "m", lang));
    row(
        &mut h,
        lang,
        &t("Ventilation mécanique", "Mechanical ventilation"),
        &if room.mechanical_enabled {
            format!("{} — {}", fmt_unit(room.mechanical_flow_m3h, 0, "m³/h", lang), room.mechanical_mode.name(lang))
        } else {
            t("à l'arrêt / en panne", "off / failed")
        },
    );
    row(
        &mut h,
        lang,
        &t("Renouvellement naturel (infiltrations)", "Natural air change (infiltration)"),
        &fmt_unit(room.natural_ach, 2, "vol/h", lang),
    );
    row(
        &mut h,
        lang,
        &t("Efficacité de ventilation η", "Ventilation effectiveness η"),
        &fmt_num(room.ventilation_efficiency, 2, lang),
    );
    let timeline = c.timeline();
    if !timeline.is_empty() {
        row(
            &mut h,
            lang,
            &t("Événements pendant le rejet", "Events during the release"),
            &timeline.iter().map(|e| e.describe(lang)).collect::<Vec<_>>().join(" ; "),
        );
    }
    row(
        &mut h,
        lang,
        &t("Pression partielle d'O₂ (ambiante → minimale)", "O₂ partial pressure (ambient → minimum)"),
        &format!("{} → {}", fmt_unit(a.po2_ambient_kpa, 1, "kPa", lang), fmt_unit(a.po2_min_kpa, 1, "kPa", lang)),
    );
    row(
        &mut h,
        lang,
        &t("Ventilation effective η·Q", "Effective ventilation η·Q"),
        &format!(
            "{} ({})",
            fmt_unit(s.effective_ventilation_m3h, usize::from(s.effective_ventilation_m3h < 100.0), "m³/h", lang),
            fmt_unit(s.effective_ventilation_m3h / room.volume_m3, 2, "vol/h", lang)
        ),
    );
    let strat = match c.model.stratification {
        StratificationMode::Auto => t("automatique", "automatic"),
        StratificationMode::On => t("activé", "enabled"),
        StratificationMode::Off => t("désactivé", "disabled"),
    };
    row(
        &mut h,
        lang,
        &t("Modèle bi-zone", "Two-zone model"),
        &format!("{strat} — {}", if s.two_zone { t("utilisé", "used") } else { t("non utilisé", "not used") }),
    );
    if s.two_zone {
        let ext = match c.model.extraction {
            ExtractionLocation::Distributed => t("répartie", "distributed"),
            ExtractionLocation::AccumulationZone => t("dans la zone d'accumulation", "in the accumulation zone"),
            ExtractionLocation::OppositeZone => {
                t("opposée à la zone d'accumulation", "opposite to the accumulation zone")
            }
        };
        row(
            &mut h,
            lang,
            &t("Paramètres bi-zone", "Two-zone parameters"),
            &format!(
                "{} {} ; {} {} ; {} {} ; {} {}",
                t("couche", "layer"),
                fmt_unit(c.model.zone_height_m, 2, "m", lang),
                t("injection", "injection"),
                fmt_pct(c.model.injection_fraction * 100.0, 0, lang),
                t("échange", "exchange"),
                fmt_unit(c.model.exchange_ach, 1, "vol/h", lang),
                t("extraction", "extraction"),
                ext
            ),
        );
    }
    row(&mut h, lang, &t("Série qui fixe le minimum d'O₂", "Series setting the minimum O₂"), s.governing.label(lang));
    row(
        &mut h,
        lang,
        &t("Champ proche (exposition à la source)", "Near field (exposure at the source)"),
        &match a.near {
            Some(n) => format!(
                "{} {} ; {} {} ; {} {} ; {}",
                t("demi-sphère de rayon", "hemisphere of radius"),
                fmt_unit(n.radius_m, 1, "m", lang),
                t("vitesse d'air", "air speed"),
                fmt_unit(n.air_speed_m_s, 2, "m/s", lang),
                t("échange β", "exchange β"),
                fmt_unit(s.near_field_exchange_m3h.unwrap_or(0.0), 0, "m³/h", lang),
                if n.in_assessment {
                    t("retenu pour le classement", "included in the classification")
                } else {
                    t("indicatif, hors classement", "indicative, not in the classification")
                }
            ),
            None => t("non calculé", "not computed"),
        },
    );
    row(
        &mut h,
        lang,
        &t("Seuils d'alarme O₂", "O₂ alarm thresholds"),
        &format!(
            "{} — {} / {}",
            c.thresholds.preset.name(lang),
            fmt_pct(a.alarm1, 1, lang),
            fmt_pct(a.alarm2, 1, lang)
        ),
    );
    h.push_str("</table>");

    // Crossings
    let _ = write!(
        h,
        "<h2>4. {}</h2><table class=\"grid\"><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>",
        esc(&t("Franchissement des seuils O₂", "O₂ threshold crossings")),
        esc(&t("Seuil O₂", "O₂ level")),
        esc(&t("Premier franchissement", "First crossing")),
        esc(&t("Retour au-dessus", "Back above")),
        esc(&t("Durée cumulée en dessous", "Total time below"))
    );
    for x in &s.o2_crossings {
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&fmt_pct(x.level, 1, lang)),
            esc(&opt_time(x.first, lang)),
            esc(&x.back.map_or_else(
                || if x.censored {
                    t("non observé avant la fin du calcul", "not observed before the end of the computation")
                } else {
                    "—".into()
                },
                fmt_clock
            )),
            esc(&if x.censored {
                format!("≥ {}", fmt_duration(x.total, lang))
            } else if x.reached() {
                fmt_duration(x.total, lang)
            } else {
                "—".into()
            })
        );
    }
    h.push_str("</table>");
    if let Some(n) = a.near {
        let _ = write!(
            h,
            "<h3>{}</h3><table class=\"grid\"><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>",
            esc(&typo(
                &format!(
                    "{} ({} {})",
                    t("Près de la fuite — champ proche", "Next to the release — near field"),
                    t("demi-sphère de", "hemisphere of"),
                    fmt_unit(n.radius_m, 1, "m", lang)
                ),
                lang
            )),
            esc(&t("Seuil O₂", "O₂ level")),
            esc(&t("Premier franchissement", "First crossing")),
            esc(&t("Retour au-dessus", "Back above")),
            esc(&t("Durée cumulée en dessous", "Total time below"))
        );
        for x in
            s.near_crossings.iter().filter(|x| [a.alarm1, a.alarm2, 10.0].iter().any(|l| (l - x.level).abs() < 1e-9))
        {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&fmt_pct(x.level, 1, lang)),
                esc(&opt_time(x.first, lang)),
                esc(&x.back.map_or_else(
                    || if x.censored {
                        t("non observé avant la fin du calcul", "not observed before the end of the computation")
                    } else {
                        "—".into()
                    },
                    fmt_clock
                )),
                esc(&if x.censored {
                    format!("≥ {}", fmt_duration(x.total, lang))
                } else if x.reached() {
                    fmt_duration(x.total, lang)
                } else {
                    "—".into()
                })
            );
        }
        h.push_str("</table>");
        let _ = write!(
            h,
            "<p class=\"meta\">{}</p>",
            esc(&t(
                "Modèle champ proche / champ lointain (Nicas 1996, AIHA IH Mod, ODHMOD) : moyenne d'une demi-sphère autour de la fuite, rejet à faible vitesse supposé. Pour un jet sous pression, l'entraînement d'air n'est pas résolu : la moyenne réelle est sans doute plus faible, mais la concentration dans l'axe du jet peut être bien plus élevée. Ce n'est pas une borne en tout point.",
                "Near-field / far-field model (Nicas 1996, AIHA IH Mod, ODHMOD): average over a hemisphere around the leak, low-momentum release assumed. For a pressurised jet, air entrainment is not resolved: the real average is probably lower, but the concentration in the jet axis can be much higher. It is not a bound at every point.",
            ))
        );
    }
    if !s.co2_crossings.is_empty() {
        let _ = write!(
            h,
            "<table class=\"grid\"><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>",
            esc(&t("Seuil CO₂", "CO₂ level")),
            esc(&t("Premier dépassement", "First exceedance")),
            esc(&t("Retour en dessous", "Back below")),
            esc(&t("Durée cumulée au-dessus", "Total time above"))
        );
        for x in &s.co2_crossings {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&fmt_pct(x.level, 1, lang)),
                esc(&opt_time(x.first, lang)),
                esc(&x.back.map_or_else(|| "—".into(), fmt_clock)),
                esc(&if x.censored {
                    format!("≥ {}", fmt_duration(x.total, lang))
                } else if x.reached() {
                    fmt_duration(x.total, lang)
                } else {
                    "—".into()
                })
            );
        }
        h.push_str("</table>");
    }
    if let Some(m) = a.escape_margin {
        let _ = write!(
            h,
            "<p>{}</p>",
            esc(&typo(
                &match lang {
                    Language::French => format!("Délai indicatif entre l'alarme 1 et le niveau dangereux suivant : {}. Ce délai doit couvrir le temps de réponse du détecteur et le temps d'évacuation.", fmt_duration(m, lang)),
                    Language::English => format!("Indicative time between alarm 1 and the next dangerous level: {}. It must cover the detector response time and the egress time.", fmt_duration(m, lang)),
                },
                lang
            ))
        );
    }

    let mut section = 5;
    if let Some(sz) = opts.sizing {
        let _ = write!(h, "<h2>{section}. {}</h2><table>", esc(&t("Dimensionnement", "Sizing")));
        section += 1;
        let crit = fmt_pct(sz.criterion_o2, 1, lang);
        let fmt_sizing = |v: Sizing, unit: &str, dec: usize| {
            match v {
            Sizing::AlreadyMet => t("critère déjà respecté", "criterion already met"),
            Sizing::Value(x) => fmt_unit(x, dec, unit, lang),
            Sizing::NotAchievable => t(
                "aucun débit raisonnable ne suffit : agir à la source (inventaire, limiteur de débit, stockage extérieur)",
                "no reasonable flow is sufficient: act at the source (inventory, flow limiter, outdoor storage)",
            ),
            Sizing::Undetermined => t(
                "non démontré : l'horizon de calcul ne couvre pas l'événement, même prolongé à 168 h",
                "not demonstrated: the computation horizon does not cover the event, even extended to 168 h",
            ),
        }
        };
        let flow_label = match sz.mode {
            VentilationMode::Extraction => {
                t("Débit d'extraction mécanique nécessaire", "Required mechanical extraction flow")
            }
            VentilationMode::Supply => {
                t("Débit de soufflage (air neuf) nécessaire", "Required supply (fresh-air) flow")
            }
        };
        row(&mut h, lang, &format!("{flow_label} (O₂ ≥ {crit})"), &fmt_sizing(sz.required_ventilation, "m³/h", 0));
        row(
            &mut h,
            lang,
            &t("Mode de ventilation du dimensionnement", "Ventilation mode of the sizing"),
            &match sz.mode {
                VentilationMode::Extraction => t(
                    "extraction (l'air neuf entrant vaut le débit extrait moins le débit de gaz)",
                    "extraction (fresh air in = extracted flow minus the gas flow)",
                ),
                VentilationMode::Supply => {
                    t("soufflage (le débit soufflé est de l'air neuf)", "supply (the supplied flow is fresh air)")
                }
            },
        );
        row(
            &mut h,
            lang,
            &format!("{} (O₂ ≥ {crit})", t("Diamètre de fuite maximal tolérable", "Maximum tolerable leak diameter")),
            &fmt_sizing(sz.max_diameter, "mm", 2),
        );
        h.push_str("</table>");
    }

    // Ventilation failure check
    if let Some(f) = &opts.failure {
        let _ = write!(
            h,
            "<h2>{section}. {}</h2><p>{}</p><table class=\"grid\"><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>",
            esc(&t("Panne de ventilation", "Ventilation failure")),
            esc(&t(
                "Même scénario avec la ventilation mécanique en service puis en panne (seules les infiltrations subsistent). Temps comptés depuis le début du rejet.",
                "Same scenario with the mechanical ventilation running, then failed (only infiltration remains). Times from the start of the release.",
            )),
            esc(&t("Variante", "Variant")),
            esc(&t("O₂ minimal", "Minimum O₂")),
            esc(&format!("{} ({})", t("Alarme 1", "Alarm 1"), fmt_pct(a.alarm1, 1, lang))),
            esc(&format!("{} ({})", t("Alarme 2", "Alarm 2"), fmt_pct(a.alarm2, 1, lang))),
            esc(&t("Durée sous l'alarme 1", "Time below alarm 1")),
            esc(&t("Retour > 19,5 %", "Back > 19.5%"))
        );
        section += 1;
        let first = |x: &Summary, l: f64| opt_time(x.o2_crossing(l).and_then(|c| c.first), lang);
        let below = |x: &Summary| {
            x.o2_crossing(a.alarm1).filter(|c| c.reached()).map_or_else(
                || "—".into(),
                |c| format!("{}{}", if c.censored { "≥ " } else { "" }, fmt_duration(c.total, lang)),
            )
        };
        let back = |x: &Summary| match x.o2_crossing(19.5) {
            Some(c) if c.reached() => c.back.map_or_else(
                || t("non observé avant la fin du calcul", "not observed before the end of the computation"),
                fmt_clock,
            ),
            _ => "—".into(),
        };
        for (label, x) in [
            (t("Ventilation en service", "Ventilation running"), &f.running),
            (t("Panne de ventilation", "Ventilation failure"), &f.failed),
        ] {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&label),
                esc(&fmt_pct(x.min_o2, 1, lang)),
                esc(&first(x, a.alarm1)),
                esc(&first(x, a.alarm2)),
                esc(&below(x)),
                esc(&back(x))
            );
        }
        h.push_str("</table>");
        let dur_text = |x: &Summary| x.o2_crossing(a.alarm1).map_or_else(|| "—".to_string(), |c| c.total_text(lang));
        let _ = write!(
            h,
            "<p>{}</p>",
            esc(&typo(
                &format!(
                    "{} {} → {} ; {} {} → {}.",
                    t("O₂ minimal :", "Minimum O₂:"),
                    fmt_pct(f.running.min_o2, 1, lang),
                    fmt_pct(f.failed.min_o2, 1, lang),
                    t("durée sous l'alarme 1 :", "time below alarm 1:"),
                    dur_text(&f.running),
                    dur_text(&f.failed)
                ),
                lang
            ))
        );
    }

    // Before / after study
    if let Some(Err(cause)) = &opts.before_after {
        let _ = write!(
            h,
            "<h2>{section}. {}</h2><div class=\"warn danger\">{} {}</div>",
            esc(&t(
                "Étude avant / après modification de l'installation",
                "Before / after study of the installation change"
            )),
            esc(&t(
                "Étude demandée mais impossible à calculer : la configuration future n'est pas valide.",
                "Study requested but impossible to compute: the future configuration is not valid.",
            )),
            esc(cause)
        );
        section += 1;
    }
    if let Some(Ok(ba)) = &opts.before_after {
        let _ = write!(
            h,
            "<h2>{section}. {}</h2>",
            esc(&t(
                "Étude avant / après modification de l'installation",
                "Before / after study of the installation change"
            ))
        );
        section += 1;
        if !ba.label.is_empty() {
            let _ = write!(h, "<p><b>{}</b></p>", esc(&typo(&ba.label, lang)));
        }
        let _ = write!(
            h,
            "<p>{} {}</p>",
            esc(&t("Modifications étudiées :", "Changes studied:")),
            esc(&typo(&ba.differences.join(" ; "), lang))
        );
        let _ = write!(
            h,
            "<table class=\"grid\"><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>",
            esc(&t("Configuration", "Configuration")),
            esc(&t("O₂ minimal", "Minimum O₂")),
            esc(&format!("{} ({})", t("Alarme 1", "Alarm 1"), fmt_pct(a.alarm1, 1, lang))),
            esc(&format!("{} ({})", t("Alarme 2", "Alarm 2"), fmt_pct(a.alarm2, 1, lang))),
            esc(&t("Durée sous l'alarme 1", "Time below alarm 1")),
            esc(&t("Retour > 19,5 %", "Back > 19.5%")),
            esc(&t("Série qui fixe le minimum", "Series setting the minimum"))
        );
        let first = |x: &Summary, l: f64| opt_time(x.o2_crossing(l).and_then(|c| c.first), lang);
        let below = |x: &Summary| {
            x.o2_crossing(a.alarm1).filter(|c| c.reached()).map_or_else(
                || "—".into(),
                |c| format!("{}{}", if c.censored { "≥ " } else { "" }, fmt_duration(c.total, lang)),
            )
        };
        let back = |x: &Summary| match x.o2_crossing(19.5) {
            Some(c) if c.reached() => c.back.map_or_else(
                || t("non observé avant la fin du calcul", "not observed before the end of the computation"),
                fmt_clock,
            ),
            _ => "—".into(),
        };
        for (label, x) in &ba.cases {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(label),
                esc(&fmt_pct(x.min_o2, 1, lang)),
                esc(&first(x, a.alarm1)),
                esc(&first(x, a.alarm2)),
                esc(&below(x)),
                esc(&back(x)),
                esc(x.governing.label(lang))
            );
        }
        h.push_str("</table>");
        if ba.cases.len() == 4 {
            let (cur_on, cur_off, fut_on, fut_off) = (&ba.cases[0].1, &ba.cases[1].1, &ba.cases[2].1, &ba.cases[3].1);
            let _ = write!(
                h,
                "<p>{}</p>",
                esc(&typo(
                    &match lang {
                        Language::French => format!(
                            "Après modification, l'O₂ minimal passe de {} à {} avec la ventilation en service, et de {} à {} en cas de panne. L'hypothèse de fuite est celle du scénario ; l'ajout de récipients n'implique pas qu'ils fuient tous à la fois.",
                            fmt_pct(cur_on.min_o2, 1, lang),
                            fmt_pct(fut_on.min_o2, 1, lang),
                            fmt_pct(cur_off.min_o2, 1, lang),
                            fmt_pct(fut_off.min_o2, 1, lang)
                        ),
                        Language::English => format!(
                            "After the change, the minimum O₂ goes from {} to {} with the ventilation running, and from {} to {} in case of failure. The leak hypothesis is the one of the scenario; adding containers does not imply that they all leak at once.",
                            fmt_pct(cur_on.min_o2, 1, lang),
                            fmt_pct(fut_on.min_o2, 1, lang),
                            fmt_pct(cur_off.min_o2, 1, lang),
                            fmt_pct(fut_off.min_o2, 1, lang)
                        ),
                    },
                    lang
                ))
            );
        }
    }

    // Recommendations: proposed by the software, retained by the author
    let recs = a.recommendations(lang);
    let (retained, excluded): (Vec<_>, Vec<_>) = recs.iter().partition(|r| c.report.is_retained(r.id));
    let intro = match lang {
        Language::French => format!(
            "Mesures proposées par le logiciel à partir du scénario et retenues par le rédacteur ({} sur {}). Les propositions non retenues figurent en fin de section avec sa justification.",
            retained.len(),
            recs.len()
        ),
        Language::English => format!(
            "Measures proposed by the software from the scenario and retained by the author ({} of {}). Proposals not retained are listed at the end of the section with the author's justification.",
            retained.len(),
            recs.len()
        ),
    };
    let _ = write!(
        h,
        "<h2>{section}. {}</h2><p class=\"meta\">{}</p>",
        esc(&t("Recommandations", "Recommendations")),
        esc(&typo(&intro, lang))
    );
    section += 1;
    for r in &retained {
        let _ = write!(
            h,
            r#"<div class="rec {:?}"><div class="p">{}</div><b>{}</b><br>{}"#,
            r.priority,
            esc(r.priority.label(lang)),
            esc(&r.title),
            esc(&r.text)
        );
        let note = c.report.note(r.id);
        if !note.trim().is_empty() {
            let _ = write!(
                h,
                r#"<div class="note">{} {}</div>"#,
                esc(&t("Commentaire du rédacteur :", "Author's comment:")),
                esc(&typo(note, lang))
            );
        }
        h.push_str("</div>");
    }
    if !c.report.additional.trim().is_empty() {
        let _ = write!(
            h,
            r#"<div class="rec Medium"><div class="p">{}</div><b>{}</b><br>{}</div>"#,
            esc(&t("Rédacteur", "Author")),
            esc(&t("Mesures complémentaires", "Additional measures")),
            esc(&typo(c.report.additional.trim(), lang)).replace('\n', "<br>")
        );
    }
    if !excluded.is_empty() {
        let _ = write!(
            h,
            "<h3>{}</h3><table class=\"grid\"><tr><th>{}</th><th>{}</th><th>{}</th></tr>",
            esc(&t("Propositions non retenues par le rédacteur", "Proposals not retained by the author")),
            esc(&t("Proposition", "Proposal")),
            esc(&t("Priorité proposée", "Proposed priority")),
            esc(&t("Justification", "Justification"))
        );
        for r in &excluded {
            let note = c.report.note(r.id);
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&r.title),
                esc(r.priority.label(lang)),
                esc(&if note.trim().is_empty() { "—".to_string() } else { typo(note, lang) })
            );
        }
        h.push_str("</table>");
    }

    // Reference tables
    let _ = write!(
        h,
        "<h2>{section}. {}</h2><table class=\"grid\"><tr><th>{}</th><th>{}</th></tr>",
        esc(&t("Repères physiologiques", "Physiological reference")),
        esc(&t("O₂ (% vol)", "O₂ (% vol)")),
        esc(&t("Effets / statut", "Effects / status"))
    );
    section += 1;
    for (k, v) in o2_effects_table(lang) {
        let k = if lang == Language::English { k.replace(',', ".") } else { k.to_string() };
        let _ = write!(h, "<tr><td>{}</td><td>{}</td></tr>", esc(&k), esc(&typo(v, lang)));
    }
    h.push_str("</table>");
    let _ = write!(
        h,
        "<p class=\"meta\">{}</p>",
        esc(&t(
            "L'anoxie ne provoque aucune sensation d'étouffement : la respiration est régulée par le CO₂, pas par l'O₂. Une à deux inspirations de gaz inerte pur peuvent entraîner une perte de connaissance immédiate.",
            "Anoxia causes no feeling of suffocation: breathing is driven by CO₂, not O₂. One or two breaths of pure inert gas can cause immediate collapse."
        ))
    );
    if c.gas.is_toxic() {
        let _ = write!(
            h,
            "<table class=\"grid\"><tr><th>{}</th><th>{}</th></tr>",
            esc(&t("CO₂ (% vol)", "CO₂ (% vol)")),
            esc(&t("Effets / valeur limite", "Effects / limit value"))
        );
        for (k, v) in co2_effects_table(lang) {
            let k = if lang == Language::English { k.replace(',', ".") } else { k.to_string() };
            let _ = write!(h, "<tr><td>{}</td><td>{}</td></tr>", esc(&k), esc(&typo(v, lang)));
        }
        h.push_str("</table>");
    }

    // Model
    let _ = write!(
        h,
        "<h2>{section}. {}</h2><ul>",
        esc(&t("Hypothèses et limites du modèle", "Model assumptions and limitations"))
    );
    section += 1;
    for l in model_limitations(lang) {
        let _ = write!(h, "<li>{}</li>", esc(&l));
    }
    h.push_str("</ul>");
    let _ = write!(
        h,
        "<p class=\"meta\">{}</p>",
        esc(&t(
            "Méthodes : écoulement isentropique par orifice (sonique/subsonique), gaz réel (table NIST REFPROP, Peng-Robinson / viriel hors table), courbe de saturation CO₂ de Span & Wagner (1996), bilan de masse bien mélangé intégré exactement. Références : INRS ED 967, EIGA Doc. 44, OSHA 29 CFR 1910.146, NIOSH Pocket Guide, Code du travail R.4412-149, TNO Yellow Book (CPR 14E).",
            "Methods: isentropic orifice flow (choked/subsonic), real gas (NIST REFPROP table, Peng–Robinson / virial outside the table), Span & Wagner (1996) CO₂ saturation curve, exactly integrated well-mixed mass balance. References: INRS ED 967, EIGA Doc. 44, OSHA 29 CFR 1910.146, NIOSH Pocket Guide, French Labour Code R.4412-149, TNO Yellow Book (CPR 14E)."
        ))
    );

    if !id.comments.trim().is_empty() {
        let _ = write!(
            h,
            "<h2>{section}. {}</h2><p>{}</p>",
            esc(&t("Commentaires", "Comments")),
            esc(&id.comments).replace('\n', "<br>")
        );
        section += 1;
    }

    // Signature
    let _ = write!(
        h,
        "<h2>{section}. {}</h2><table class=\"grid sign\"><tr><th></th><th>{}</th><th>{}</th><th>{}</th></tr><tr><td><b>{}</b></td><td>{}</td><td>{}</td><td></td></tr><tr><td><b>{}</b></td><td>{}</td><td></td><td></td></tr></table>",
        esc(&t("Validation", "Approval")),
        esc(&t("Nom", "Name")),
        esc(&t("Date", "Date")),
        esc(&t("Visa", "Signature")),
        esc(&t("Rédigé par", "Prepared by")),
        esc(&id.user),
        esc(&opts.generated_at),
        esc(&t("Vérifié par", "Reviewed by")),
        esc(&id.reviewer)
    );

    let _ = write!(
        h,
        "<footer>{}<br>{} O₂ {} = {}.<br>{} <a href=\"{}\">{}</a>.</footer></body></html>",
        esc(&disclaimer(lang)),
        esc(&t("Référence", "Reference")),
        esc(&t("ambiant", "ambient")),
        esc(&fmt_pct(O2_AMBIENT_PERCENT, 2, lang)),
        esc(&format!(
            "Anoxia Simulator v{} — {} {} —",
            crate::VERSION,
            t("logiciel libre (AGPL-3.0) de", "free software (AGPL-3.0) by"),
            crate::AUTHOR
        )),
        esc(crate::REPOSITORY_URL),
        esc(crate::REPOSITORY_URL.trim_start_matches("https://"))
    );
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Configuration;
    use crate::gas::GasType;

    #[test]
    fn report_is_self_contained_and_escaped() {
        let mut c = Configuration::default();
        c.identification.unit = "<script>alert('x')</script>".into();
        c.identification.comments = "Ligne 1\nLigne 2 & co".into();
        let run = crate::run(&c).unwrap();
        for lang in [Language::French, Language::English] {
            let html = html(
                &run,
                &ReportOptions {
                    lang,
                    generated_at: "2026-11-01 10:00".into(),
                    sizing: None,
                    failure: None,
                    before_after: None,
                },
            );
            assert!(!html.contains("<script>"));
            assert!(html.contains("&lt;script&gt;"));
            assert!(!html.contains("http://cdn") && !html.contains("https://cdn"));
            assert!(html.contains("<svg"));
            assert!(html.contains("</html>"));
        }
    }

    #[test]
    fn english_report_has_no_french_recommendations() {
        let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        c.room.volume_m3 = 15.0;
        let run = crate::run(&c).unwrap();
        let html = html(
            &run,
            &ReportOptions {
                lang: Language::English,
                generated_at: String::new(),
                sizing: None,
                failure: None,
                before_after: None,
            },
        );
        assert!(!html.contains("Évacuation"));
        assert!(html.contains("Never enter to rescue"));
    }

    #[test]
    fn co2_report_has_co2_chart() {
        let c = Configuration { gas: GasType::CarbonDioxide, ..Configuration::default() };
        let run = crate::run(&c).unwrap();
        let html = html(
            &run,
            &ReportOptions {
                lang: Language::French,
                generated_at: String::new(),
                sizing: None,
                failure: None,
                before_after: None,
            },
        );
        assert!(html.contains("DIVS"));
        // O₂ (event + full duration) and CO₂ (limits + full scale) charts
        assert!(html.matches("<svg").count() >= 2);
        assert!(html.contains("0–10"));
    }

    #[test]
    fn sizing_and_failure_sections_are_included() {
        let mut c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        c.source.count = 2;
        c.room.volume_m3 = 35.0;
        c.room.mechanical_enabled = true;
        c.room.mechanical_flow_m3h = 150.0;
        let run = crate::run(&c).unwrap();
        let sizing = SizingSummary::compute(&c, 19.0).unwrap();
        assert!(matches!(sizing.required_ventilation, Sizing::Value(q) if q > 1000.0));
        assert!(matches!(sizing.max_diameter, Sizing::Value(d) if d > 0.1 && d < c.release.diameter_mm));
        let failure = FailureSummary::compute(&c).expect("mechanical ventilation present");
        assert!(failure.failed.min_o2 <= failure.running.min_o2 + 1e-9);
        let opts = ReportOptions {
            lang: Language::French,
            generated_at: String::new(),
            sizing: Some(sizing),
            failure: Some(failure),
            before_after: None,
        };
        let html = html(&run, &opts);
        assert!(html.contains("Dimensionnement"));
        assert!(html.contains("Diamètre de fuite maximal tolérable"));
        assert!(html.contains("Panne de ventilation"));
        assert!(html.contains("Ventilation en service"));
        // No mechanical ventilation: nothing to lose, no failure check
        c.room.mechanical_enabled = false;
        assert!(FailureSummary::compute(&c).is_none());
    }

    #[test]
    fn key_messages_are_plain_sentences() {
        let mut c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        c.source.count = 2;
        c.room.volume_m3 = 35.0;
        c.room.mechanical_enabled = true;
        c.room.mechanical_flow_m3h = 150.0;
        let run = crate::run(&c).unwrap();
        let failure = FailureSummary::compute(&c).unwrap();
        let fr = key_messages(&run, Some(&failure), Language::French);
        assert!(fr.len() >= 4, "{fr:?}");
        assert!(fr[0].contains("alarme 1") && fr[0].contains("après le début du rejet"), "{}", fr[0]);
        assert!(fr.iter().any(|m| m.contains("panne de la ventilation")), "{fr:?}");
        assert!(fr.iter().all(|m| !m.contains("00:")), "no clock values: {fr:?}");
        let en = key_messages(&run, None, Language::English);
        assert!(en[0].contains("after the start of the release"), "{}", en[0]);
        assert!(en.iter().all(|m| !m.contains("panne")));
        // A harmless scenario says so.
        let mut quiet = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        quiet.room.volume_m3 = 500.0;
        quiet.release.diameter_mm = 0.5;
        let run = crate::run(&quiet).unwrap();
        let msgs = key_messages(&run, None, Language::French);
        assert!(msgs[0].starts_with("Aucun seuil d'alarme"), "{}", msgs[0]);
        // Instantaneous release: no "0 s after the start", but "immediately".
        let mut bound = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        bound.source.count = 4;
        bound.room.volume_m3 = 30.0;
        bound.release.kind = crate::config::ReleaseKind::Instantaneous;
        let run = crate::run(&bound).unwrap();
        let msgs = key_messages(&run, None, Language::French);
        assert!(msgs.iter().any(|m| m.contains("immédiatement")), "{msgs:?}");
        assert!(msgs.iter().all(|m| !m.contains("0 s après")), "{msgs:?}");
    }

    #[test]
    fn report_respects_the_authors_recommendation_choices() {
        let mut c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        c.room.volume_m3 = 35.0;
        c.report.set_retained("o2_detection", false);
        c.report.set_note("o2_detection", "Détection existante (3 capteurs), vérifiée en janvier.");
        c.report.set_note("training", "Formation annuelle en place.");
        c.report.additional = "Consigne affichée à l'entrée.\nContrôle trimestriel des raccords.".into();
        let run = crate::run(&c).unwrap();
        let recs = run.assessment.recommendations(Language::French);
        assert!(recs.iter().any(|r| r.id == "o2_detection"));
        assert!(recs.iter().any(|r| r.id == "training"));
        let opts = ReportOptions {
            lang: Language::French,
            generated_at: String::new(),
            sizing: None,
            failure: None,
            before_after: None,
        };
        let html = html(&run, &opts);
        assert!(html.contains("Propositions non retenues par le rédacteur"));
        assert!(html.contains("Détection existante (3 capteurs)"));
        assert!(html.contains("Formation annuelle en place."));
        // (the apostrophe is typographically converted and HTML-escaped)
        assert!(html.contains("Consigne affichée à l"));
        assert!(html.contains("<br>Contrôle trimestriel des raccords."));
        assert!(html.contains(&format!("({} sur {})", recs.len() - 1, recs.len())));
        // Tiles never show bare clock values
        assert!(html.contains("après le début du rejet"));
        assert!(html.contains("En clair"));
    }

    #[test]
    fn before_after_study_compares_four_configurations() {
        let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        c.room.volume_m3 = 60.0;
        c.room.mechanical_enabled = true;
        c.room.mechanical_flow_m3h = 200.0;
        // Not enabled: nothing
        assert!(BeforeAfterSummary::compute(&c, Language::French).is_none());
        c.future.enabled = true;
        assert!(BeforeAfterSummary::compute(&c, Language::French).is_none(), "no change defined");
        c.future.source_count = Some(3);
        c.future.label = "Deux B50 supplémentaires".into();
        let ba = BeforeAfterSummary::compute(&c, Language::French).unwrap().expect("valid future");
        assert_eq!(ba.cases.len(), 4);
        let min = |i: usize| ba.cases[i].1.min_o2;
        // More inventory: never better; failure: never better
        assert!(min(2) <= min(0) + 1e-9 && min(3) <= min(1) + 1e-9);
        assert!(min(1) <= min(0) + 1e-9 && min(3) <= min(2) + 1e-9);
        assert_eq!(ba.differences.len(), 1);
        assert!(ba.differences[0].contains("1 → 3"));
        let run = crate::run(&c).unwrap();
        let opts = ReportOptions {
            lang: Language::French,
            generated_at: String::new(),
            sizing: None,
            failure: None,
            before_after: Some(Ok(ba)),
        };
        let html = html(&run, &opts);
        assert!(html.contains("Étude avant / après"));
        assert!(html.contains("Futur – panne de ventilation"));
        assert!(html.contains("Deux B50 supplémentaires"));
        // An impossible future (101 containers) is reported, not silently dropped
        c.future.source_count = Some(101);
        let bad = BeforeAfterSummary::compute(&c, Language::French).unwrap();
        let cause = bad.clone().expect_err("invalid future");
        assert!(cause.contains("source.count"), "{cause}");
        let opts = ReportOptions {
            lang: Language::French,
            generated_at: String::new(),
            sizing: None,
            failure: None,
            before_after: Some(bad),
        };
        let html2 = super::html(&run, &opts);
        assert!(html2.contains("impossible à calculer"));
        assert!(html2.contains("source.count"));
    }

    #[test]
    fn key_messages_say_at_least_for_censored_durations() {
        // Instantaneous release, 60 m³, 1 000 m³/h, 0.01 vol/h, 6 min horizon:
        // the failed case is still below 19 % at the end of the computation.
        let mut c = Configuration { gas: GasType::Nitrogen, ..Configuration::default() };
        c.release.kind = ReleaseKind::Instantaneous;
        c.room.volume_m3 = 60.0;
        c.room.mechanical_enabled = true;
        c.room.mechanical_flow_m3h = 1000.0;
        c.room.natural_ach = 0.01;
        c.room.ventilation_efficiency = 1.0;
        c.model.stratification = StratificationMode::Off;
        c.model.max_duration_h = 0.1;
        let run = crate::run(&c).unwrap();
        let failure = FailureSummary::compute(&c).unwrap();
        assert!(failure.failed.o2_crossing(19.0).unwrap().censored);
        let fr = key_messages(&run, Some(&failure), Language::French);
        let sentence = fr.iter().find(|m| m.contains("panne")).unwrap();
        assert!(sentence.contains("au moins"), "{sentence}");
        let en = key_messages(&run, Some(&failure), Language::English);
        assert!(en.iter().any(|m| m.contains("at least")), "{en:?}");
        let opts = ReportOptions {
            lang: Language::French,
            generated_at: String::new(),
            sizing: None,
            failure: Some(failure),
            before_after: None,
        };
        let html = html(&run, &opts);
        assert!(html.contains("au moins"));
    }

    #[test]
    fn csv_has_readable_headers_and_locale() {
        let c = Configuration { gas: GasType::Argon, ..Configuration::default() };
        let run = crate::run(&c).unwrap();
        let fr = csv_text(&run, Language::French);
        assert!(fr.starts_with('\u{feff}'));
        let mut lines = fr.lines();
        let header = lines.next().unwrap();
        assert!(header.starts_with("\u{feff}Temps (s);O₂ bien mélangé (% vol);"), "{header}");
        assert_eq!(lines.count(), run.result.time.len());
        assert!(fr.contains(";0,"), "decimal comma");
        let en = csv_text(&run, Language::English);
        assert!(en.lines().next().unwrap().starts_with("\u{feff}Time (s),O₂ well-mixed (% vol),"));
        assert!(!en.contains(';'));
    }
}
