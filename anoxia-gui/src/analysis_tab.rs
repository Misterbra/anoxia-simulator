//! Analyses tab: comparisons, parameter sweeps, sensitivity, ventilation
//! failure and sizing. Studies run on a background thread and are cached.

use crate::app::AnoxiaApp;
use crate::plots::{self, Curve, PlotSpec, PALETTE};
use crate::theme::{self, card, hint, section_title, MUTED};
use anoxia_core::analysis::{self, Criterion, Parameter, Point, Sizing, TornadoBar, Variant};
use anoxia_core::config::VentilationMode;
use anoxia_core::i18n::{fmt_clock, fmt_duration, fmt_num, fmt_pct, fmt_unit};
use anoxia_core::{Configuration, Language};
use eframe::egui::{self, Color32, DragValue, RichText, Ui};
use egui_plot::{Bar, BarChart, HLine, Legend, Line, LineStyle, Plot, Points, VLine};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Study {
    Compare,
    Sweep,
    Tornado,
    Failure,
    BeforeAfter,
    Sizing,
}

impl Study {
    fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Compare => lang.pick("Comparaison de scénarios", "Scenario comparison"),
            Self::Sweep => lang.pick("Balayage d'un paramètre", "Parameter sweep"),
            Self::Tornado => lang.pick("Sensibilité (tornado)", "Sensitivity (tornado)"),
            Self::Failure => lang.pick("Panne de ventilation", "Ventilation failure"),
            Self::BeforeAfter => lang.pick("Avant / après installation", "Before / after installation"),
            Self::Sizing => lang.pick("Dimensionnement", "Sizing"),
        }
    }
}

/// O₂ level to respect in the sizing calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizingTarget {
    Alarm1,
    Alarm2,
    Osha,
}

impl SizingTarget {
    pub fn level(self, c: &Configuration) -> f64 {
        let (a1, a2) = c.thresholds.levels();
        match self {
            Self::Alarm1 => a1,
            Self::Alarm2 => a2,
            Self::Osha => 19.5,
        }
    }

    pub fn name(self, c: &Configuration, lang: Language) -> String {
        let v = fmt_pct(self.level(c), 1, lang);
        match self {
            Self::Alarm1 => format!("{} ({v})", lang.pick("Alarme 1", "Alarm 1")),
            Self::Alarm2 => format!("{} ({v})", lang.pick("Alarme 2", "Alarm 2")),
            Self::Osha => format!("OSHA ({v})"),
        }
    }
}

pub enum Output {
    Variants(Vec<Variant>),
    Sweep(Vec<Point>),
    Tornado(f64, Vec<TornadoBar>),
    Sizing { ventilation: Sizing, diameter: Sizing, criterion: Criterion },
    Error(String),
}

struct Job {
    study: Study,
    key: String,
    rx: Receiver<Output>,
    started: Instant,
}

pub struct AnalysisState {
    pub study: Study,
    pub compare_param: Parameter,
    pub sweep_param: Parameter,
    pub sweep_range: Option<(f64, f64)>,
    pub tornado_rel: f64,
    pub sizing_target: SizingTarget,
    /// Ventilation mode for the sizing; `None` = the scenario's mode
    pub sizing_mode: Option<VentilationMode>,
    job: Option<Job>,
    results: HashMap<Study, (String, Output)>,
}

impl Default for AnalysisState {
    fn default() -> Self {
        Self {
            study: Study::Compare,
            compare_param: Parameter::LeakDiameter,
            sweep_param: Parameter::MechanicalFlow,
            sweep_range: None,
            tornado_rel: 0.3,
            sizing_target: SizingTarget::Alarm1,
            sizing_mode: None,
            job: None,
            results: HashMap::new(),
        }
    }
}

impl AnalysisState {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Sizing results for the report, if computed for `config`.
    /// Ventilation mode used by the sizing for this scenario.
    pub fn sizing_mode_for(&self, config: &Configuration) -> VentilationMode {
        self.sizing_mode.unwrap_or(config.room.mechanical_mode)
    }

    pub fn sizing_for(&self, config: &Configuration) -> Option<(Sizing, Sizing, Criterion)> {
        let (key, out) = self.results.get(&Study::Sizing)?;
        if *key != sizing_key(config, self.sizing_target, self.sizing_mode_for(config)) {
            return None;
        }
        match out {
            Output::Sizing { ventilation, diameter, criterion } => Some((*ventilation, *diameter, *criterion)),
            _ => None,
        }
    }

    fn poll(&mut self) {
        if let Some(job) = &self.job {
            if let Ok(out) = job.rx.try_recv() {
                let job = self.job.take().expect("job present");
                self.results.insert(job.study, (job.key, out));
            }
        }
    }

    /// Start a study; a running study with another key is abandoned (its
    /// result is simply never read).
    fn launch(&mut self, study: Study, key: String, work: impl FnOnce() -> Output + Send + 'static) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        self.job = Some(Job { study, key, rx, started: Instant::now() });
    }
}

/// JSON of the inputs that influence the physics. The identification is
/// excluded so that editing the site or the author does not invalidate the
/// studies (and does not drop the sizing from the report).
fn physics_json(config: &Configuration) -> String {
    let mut c = config.clone();
    c.identification = Default::default();
    c.report = Default::default();
    c.future = Default::default();
    serde_json::to_string(&c).unwrap_or_default()
}

fn sizing_key(config: &Configuration, target: SizingTarget, mode: VentilationMode) -> String {
    format!("sizing|{target:?}|{mode:?}|{}", physics_json(config))
}

/// Values for a comparison around the current value.
fn compare_values(p: Parameter, c: &Configuration) -> Vec<f64> {
    let v = p.get(c);
    match p {
        Parameter::DischargeCoefficient => vec![0.62, 0.75, 0.85, 1.0],
        Parameter::Efficiency => vec![0.3, 0.5, 0.7, 1.0],
        Parameter::MechanicalFlow => {
            let base = v.max(50.0);
            vec![0.0, base * 0.5, base, base * 2.0, base * 4.0]
        }
        Parameter::NaturalAch => vec![0.0, 0.2, 0.5, 1.0, 2.0],
        _ => [0.25, 0.5, 1.0, 2.0, 4.0].iter().map(|f| v * f).collect(),
    }
}

/// Number with a precision adapted to its magnitude.
fn smart(v: f64, unit: &str, lang: Language) -> String {
    let d = if v.abs() >= 100.0 {
        0
    } else if v.abs() >= 10.0 {
        1
    } else {
        2
    };
    if unit.is_empty() {
        fmt_num(v, d, lang)
    } else {
        fmt_unit(v, d, unit, lang)
    }
}

/// French-aware drag value.
fn drag(ui: &mut Ui, v: &mut f64, speed: f64, lang: Language) -> bool {
    ui.add(
        DragValue::new(v)
            .speed(speed)
            .custom_formatter(move |x, _| {
                fmt_num(x, if x.abs() >= 100.0 { 0 } else { 2 }, lang).replace('\u{202f}', "")
            })
            .custom_parser(|s| s.replace(',', ".").replace(['\u{202f}', ' ', '\u{a0}'], "").parse().ok()),
    )
    .changed()
}

pub fn show(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    app.analysis.poll();

    ui.horizontal_wrapped(|ui| {
        for s in [Study::Compare, Study::Sweep, Study::Tornado, Study::Failure, Study::BeforeAfter, Study::Sizing] {
            ui.selectable_value(&mut app.analysis.study, s, s.name(lang));
        }
    });
    ui.separator();

    if app.error.is_some() {
        theme::callout(
            ui,
            theme::DANGER,
            t(
                "Corriger les paramètres invalides pour lancer les analyses.",
                "Fix the invalid inputs to run the analyses.",
            ),
        );
        return;
    }

    if app.analysis.study == Study::BeforeAfter && future_editor(ui, &mut app.config, lang) {
        app.last_change = Instant::now();
    }
    let config = app.config.clone();
    let st = &mut app.analysis;
    let study = st.study;
    let key = controls(st, ui, &config, lang);

    // Launch when inputs are stable for 300 ms and the cached key differs;
    // a running study for an outdated key is replaced.
    let cached = st.results.get(&study).map(|(k, _)| k.as_str());
    let settled = app.last_change.elapsed() > Duration::from_millis(300);
    let running_same = st.job.as_ref().is_some_and(|j| j.study == study && j.key == key);
    if cached != Some(key.as_str()) && !running_same && settled {
        let c = config.clone();
        let p = st.compare_param;
        let sp = st.sweep_param;
        let range = st.sweep_range.unwrap_or_else(|| sp.default_range(&config));
        let rel = st.tornado_rel;
        let target = st.sizing_target;
        let sizing_mode = st.sizing_mode_for(&config);
        let work: Box<dyn FnOnce() -> Output + Send> = match study {
            Study::Compare => {
                Box::new(move || Output::Variants(analysis::compare(&c, p, &compare_values(p, &c), lang)))
            }
            Study::Sweep => {
                Box::new(move || Output::Sweep(analysis::sweep(&c, sp, &analysis::range_values(range.0, range.1, 25))))
            }
            Study::Tornado => Box::new(move || {
                let (b, bars) = analysis::tornado(&c, rel);
                Output::Tornado(b, bars)
            }),
            Study::Failure => Box::new(move || Output::Variants(analysis::ventilation_failure(&c, lang))),
            Study::BeforeAfter => Box::new(move || match analysis::before_after(&c, lang) {
                Ok(v) => Output::Variants(v),
                Err(e) => Output::Error(format!(
                    "{} {e}",
                    lang.pick(
                        "Étude impossible : la configuration future n'est pas valide.",
                        "Study impossible: the future configuration is not valid."
                    )
                )),
            }),
            Study::Sizing => Box::new(move || {
                let crit = Criterion { min_o2: target.level(&c), max_co2: 3.0 };
                // The mode is chosen explicitly for the search; the scenario is not changed.
                let mut c = c;
                c.room.mechanical_mode = sizing_mode;
                match (analysis::required_ventilation(&c, crit), analysis::max_leak_diameter(&c, crit)) {
                    (Ok(v), Ok(d)) => Output::Sizing { ventilation: v, diameter: d, criterion: crit },
                    (Err(e), _) | (_, Err(e)) => Output::Error(e.to_string()),
                }
            }),
        };
        st.launch(study, key.clone(), work);
    }

    if let Some(j) = st.job.as_ref().filter(|j| j.study == study) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("{} ({:.1} s)", t("Calcul en cours…", "Computing…"), j.started.elapsed().as_secs_f32()));
        });
    }

    let Some((rkey, out)) = st.results.get(&study) else { return };
    let outdated = *rkey != key;
    let apply_flow = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if outdated {
                ui.set_opacity(0.5);
            }
            let mut apply_flow = None;
            match out {
                Output::Variants(v) if study == Study::BeforeAfter => before_after_view(ui, v, &config, lang),
                Output::Variants(v) => variants_view(ui, v, &config, st.compare_param, lang, study == Study::Failure),
                Output::Sweep(points) => sweep_view(ui, points, st.sweep_param, &config, lang),
                Output::Tornado(base, bars) => tornado_view(ui, *base, bars, st.tornado_rel, &config, lang),
                Output::Sizing { ventilation, diameter, criterion } => {
                    let mode = st.sizing_mode_for(&config);
                    apply_flow = sizing_view(ui, *ventilation, *diameter, *criterion, mode, &config, lang);
                }
                Output::Error(e) => theme::callout(ui, theme::DANGER, e),
            }
            apply_flow
        })
        .inner;
    if let Some((q, mode)) = apply_flow {
        app.config.room.mechanical_enabled = true;
        app.config.room.mechanical_flow_m3h = q.ceil();
        app.config.room.mechanical_mode = mode;
        app.last_change = Instant::now();
        if app.auto_compute {
            app.compute();
        }
    }
}

/// Draw the controls of the active study and return its cache key.
fn controls(st: &mut AnalysisState, ui: &mut Ui, config: &Configuration, lang: Language) -> String {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let json = physics_json(config);
    let applicable: Vec<Parameter> = Parameter::all().into_iter().filter(|p| p.applicable(config)).collect();
    let combo = |ui: &mut Ui, id: &str, value: &mut Parameter| {
        egui::ComboBox::from_id_source(id).selected_text(value.name(lang)).show_ui(ui, |ui| {
            for p in &applicable {
                ui.selectable_value(value, *p, p.name(lang));
            }
        });
    };
    match st.study {
        Study::Compare => {
            if !st.compare_param.applicable(config) {
                st.compare_param = applicable.first().copied().unwrap_or(Parameter::RoomVolume);
            }
            ui.horizontal(|ui| {
                ui.label(t("Paramètre comparé :", "Compared parameter:"));
                combo(ui, "cmp_param", &mut st.compare_param);
            });
            hint(ui, t("Courbes d'O₂ superposées pour plusieurs valeurs autour de la valeur actuelle (en gras dans le tableau).", "Overlaid O₂ curves for several values around the current one (bold in the table)."));
            format!("cmp|{:?}|{:?}|{json}", st.compare_param, lang)
        }
        Study::Sweep => {
            if !st.sweep_param.applicable(config) {
                st.sweep_param = applicable.first().copied().unwrap_or(Parameter::RoomVolume);
                st.sweep_range = None;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Paramètre :", "Parameter:"));
                let before = st.sweep_param;
                combo(ui, "sweep_param", &mut st.sweep_param);
                if before != st.sweep_param {
                    st.sweep_range = None;
                }
                let (mut lo, mut hi) = st.sweep_range.unwrap_or_else(|| st.sweep_param.default_range(config));
                ui.label(t("de", "from"));
                let (slo, shi) = (lo.abs().max(0.1) * 0.01, hi.abs().max(0.1) * 0.01);
                let c1 = drag(ui, &mut lo, slo, lang);
                ui.label(t("à", "to"));
                let c2 = drag(ui, &mut hi, shi, lang);
                ui.label(st.sweep_param.unit());
                if c1 || c2 {
                    if hi <= lo {
                        hi = lo * 1.1 + 0.01;
                    }
                    st.sweep_range = Some((lo.max(0.0), hi));
                }
                if ui.small_button(t("Plage par défaut", "Default range")).clicked() {
                    st.sweep_range = None;
                }
            });
            let (lo, hi) = st.sweep_range.unwrap_or_else(|| st.sweep_param.default_range(config));
            format!("sweep|{:?}|{lo}|{hi}|{json}", st.sweep_param)
        }
        Study::Tornado => {
            ui.horizontal(|ui| {
                ui.label(t("Variation de chaque paramètre : ±", "Variation of each parameter: ±"));
                let mut pct = st.tornado_rel * 100.0;
                if ui.add(DragValue::new(&mut pct).range(5.0..=80.0).speed(1.0).suffix(" %")).changed() {
                    st.tornado_rel = pct / 100.0;
                }
            });
            hint(ui, t("Impact de chaque paramètre sur l'O₂ minimal, les autres restant fixes : identifie les données à fiabiliser en priorité.", "Impact of each parameter on the minimum O₂, others fixed: identifies which data to firm up first."));
            format!("tornado|{}|{json}", st.tornado_rel)
        }
        Study::Failure => {
            hint(ui, t("Compare le scénario avec la ventilation mécanique en service et en panne (seules les infiltrations subsistent).", "Compares the scenario with mechanical ventilation running and failed (only infiltration remains)."));
            format!("fail|{:?}|{json}", lang)
        }
        Study::BeforeAfter => {
            format!("ba|{:?}|{}|{json}", lang, serde_json::to_string(&config.future).unwrap_or_default())
        }
        Study::Sizing => {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Maintenir l'O₂ au-dessus de :", "Keep O₂ above:"));
                for target in [SizingTarget::Alarm1, SizingTarget::Alarm2, SizingTarget::Osha] {
                    ui.selectable_value(&mut st.sizing_target, target, target.name(config, lang));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Mode de ventilation dimensionné :", "Ventilation mode to size:"));
                let scenario_mode = config.room.mechanical_mode;
                let mut mode = st.sizing_mode_for(config);
                for (m, fr, en) in [
                    (VentilationMode::Extraction, "Extraction", "Extraction"),
                    (VentilationMode::Supply, "Soufflage (air neuf)", "Supply (fresh air)"),
                ] {
                    let label = if m == scenario_mode {
                        format!("{} ({})", lang.pick(fr, en), t("mode du scénario", "scenario mode"))
                    } else {
                        lang.pick(fr, en).to_string()
                    };
                    ui.selectable_value(&mut mode, m, label);
                }
                st.sizing_mode = (mode != scenario_mode).then_some(mode);
            });
            hint(ui, t("Recherche le débit de ventilation mécanique (dans le mode choisi) et le diamètre de fuite maximal respectant ce seuil (et CO₂ ≤ 3 % pour le CO₂). Le calcul est prolongé jusqu'à couvrir l'événement ; un résultat non démontré est signalé comme tel.", "Finds the mechanical ventilation flow (in the chosen mode) and the maximum leak diameter meeting this level (and CO₂ ≤ 3% for CO₂). The computation is extended to cover the event; a result that is not demonstrated is reported as such."));
            sizing_key(config, st.sizing_target, st.sizing_mode_for(config))
        }
    }
}

fn variants_view(
    ui: &mut Ui,
    variants: &[Variant],
    config: &Configuration,
    param: Parameter,
    lang: Language,
    failure: bool,
) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let (a1, a2) = config.thresholds.levels();
    let current = if failure {
        if config.room.mechanical_enabled {
            1.0
        } else {
            0.0
        }
    } else {
        param.get(config)
    };
    card(ui, |ui| {
        section_title(
            ui,
            if failure {
                t("Ventilation en service / en panne", "Ventilation running / failed")
            } else {
                t("Courbes d'oxygène comparées", "Compared oxygen curves")
            },
        );
        let mem = egui::Id::new("cmp_full");
        let mut full = ui.ctx().data(|d| d.get_temp::<bool>(mem)).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("Fenêtre :", "Window:")).small().color(MUTED));
            if ui.selectable_label(!full, t("événement", "event")).clicked() {
                full = false;
            }
            if ui.selectable_label(full, t("durée complète", "full duration")).clicked() {
                full = true;
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(mem, full));
        let curves: Vec<Curve> = variants
            .iter()
            .enumerate()
            .map(|(i, v)| Curve {
                name: v.label.clone(),
                x: &v.result.time,
                y: &v.result.o2,
                color: PALETTE[i % PALETTE.len()],
                width: if (v.value - current).abs() < 1e-9 { 3.0 } else { 1.8 },
                dashed: false,
            })
            .collect();
        let window = variants.iter().map(|v| plots::event_window(&v.result.summary)).fold(60.0, f64::max);
        let ymin = variants.iter().map(|v| v.result.summary.min_o2).fold(21.0, f64::min);
        let spec = PlotSpec {
            id: "cmp_plot",
            y_label: t("O₂ (% vol)", "O₂ (% vol)"),
            y_unit: "%",
            y_include: Some((ymin.min(a1) - 0.5, 21.2)),
            height: 360.0,
            x_end: (!full).then_some(window),
            markers: &[],
        };
        plots::time_plot(ui, &spec, &curves, &plots::o2_thresholds(a1, a2, lang), lang);
    });

    let co2 = config.gas.is_toxic();
    let mut headers = vec![
        t("Variante", "Variant"),
        t("O₂ min.", "Min O₂"),
        t("Alarme 1 (min:s)", "Alarm 1 (min:s)"),
        t("Alarme 2 (min:s)", "Alarm 2 (min:s)"),
        t("Durée sous alarme 1", "Time below alarm 1"),
        t("Retour > 19,5 % (min:s)", "Back > 19.5% (min:s)"),
    ];
    if co2 {
        headers.push(t("CO₂ max.", "Peak CO₂"));
    }
    egui::Grid::new("cmp_table").striped(true).num_columns(headers.len()).spacing([20.0, 4.0]).show(ui, |ui| {
        for h in &headers {
            ui.label(RichText::new(*h).strong());
        }
        ui.end_row();
        for v in variants {
            let s = &v.result.summary;
            let is_current = (v.value - current).abs() < 1e-9;
            let cell = |text: String| if is_current { RichText::new(text).strong() } else { RichText::new(text) };
            let first = |l: f64| s.o2_crossing(l).and_then(|c| c.first).map_or_else(|| "—".into(), fmt_clock);
            let near = (s.min_o2 - a1).abs() < 0.1 || (s.min_o2 - a2).abs() < 0.1;
            ui.label(cell(format!("{}{}", v.label, if is_current { t(" (actuel)", " (current)") } else { "" })));
            ui.label(cell(fmt_pct(s.min_o2, if near { 2 } else { 1 }, lang)));
            ui.label(cell(first(a1)));
            ui.label(cell(first(a2)));
            ui.label(cell(s.o2_crossing(a1).filter(|c| c.reached()).map_or_else(
                || "—".into(),
                |c| format!("{}{}", if c.censored { "≥ " } else { "" }, fmt_duration(c.total, lang)),
            )));
            ui.label(cell(match s.o2_crossing(19.5) {
                Some(c) if c.reached() => c.back.map_or_else(
                    || t("non observé avant la fin du calcul", "not observed before the end of the computation").into(),
                    fmt_clock,
                ),
                _ => "—".into(),
            }));
            if co2 {
                ui.label(cell(s.max_co2_percent.map_or_else(|| "—".into(), |c| fmt_pct(c, 1, lang))));
            }
            ui.end_row();
        }
    });
    hint(ui, t("Temps comptés depuis le début de la fuite, en min:s (h:min:s au-delà d'une heure). « — » : seuil non atteint.", "Times from the start of the release, in min:s (h:min:s beyond one hour). \"—\": level not reached."));
    if failure && variants.len() == 2 {
        let below =
            |v: &Variant| v.result.summary.o2_crossing(a1).map_or_else(|| "—".to_string(), |c| c.total_text(lang));
        let (on, off) = (&variants[0], &variants[1]);
        hint(
            ui,
            &format!(
                "{} {} → {} ; {} {} → {}.",
                t("O₂ minimal :", "Minimum O₂:"),
                fmt_pct(on.result.summary.min_o2, 2, lang),
                fmt_pct(off.result.summary.min_o2, 2, lang),
                t("durée sous l'alarme 1 :", "time below alarm 1:"),
                below(on),
                below(off)
            ),
        );
        hint(ui, t("Cette vérification figure dans le rapport HTML.", "This check is included in the HTML report."));
    }
}

fn sweep_view(ui: &mut Ui, points: &[Point], p: Parameter, config: &Configuration, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let (a1, a2) = config.thresholds.levels();
    let current = p.get(config);
    let unit = p.unit();
    let xlabel = if unit.is_empty() { p.name(lang).to_string() } else { format!("{} ({unit})", p.name(lang)) };
    card(ui, |ui| {
        section_title(ui, t("O₂ minimal en fonction du paramètre", "Minimum O₂ versus parameter"));
        let pts: Vec<[f64; 2]> = points.iter().map(|q| [q.value, q.min_o2]).collect();
        Plot::new("sweep_plot")
            .height(300.0)
            .legend(Legend::default().position(egui_plot::Corner::RightTop))
            .x_axis_label(xlabel.clone())
            .y_axis_label(t("O₂ minimal (% vol)", "Minimum O₂ (% vol)"))
            .y_axis_min_width(56.0)
            .allow_scroll(false)
            .include_y(21.0)
            .include_y(a2 - 0.5)
            .label_formatter(move |_, v| format!("{}\nO₂ min = {}", fmt_num(v.x, 2, lang), fmt_pct(v.y, 2, lang)))
            .show(ui, |pl| {
                pl.hline(
                    HLine::new(a1)
                        .name(format!("{} {}", t("Alarme 1", "Alarm 1"), fmt_pct(a1, 1, lang)))
                        .color(theme::WARNING)
                        .style(LineStyle::dashed_loose()),
                );
                pl.hline(
                    HLine::new(a2)
                        .name(format!("{} {}", t("Alarme 2", "Alarm 2"), fmt_pct(a2, 1, lang)))
                        .color(theme::DANGER)
                        .style(LineStyle::dashed_loose()),
                );
                pl.vline(
                    VLine::new(current)
                        .name(t("Valeur actuelle", "Current value"))
                        .color(Color32::from_rgb(38, 50, 56))
                        .width(1.5_f32),
                );
                pl.line(Line::new(pts.clone()).name(t("O₂ minimal", "Minimum O₂")).color(theme::ACCENT).width(2.0_f32));
                pl.points(Points::new(pts).radius(3.0_f32).color(theme::ACCENT));
            });
    });
    card(ui, |ui| {
        section_title(ui, t("Délai avant alarme", "Time to alarm"));
        let p1: Vec<[f64; 2]> = points.iter().filter_map(|q| q.t_alarm1.map(|v| [q.value, v / 60.0])).collect();
        let p2: Vec<[f64; 2]> = points.iter().filter_map(|q| q.t_alarm2.map(|v| [q.value, v / 60.0])).collect();
        // Head-room above the highest curve so that the legend does not sit on it.
        let top = p1.iter().chain(&p2).map(|q| q[1]).fold(1.0, f64::max) * 1.6;
        Plot::new("sweep_time_plot")
            .height(240.0)
            .legend(Legend::default().position(egui_plot::Corner::LeftTop))
            .x_axis_label(xlabel)
            .y_axis_label(t("Délai (min)", "Time (min)"))
            .y_axis_min_width(56.0)
            .allow_scroll(false)
            .include_y(0.0)
            .include_y(top)
            .label_formatter(move |_, v| format!("{}\nt = {}", fmt_num(v.x, 2, lang), fmt_clock(v.y * 60.0)))
            .show(ui, |pl| {
                pl.vline(
                    VLine::new(current)
                        .name(t("Valeur actuelle", "Current value"))
                        .color(Color32::from_rgb(38, 50, 56))
                        .width(1.5_f32),
                );
                pl.line(Line::new(p1).name(t("Alarme 1", "Alarm 1")).color(theme::WARNING).width(2.0_f32));
                pl.line(Line::new(p2).name(t("Alarme 2", "Alarm 2")).color(theme::DANGER).width(2.0_f32));
            });
        hint(
            ui,
            t(
                "Une courbe s'interrompt quand le seuil n'est plus atteint.",
                "A curve stops where the threshold is no longer reached.",
            ),
        );
    });
}

fn tornado_view(ui: &mut Ui, base: f64, bars: &[TornadoBar], rel: f64, config: &Configuration, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let (a1, a2) = config.thresholds.levels();
    card(ui, |ui| {
        section_title(ui, t("Sensibilité de l'O₂ minimal", "Sensitivity of the minimum O₂"));
        hint(
            ui,
            &format!(
                "{} {}",
                t("O₂ minimal du scénario de base :", "Minimum O₂ of the base case:"),
                fmt_pct(base, 2, lang)
            ),
        );
        let names: Vec<String> = bars.iter().map(|b| b.parameter.name(lang).to_string()).collect();
        let n = bars.len();
        let low_color = Color32::from_rgb(239, 108, 0);
        let high_color = theme::ACCENT;
        #[allow(clippy::cast_precision_loss)]
        let mk = |value: fn(&TornadoBar) -> f64, color: Color32| -> Vec<Bar> {
            bars.iter()
                .enumerate()
                .map(|(i, b)| {
                    Bar::new((n - 1 - i) as f64, value(b) - base)
                        .base_offset(base)
                        .width(0.6)
                        .name(&names[i])
                        .fill(color.gamma_multiply(0.75))
                        .stroke(egui::Stroke::new(1.0_f32, color))
                })
                .collect()
        };
        let low = mk(|b| b.low_min_o2, low_color);
        let high = mk(|b| b.high_min_o2, high_color);
        let lo_x = bars.iter().map(|b| b.low_min_o2.min(b.high_min_o2)).fold(base, f64::min);
        let hi_x = bars.iter().map(|b| b.low_min_o2.max(b.high_min_o2)).fold(base, f64::max);
        let names2 = names.clone();
        #[allow(clippy::cast_precision_loss)]
        Plot::new("tornado")
            .height((70.0 + 38.0 * n as f32).max(230.0))
            .legend(Legend::default().position(egui_plot::Corner::RightBottom))
            .x_axis_label(t("O₂ minimal (% vol)", "Minimum O₂ (% vol)"))
            .y_axis_min_width(200.0)
            .show_y(false)
            .y_axis_formatter(move |m, _| {
                let i = m.value.round();
                if (m.value - i).abs() > 1e-6 || i < 0.0 {
                    return String::new();
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let idx = n.checked_sub(1 + i as usize);
                idx.and_then(|k| names2.get(k)).cloned().unwrap_or_default()
            })
            .include_y(-0.8)
            .include_y(n as f64)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |pl| {
                pl.bar_chart(BarChart::new(low).horizontal().color(low_color).name(format!(
                    "{} −{}",
                    t("Valeur basse", "Low value"),
                    fmt_pct(rel * 100.0, 0, lang)
                )));
                pl.bar_chart(BarChart::new(high).horizontal().color(high_color).name(format!(
                    "{} +{}",
                    t("Valeur haute", "High value"),
                    fmt_pct(rel * 100.0, 0, lang)
                )));
                pl.vline(VLine::new(base).color(Color32::from_rgb(38, 50, 56)).name(t("Base", "Base")));
                let margin = (hi_x - lo_x).max(0.2) * 0.3;
                for (level, name, color) in
                    [(a1, t("Alarme 1", "Alarm 1"), theme::WARNING), (a2, t("Alarme 2", "Alarm 2"), theme::DANGER)]
                {
                    if level > lo_x - margin && level < hi_x + margin {
                        pl.vline(
                            VLine::new(level)
                                .color(color)
                                .style(LineStyle::dashed_loose())
                                .name(format!("{name} {}", fmt_pct(level, 1, lang))),
                        );
                    }
                }
            });
    });
    egui::Grid::new("tornado_table").striped(true).num_columns(4).spacing([20.0, 4.0]).show(ui, |ui| {
        for h in [
            t("Paramètre", "Parameter"),
            t("Valeur basse → O₂ min.", "Low value → min O₂"),
            t("Valeur haute → O₂ min.", "High value → min O₂"),
            t("Écart (points d'O₂)", "Swing (O₂ points)"),
        ] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for b in bars {
            let u = b.parameter.unit();
            ui.label(b.parameter.name(lang));
            ui.label(format!("{} → {}", smart(b.low_value, u, lang), fmt_pct(b.low_min_o2, 2, lang)));
            ui.label(format!("{} → {}", smart(b.high_value, u, lang), fmt_pct(b.high_min_o2, 2, lang)));
            ui.label(fmt_num(b.swing(), 2, lang));
            ui.end_row();
        }
    });
}

/// Returns a flow to apply to the scenario if the user asks for it.
#[allow(clippy::too_many_lines)]
fn sizing_view(
    ui: &mut Ui,
    vent: Sizing,
    diam: Sizing,
    crit: Criterion,
    mode: VentilationMode,
    config: &Configuration,
    lang: Language,
) -> Option<(f64, VentilationMode)> {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let ok_green = Color32::from_rgb(27, 94, 32);
    let mut apply = None;
    let (mode_title, mode_word) = match mode {
        VentilationMode::Extraction => (
            t("Débit d'extraction mécanique nécessaire", "Required mechanical extraction flow"),
            t("extraction", "extraction"),
        ),
        VentilationMode::Supply => (
            t("Débit de soufflage (air neuf) nécessaire", "Required supply (fresh-air) flow"),
            t("soufflage", "supply"),
        ),
    };
    card(ui, |ui| {
        section_title(ui, mode_title);
        hint(
            ui,
            &format!(
                "{} O₂ ≥ {}{} — {} {}",
                t("Critère :", "Criterion:"),
                fmt_pct(crit.min_o2, 1, lang),
                if config.gas.is_toxic() {
                    format!(", CO₂ ≤ {}", fmt_pct(crit.max_co2, 1, lang))
                } else {
                    String::new()
                },
                t("efficacité de ventilation (η) prise en compte :", "ventilation effectiveness (η) used:"),
                fmt_num(config.room.ventilation_efficiency, 2, lang)
            ),
        );
        let current = if config.room.mechanical_enabled { config.room.mechanical_flow_m3h } else { 0.0 };
        match vent {
            Sizing::AlreadyMet => theme::callout(ui, ok_green, t("Le critère est respecté même sans ventilation mécanique (infiltrations seules).", "The criterion is met even without mechanical ventilation (infiltration only).")),
            Sizing::Value(q) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(fmt_unit(q, 0, "m³/h", lang)).size(26.0).strong());
                    ui.label(RichText::new(format!(
                        "{} {}{}",
                        t("— débit actuel", "— current flow"),
                        fmt_unit(current, 0, "m³/h", lang),
                        if current > 0.0 { format!(" (× {})", fmt_num(q / current, 1, lang)) } else { String::new() }
                    )).color(MUTED));
                });
                let ach = q / config.room.volume_m3;
                hint(ui, &format!("≈ {} {}", fmt_unit(ach, 1, "vol/h", lang), t("de renouvellement nominal", "nominal air changes")));
                if ach > 15.0 {
                    theme::callout(ui, theme::WARNING, t("Plus de 15 vol/h est rarement réaliste dans un local occupé (courants d'air, bruit, coût) : agir d'abord à la source (quantité de gaz, limiteur de débit, coupure automatique, stockage extérieur).", "More than 15 ACH is rarely realistic in an occupied room (draughts, noise, cost): act at the source first (gas inventory, flow limiter, automatic shut-off, outdoor storage)."));
                }
                if current >= q && config.room.mechanical_mode == mode {
                    theme::callout(ui, ok_green, t("Le débit actuel est suffisant pour ce critère.", "The current flow meets this criterion."));
                } else if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!(
                                "{} ({mode_word})",
                                t("Appliquer ce débit au scénario", "Apply this flow to the scenario")
                            ))
                            .strong()
                            .color(Color32::WHITE),
                        )
                        .fill(theme::ACCENT),
                    )
                    .clicked()
                {
                    apply = Some((q, mode));
                }
            }
            Sizing::NotAchievable => theme::callout(ui, theme::DANGER, t("Aucun débit de ventilation raisonnable ne permet de respecter le critère : réduire le risque à la source (quantité de gaz, limiteur de débit, stockage extérieur).", "No reasonable ventilation flow meets the criterion: reduce the risk at the source (gas inventory, flow limiter, outdoor storage).")),
            Sizing::Undetermined => theme::callout(ui, theme::WARNING, t("Non démontré : même prolongé à 168 h, le calcul ne couvre pas l'événement. Vérifier la durée du rejet et la durée maximale simulée.", "Not demonstrated: even extended to 168 h, the computation does not cover the event. Check the release duration and the maximum simulated time.")),
        }
    });
    if config.release.kind.uses_orifice() {
        card(ui, |ui| {
            section_title(ui, t("Diamètre de fuite maximal tolérable", "Maximum tolerable leak diameter"));
            hint(
                ui,
                &format!(
                    "{} {}. {}",
                    t(
                        "Avec la ventilation actuelle ; diamètre du scénario :",
                        "With the current ventilation; scenario diameter:"
                    ),
                    fmt_unit(config.release.diameter_mm, 2, "mm", lang),
                    t(
                        "Utile pour dimensionner un orifice de restriction ou un limiteur de débit.",
                        "Useful to size a restriction orifice or flow limiter."
                    )
                ),
            );
            match diam {
                Sizing::AlreadyMet => {
                    theme::callout(ui, ok_green, t("Critère respecté jusqu'à 50 mm.", "Criterion met up to 50 mm."))
                }
                Sizing::Value(d) => {
                    ui.label(RichText::new(fmt_unit(d, 2, "mm", lang)).size(26.0).strong());
                }
                Sizing::NotAchievable => theme::callout(
                    ui,
                    theme::DANGER,
                    t(
                        "Même une fuite de 0,05 mm ne respecte pas le critère.",
                        "Even a 0.05 mm leak does not meet the criterion.",
                    ),
                ),
                Sizing::Undetermined => theme::callout(
                    ui,
                    theme::WARNING,
                    t(
                        "Non démontré : l'horizon de calcul ne couvre pas l'événement.",
                        "Not demonstrated: the computation horizon does not cover the event.",
                    ),
                ),
            }
        });
    }
    hint(ui, t("Ces résultats sont repris dans le rapport HTML.", "These results are included in the HTML report."));
    apply
}

/// Editor of the planned change (stored in the scenario). Returns `true` when
/// something changed.
fn future_editor(ui: &mut Ui, config: &mut Configuration, lang: Language) -> bool {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let mut changed = false;
    let kind = config.release.kind;
    let current = config.clone();
    let f = &mut config.future;
    card(ui, |ui| {
        section_title(ui, t("Installation future", "Future installation"));
        hint(ui, t(
            "Décrivez ce qui change (machine, bouteilles ou réservoir ajoutés, nouvelle ventilation). Le local, l'hypothèse de fuite et les seuils restent ceux du scénario : la comparaison isole le changement. Ajouter des bouteilles ne signifie pas qu'elles fuient toutes à la fois.",
            "Describe what changes (added machine, cylinders or tank, new ventilation). The room, the leak hypothesis and the levels stay those of the scenario: the comparison isolates the change. Adding cylinders does not mean that they all leak at once.",
        ));
        changed |= ui
            .checkbox(
                &mut f.enabled,
                t(
                    "Étudier l'installation future (incluse dans le rapport)",
                    "Study the future installation (included in the report)",
                ),
            )
            .changed();
        ui.horizontal(|ui| {
            ui.label(t("Description :", "Description:"));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut f.label).desired_width(360.0).hint_text(t(
                    "ex. ajout d'un congélateur −80 °C alimenté en LN₂",
                    "e.g. added −80 °C freezer fed with LN₂",
                )))
                .changed();
        });
        let opt = |ui: &mut Ui,
                   label: &str,
                   value: &mut Option<f64>,
                   now: f64,
                   range: std::ops::RangeInclusive<f64>,
                   unit: &str,
                   dec: usize,
                   speed: f64|
         -> bool {
            let mut ch = false;
            ui.horizontal(|ui| {
                let mut on = value.is_some();
                if ui.checkbox(&mut on, "").changed() {
                    *value = if on { Some(now) } else { None };
                    ch = true;
                }
                ui.label(label);
                ui.label(
                    RichText::new(format!("{} {}", t("actuel :", "current:"), fmt_unit(now, dec, unit, lang)))
                        .color(MUTED),
                );
                if let Some(v) = value {
                    ui.label("→");
                    ch |= ui
                        .add(
                            DragValue::new(v)
                                .speed(speed)
                                .range(range)
                                .custom_formatter(move |x, _| fmt_num(x, dec, lang).replace('\u{202f}', ""))
                                .custom_parser(|s| {
                                    s.replace(',', ".").replace(['\u{202f}', ' ', '\u{a0}'], "").parse().ok()
                                }),
                        )
                        .changed();
                    ui.label(unit);
                }
            });
            ch
        };
        if kind == anoxia_core::config::ReleaseKind::ConstantFlow {
            changed |= opt(
                ui,
                t("Débit imposé", "Imposed flow"),
                &mut f.constant_flow_m3h,
                current.release.constant_flow_m3h,
                0.0001..=1e6,
                "m³/h",
                1,
                1.0,
            );
        } else {
            let mut count = f.source_count.map(f64::from);
            changed |= opt(
                ui,
                t("Nombre de récipients", "Number of containers"),
                &mut count,
                f64::from(current.source.count),
                1.0..=100.0,
                "",
                0,
                1.0,
            );
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let new_count = count.map(|c| c.round().max(1.0) as u32);
            if new_count != f.source_count {
                f.source_count = new_count;
                changed = true;
            }
            changed |= opt(
                ui,
                t("Capacité en eau", "Water capacity"),
                &mut f.volume_liters,
                current.source.volume_liters,
                0.1..=100_000.0,
                "L",
                1,
                1.0,
            );
            if !current.gas.is_liquefied() {
                changed |= opt(
                    ui,
                    t("Pression", "Pressure"),
                    &mut f.pressure_bar_g,
                    current.source.pressure_bar_g,
                    0.1..=700.0,
                    "bar",
                    0,
                    1.0,
                );
            }
            if kind.uses_orifice() {
                changed |= opt(
                    ui,
                    t("Diamètre de fuite", "Leak diameter"),
                    &mut f.diameter_mm,
                    current.release.diameter_mm,
                    0.05..=100.0,
                    "mm",
                    2,
                    0.1,
                );
            }
        }
        let now_vent = if current.room.mechanical_enabled { current.room.mechanical_flow_m3h } else { 0.0 };
        changed |= opt(
            ui,
            t("Ventilation mécanique", "Mechanical ventilation"),
            &mut f.mechanical_flow_m3h,
            now_vent,
            0.0..=1e6,
            "m³/h",
            0,
            10.0,
        );
        if f.enabled && f.is_empty() {
            hint(ui, t("Cochez au moins une valeur à modifier.", "Tick at least one value to change."));
        } else if f.enabled {
            // Same validation as the engine, shown where the value is typed
            if let Err(e) = f.apply(&current).validate() {
                theme::callout(
                    ui,
                    theme::DANGER,
                    &format!("{} {e}", t("Configuration future invalide :", "Invalid future configuration:")),
                );
            }
        }
    });
    changed
}

/// The four configurations: current / future × ventilation running / failed.
fn before_after_view(ui: &mut Ui, variants: &[Variant], config: &Configuration, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    if !config.future.enabled || config.future.is_empty() {
        hint(
            ui,
            t(
                "Activez l'étude et indiquez au moins un changement ci-dessus.",
                "Enable the study and set at least one change above.",
            ),
        );
        return;
    }
    let (a1, a2) = config.thresholds.levels();
    card(ui, |ui| {
        section_title(
            ui,
            t(
                "Oxygène : actuel / futur, ventilation en service / en panne",
                "Oxygen: current / future, ventilation running / failed",
            ),
        );
        let diffs = config.future.differences(config, lang);
        if !diffs.is_empty() {
            hint(ui, &format!("{} {}", t("Changements :", "Changes:"), diffs.join(" ; ")));
        }
        let mem = egui::Id::new("ba_full");
        let mut full = ui.ctx().data(|d| d.get_temp::<bool>(mem)).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("Fenêtre :", "Window:")).small().color(MUTED));
            if ui.selectable_label(!full, t("événement", "event")).clicked() {
                full = false;
            }
            if ui.selectable_label(full, t("durée complète", "full duration")).clicked() {
                full = true;
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(mem, full));
        let curves: Vec<Curve> = variants
            .iter()
            .enumerate()
            .map(|(i, v)| Curve {
                name: v.label.clone(),
                x: &v.result.time,
                y: &v.result.o2,
                color: PALETTE[i % PALETTE.len()],
                width: if i == 0 { 3.0 } else { 1.8 },
                dashed: i % 2 == 1,
            })
            .collect();
        let window = variants.iter().map(|v| plots::event_window(&v.result.summary)).fold(60.0, f64::max);
        let ymin = variants.iter().map(|v| v.result.summary.min_o2).fold(21.0, f64::min);
        let spec = PlotSpec {
            id: "ba_plot",
            y_label: t("O₂ (% vol)", "O₂ (% vol)"),
            y_unit: "%",
            y_include: Some((ymin.min(a1) - 0.5, 21.2)),
            height: 480.0,
            x_end: (!full).then_some(window),
            markers: &[],
        };
        plots::time_plot(ui, &spec, &curves, &plots::o2_thresholds(a1, a2, lang), lang);
    });
    let headers = [
        t("Configuration", "Configuration"),
        t("O₂ min.", "Min O₂"),
        t("Alarme 1 (min:s)", "Alarm 1 (min:s)"),
        t("Alarme 2 (min:s)", "Alarm 2 (min:s)"),
        t("Durée sous alarme 1", "Time below alarm 1"),
        t("Retour > 19,5 % (min:s)", "Back > 19.5% (min:s)"),
        t("Série qui fixe le minimum", "Series setting the minimum"),
    ];
    egui::Grid::new("ba_table").striped(true).num_columns(headers.len()).spacing([20.0, 4.0]).show(ui, |ui| {
        for h in &headers {
            ui.label(RichText::new(*h).strong());
        }
        ui.end_row();
        for v in variants {
            let s = &v.result.summary;
            let first = |l: f64| s.o2_crossing(l).and_then(|c| c.first).map_or_else(|| "—".into(), fmt_clock);
            ui.label(RichText::new(&v.label).strong());
            ui.label(fmt_pct(s.min_o2, 1, lang));
            ui.label(first(a1));
            ui.label(first(a2));
            ui.label(s.o2_crossing(a1).filter(|c| c.reached()).map_or_else(
                || "—".into(),
                |c| format!("{}{}", if c.censored { "≥ " } else { "" }, fmt_duration(c.total, lang)),
            ));
            ui.label(match s.o2_crossing(19.5) {
                Some(c) if c.reached() => c.back.map_or_else(
                    || t("non observé avant la fin du calcul", "not observed before the end of the computation").into(),
                    fmt_clock,
                ),
                _ => "—".into(),
            });
            ui.label(s.governing.label(lang));
            ui.end_row();
        }
    });
    if variants.len() == 4 {
        let m = |i: usize| fmt_pct(variants[i].result.summary.min_o2, 1, lang);
        hint(
            ui,
            &format!(
                "{} {} → {} ({}) ; {} → {} ({}).",
                t("Après modification, O₂ minimal :", "After the change, minimum O₂:"),
                m(0),
                m(2),
                t("ventilation en service", "ventilation running"),
                m(1),
                m(3),
                t("en panne", "failure")
            ),
        );
    }
    hint(
        ui,
        t(
            "L'étude figure dans le rapport HTML (recalculée à l'export).",
            "The study is included in the HTML report (recomputed at export).",
        ),
    );
}
