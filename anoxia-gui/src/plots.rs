//! Shared plotting helpers (egui_plot).

use crate::theme;
use anoxia_core::i18n::{fmt_clock, fmt_num};
use anoxia_core::Language;
use eframe::egui::{Color32, Ui};
use egui_plot::{HLine, Legend, Line, LineStyle, Plot, PlotBounds, PlotPoints, VLine};

/// Time unit chosen from the displayed duration.
#[derive(Debug, Clone, Copy)]
pub struct TimeAxis {
    pub div: f64,
    pub unit: &'static str,
}

impl TimeAxis {
    pub fn for_duration(seconds: f64) -> Self {
        if seconds > 3.0 * 3600.0 {
            Self { div: 3600.0, unit: "h" }
        } else if seconds > 600.0 {
            Self { div: 60.0, unit: "min" }
        } else {
            Self { div: 1.0, unit: "s" }
        }
    }

    pub fn label(self, lang: Language) -> String {
        format!("{} ({})", lang.pick("Temps depuis le début de la fuite", "Time since start of release"), self.unit)
    }
}

/// A curve to draw.
pub struct Curve<'a> {
    pub name: String,
    pub x: &'a [f64],
    pub y: &'a [f64],
    pub color: Color32,
    pub width: f32,
    pub dashed: bool,
}

/// Build plot points in display units, decimated to keep the UI responsive.
pub fn points(x: &[f64], y: &[f64], axis: TimeAxis) -> PlotPoints {
    let n = x.len().min(y.len());
    let step = n.div_ceil(2000).max(1);
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(2 * (n / step) + 2);
    let mut i = 0;
    while i < n {
        let end = (i + step).min(n);
        // keep both extremes of each bucket, in time order (minima of O₂ and
        // peaks of CO₂ / flow are never clipped)
        let kmin = (i..end).min_by(|&a, &b| y[a].total_cmp(&y[b])).unwrap_or(i);
        let kmax = (i..end).max_by(|&a, &b| y[a].total_cmp(&y[b])).unwrap_or(i);
        let (k1, k2) = if kmin <= kmax { (kmin, kmax) } else { (kmax, kmin) };
        pts.push([x[k1] / axis.div, y[k1]]);
        if k2 != k1 {
            pts.push([x[k2] / axis.div, y[k2]]);
        }
        i = end;
    }
    if let (Some(&lx), Some(&ly)) = (x.get(n.wrapping_sub(1)), y.get(n.wrapping_sub(1))) {
        pts.push([lx / axis.div, ly]);
    }
    PlotPoints::new(pts)
}

/// Horizontal threshold line.
pub struct Threshold {
    pub y: f64,
    pub name: String,
    pub color: Color32,
}

/// Vertical marker of a dated event (ventilation failure, backup fan, isolation).
pub struct Marker {
    /// Time (s)
    pub x: f64,
    pub name: String,
}

/// Markers of the scenario timeline of a run.
pub fn event_markers(run: &anoxia_core::SimulationRun, lang: Language) -> Vec<Marker> {
    run.config.timeline().iter().map(|e| Marker { x: e.time_s, name: e.action.short(lang) }).collect()
}

/// Plot options.
pub struct PlotSpec<'a> {
    pub id: &'a str,
    pub y_label: &'a str,
    pub y_unit: &'static str,
    /// Values always included in the vertical range
    pub y_include: Option<(f64, f64)>,
    pub height: f32,
    /// Initial time window (s); `None` = whole series
    pub x_end: Option<f64>,
    /// Dated events drawn as vertical lines
    pub markers: &'a [Marker],
}

/// Time-series plot with named thresholds, axis labels and a hover readout.
///
/// The mouse wheel scrolls the page (not the plot); Ctrl + wheel zooms,
/// drag pans and double-click restores the initial view.
pub fn time_plot(ui: &mut Ui, spec: &PlotSpec<'_>, curves: &[Curve<'_>], thresholds: &[Threshold], lang: Language) {
    let full = curves.iter().filter_map(|c| c.x.last().copied()).fold(1.0, f64::max);
    let x_end = spec.x_end.unwrap_or(full).clamp(1.0, full);
    let axis = TimeAxis::for_duration(x_end);
    let div = axis.div;
    let y_unit = spec.y_unit;

    // Vertical range over the displayed window
    let (mut ymin, mut ymax) = spec.y_include.unwrap_or((f64::INFINITY, f64::NEG_INFINITY));
    for c in curves {
        for (&x, &v) in c.x.iter().zip(c.y) {
            if x <= x_end {
                ymin = ymin.min(v);
                ymax = ymax.max(v);
            }
        }
    }
    if !ymin.is_finite() || !ymax.is_finite() {
        (ymin, ymax) = (0.0, 1.0);
    }
    let pad = ((ymax - ymin) * 0.06).max(1e-6);
    let (lo, hi) = (ymin - pad, ymax + pad);

    let plot = Plot::new(spec.id)
        .height(spec.height)
        .legend(Legend::default().position(egui_plot::Corner::RightBottom))
        .x_axis_label(axis.label(lang))
        .y_axis_label(spec.y_label.to_string())
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .y_axis_min_width(48.0)
        .label_formatter(move |name, p| {
            let t = fmt_clock(p.x * div);
            let v = fmt_num(p.y, 2, lang);
            if name.is_empty() {
                format!("t = {t}\n{v} {y_unit}")
            } else {
                format!("{name}\nt = {t}\n{v} {y_unit}")
            }
        })
        .x_axis_formatter(move |mark, _| fmt_num(mark.value, if mark.step_size < 1.0 { 1 } else { 0 }, lang));

    // Fit the view whenever the data or the window change, or on double-click.
    let signature = format!("{full:.6}|{x_end:.6}|{lo:.6}|{hi:.6}|{}", curves.len());
    let mem_id = ui.id().with(spec.id).with("signature");
    let changed = ui.ctx().data(|d| d.get_temp::<String>(mem_id)) != Some(signature.clone());
    if changed {
        ui.ctx().data_mut(|d| d.insert_temp(mem_id, signature));
    }
    let bounds = PlotBounds::from_min_max([0.0, lo], [x_end / div * 1.02, hi]);
    let response = plot.show(ui, |p| {
        if changed {
            p.set_plot_bounds(bounds);
        }
        for th in thresholds.iter().filter(|th| th.y >= lo && th.y <= hi) {
            p.hline(HLine::new(th.y).name(&th.name).color(th.color).style(LineStyle::dashed_loose()).width(1.2_f32));
        }
        for m in spec.markers {
            p.vline(
                VLine::new(m.x / div).name(&m.name).color(theme::MUTED).style(LineStyle::dotted_dense()).width(1.4_f32),
            );
        }
        for c in curves {
            let mut line = Line::new(points(c.x, c.y, axis)).name(&c.name).color(c.color).width(c.width);
            if c.dashed {
                line = line.style(LineStyle::dashed_dense());
            }
            p.line(line);
        }
    });
    if response.response.double_clicked() {
        ui.ctx().data_mut(|d| d.remove::<String>(mem_id));
    }
}

/// Standard O₂ thresholds for a run.
pub fn o2_thresholds(alarm1: f64, alarm2: f64, lang: Language) -> Vec<Threshold> {
    let pct = |v: f64| anoxia_core::i18n::fmt_pct(v, 1, lang);
    vec![
        Threshold {
            y: alarm1,
            name: format!("{} ({})", lang.pick("Alarme 1", "Alarm 1"), pct(alarm1)),
            color: theme::WARNING,
        },
        Threshold {
            y: alarm2,
            name: format!("{} ({})", lang.pick("Alarme 2", "Alarm 2"), pct(alarm2)),
            color: theme::DANGER,
        },
        Threshold {
            y: 10.0,
            name: format!("{} ({})", lang.pick("Danger vital", "Life-threatening"), pct(10.0)),
            color: theme::VITAL,
        },
    ]
}

/// Colours for comparison curves, distinct from the threshold colours
/// (amber, red, purple).
pub const PALETTE: [Color32; 8] = [
    Color32::from_rgb(21, 101, 192),
    Color32::from_rgb(46, 125, 50),
    Color32::from_rgb(0, 131, 143),
    Color32::from_rgb(93, 64, 55),
    Color32::from_rgb(26, 35, 126),
    Color32::from_rgb(130, 119, 23),
    Color32::from_rgb(84, 110, 122),
    Color32::from_rgb(0, 0, 0),
];

/// Initial time window focused on the event: release and minimum, plus the
/// return above 19.5 % only if it happens within a comparable time (a slow
/// recovery over hours would otherwise squeeze the dip to the left edge).
pub fn event_window(s: &anoxia_core::solver::Summary) -> f64 {
    s.event_window()
}
