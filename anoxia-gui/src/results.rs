//! Results tab.

use crate::app::AnoxiaApp;
use crate::plots::{self, Curve, PlotSpec, Threshold};
use crate::theme::{self, card, hint, section_title, MUTED};
use anoxia_core::i18n::{fmt_clock, fmt_duration, fmt_pct, fmt_unit};
use anoxia_core::report::FailureSummary;
use anoxia_core::safety::{disclaimer, model_limitations, RiskLevel, WarningLevel};
use anoxia_core::{Language, SimulationRun};
use eframe::egui::{self, Color32, RichText, Ui};

/// Sections reachable from the quick navigation bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Chart,
    Thresholds,
    Recommendations,
    Checks,
}

pub fn show(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    if let Some(err) = &app.error {
        theme::callout(ui, theme::DANGER, &format!("{} {err}", t("Paramètres invalides :", "Invalid inputs:")));
    }
    let stale = app.stale();
    let Some(run) = app.run.clone() else {
        ui.label(t(
            "Renseignez les paramètres puis cliquez sur « Calculer » (F5).",
            "Fill in the inputs, then click \"Compute\" (F5).",
        ));
        return;
    };
    if stale {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t(
                    "⚠ Les résultats ne correspondent plus aux paramètres saisis.",
                    "⚠ The results no longer match the inputs.",
                ))
                .color(theme::WARNING),
            );
            if ui.button(t("Recalculer (F5)", "Recompute (F5)")).clicked() {
                app.compute();
            }
        });
    }

    // Quick navigation inside the (long) results page
    let nav_id = egui::Id::new("results_nav");
    let mut goto: Option<Section> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(t("Aller à :", "Go to:")).small().color(MUTED));
        for (sec, fr, en) in [
            (Section::Chart, "Graphique", "Chart"),
            (Section::Thresholds, "Seuils", "Thresholds"),
            (Section::Recommendations, "Recommandations", "Recommendations"),
            (Section::Checks, "Contrôles et limites", "Checks and limits"),
        ] {
            if ui.small_button(lang.pick(fr, en)).clicked() {
                goto = Some(sec);
            }
        }
    });
    if goto.is_some() {
        ui.ctx().data_mut(|d| d.insert_temp(nav_id, goto));
    }
    let target: Option<Section> = ui.ctx().data(|d| d.get_temp::<Option<Section>>(nav_id)).flatten();
    let scroll_here = |ui: &mut Ui, sec: Section| {
        if target == Some(sec) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
            ui.ctx().data_mut(|d| d.insert_temp::<Option<Section>>(nav_id, None));
        }
    };

    let failure = app.failure.clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if stale {
            ui.set_opacity(0.55);
        }
        banner(ui, &run, lang);
        ui.add_space(6.0);
        key_messages(ui, &run, failure.as_ref(), lang);
        ui.add_space(6.0);
        // The regulatory bounding approach is already stated in the banner.
        let skip = run
            .assessment
            .has(anoxia_core::safety::Flag::DisplacementMoreSevere)
            .then(|| {
                run.assessment
                    .warnings(lang)
                    .into_iter()
                    .zip(&run.assessment.flags)
                    .find(|(_, f)| **f == anoxia_core::safety::Flag::DisplacementMoreSevere)
                    .map(|(w, _)| w)
            })
            .flatten();
        for (level, w) in run.assessment.warnings_ranked(lang).into_iter().filter(|(_, w)| Some(w) != skip.as_ref()) {
            let (color, title) = match level {
                WarningLevel::Danger => (theme::DANGER, t("Danger", "Danger")),
                WarningLevel::Warning => (theme::WARNING, t("Point d'attention", "Attention")),
                WarningLevel::Info => (theme::MUTED, t("Information", "Information")),
            };
            theme::callout_titled(ui, color, title, &w);
        }
        ui.add_space(6.0);
        scroll_here(ui, Section::Chart);
        o2_chart(ui, &run, lang);
        if run.result.co2.is_some() {
            co2_chart(ui, &run, lang);
        }
        source_charts(ui, &run, lang);
        scroll_here(ui, Section::Thresholds);
        crossings(ui, &run, lang);
        scroll_here(ui, Section::Recommendations);
        recommendations(ui, &run, lang);
        scroll_here(ui, Section::Checks);
        checks(ui, &run, lang);

        ui.add_space(8.0);
        egui::CollapsingHeader::new(
            RichText::new(t("Hypothèses et limites du modèle", "Model assumptions and limitations")).strong(),
        )
        .default_open(false)
        .show(ui, |ui| {
            for l in model_limitations(lang) {
                ui.label(format!("• {l}"));
            }
        });
        ui.add_space(6.0);
        hint(ui, &disclaimer(lang));
        hint(ui, &format!("{} {}", t("Calcul du", "Computed on"), app.run_time));
    });
}

/// The key messages in plain language (same sentences as in the report).
fn key_messages(ui: &mut Ui, run: &SimulationRun, failure: Option<&FailureSummary>, lang: Language) {
    let messages = anoxia_core::report::key_messages(run, failure, lang);
    card(ui, |ui| {
        section_title(ui, lang.pick("En clair", "In plain terms"));
        for m in messages {
            ui.label(RichText::new(format!("•  {m}")).size(15.0));
            ui.add_space(2.0);
        }
    });
}

/// Result tile; `width` is imposed so that tiles wrap instead of overflowing.
fn kpi(ui: &mut Ui, width: f32, label: &str, value: &str, sub: &str, color: Color32) {
    let dark = ui.visuals().dark_mode;
    let (fill, text, muted) = if dark {
        (Color32::from_rgb(40, 44, 48), Color32::from_rgb(232, 234, 236), Color32::from_rgb(168, 176, 182))
    } else {
        (Color32::WHITE, Color32::from_rgb(52, 64, 74), Color32::from_rgb(92, 104, 114))
    };
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.45)))
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.vertical(|ui| {
                ui.add(egui::Label::new(RichText::new(label).size(12.0).color(text)).truncate());
                ui.add(
                    egui::Label::new(RichText::new(value).size(22.0).strong().color(theme::readable(color, dark)))
                        .truncate(),
                );
                ui.add(
                    egui::Label::new(RichText::new(if sub.is_empty() { " " } else { sub }).size(11.0).color(muted))
                        .truncate(),
                );
            });
        });
}

fn banner(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let a = &run.assessment;
    let s = &run.result.summary;
    let color = theme::risk_color(a.overall);
    let dark = ui.visuals().dark_mode;
    let headline = theme::readable(color, dark);
    let body = ui.visuals().text_color();
    // Light card tinted with the risk colour, a solid colour bar on the left:
    // the level is read at a glance without a wall of saturated colour.
    let frame = egui::Frame::none()
        .fill(color.gamma_multiply(if dark { 0.28 } else { 0.09 }))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.5)))
        .rounding(10.0)
        .inner_margin(egui::Margin { left: 22.0, right: 16.0, top: 14.0, bottom: 14.0 });
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(a.headline(lang)).size(24.0).strong().color(headline));
        ui.label(RichText::new(a.conclusion(lang)).size(14.0).color(body));
        // Regulatory bounding approach, when it is more severe than the simulation
        let displacement_level = RiskLevel::from_o2(a.displacement_o2, a.alarm1, a.alarm2);
        if displacement_level > a.o2_level && run.config.release.kind != anoxia_core::config::ReleaseKind::Instantaneous {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "{} {} ({} O₂)",
                    t(
                        "Approche enveloppe réglementaire (INRS / BCGA, tout l'inventaire d'un coup, sans ventilation) :",
                        "Regulatory bounding approach (INRS / BCGA, whole inventory at once, no ventilation):"
                    ),
                    displacement_level.label(lang),
                    fmt_pct(a.displacement_o2, 1, lang)
                ))
                .size(13.0)
                .strong()
                .color(body),
            );
        }
        ui.add_space(10.0);

        let not_reached = t("non atteint", "not reached");
        let time_or = |v: Option<f64>| v.map_or_else(|| not_reached.to_string(), |x| fmt_duration(x, lang));
        let after = t("après le début de la fuite", "after the start of the release");
        let sub_after = |v: Option<f64>| if v.is_some() { after.to_string() } else { String::new() };
        let mut tiles: Vec<(String, String, String)> = vec![
            (
                t("O₂ minimal", "Minimum O₂").into(),
                fmt_pct(s.min_o2, if (s.min_o2 - a.alarm2).abs() < 0.1 || (s.min_o2 - a.alarm1).abs() < 0.1 { 2 } else { 1 }, lang),
                if s.time_at_min < 1.0 {
                    t("atteint immédiatement", "reached immediately").to_string()
                } else {
                    format!("{} {}", t("atteint après", "reached after"), fmt_duration(s.time_at_min, lang))
                },
            ),
            (
                t("O₂ près de la fuite", "O₂ next to the release").into(),
                a.near.map_or_else(|| t("non calculé", "not computed").to_string(), |n| fmt_pct(n.min_o2, 1, lang)),
                a.near.map_or_else(String::new, |n| {
                    format!("{} {}", t("champ proche, rayon", "near field, radius"), fmt_unit(n.radius_m, 1, "m", lang))
                }),
            ),
            (
                format!("{} ({})", t("Alarme 1", "Alarm 1"), fmt_pct(a.alarm1, 1, lang)),
                time_or(a.t_alarm1),
                sub_after(a.t_alarm1),
            ),
            (
                format!("{} ({})", t("Alarme 2", "Alarm 2"), fmt_pct(a.alarm2, 1, lang)),
                time_or(a.t_alarm2),
                sub_after(a.t_alarm2),
            ),
            (
                format!("{} ({})", t("Danger vital", "Life-threatening"), fmt_pct(10.0, 0, lang)),
                time_or(a.t_10),
                sub_after(a.t_10),
            ),
        ];
        if let Some(co2) = s.max_co2_percent {
            tiles.push((
                t("CO₂ maximal (toxique)", "Peak CO₂ (toxic)").into(),
                fmt_pct(co2, 1, lang),
                a.co2_level.map(|c| c.label(lang).to_string()).unwrap_or_default(),
            ));
        }
        tiles.push((
            t("Gaz libérable", "Releasable gas").into(),
            fmt_unit(s.releasable_volume_m3, 1, "m³", lang),
            if let Some(ti) = run.config.isolation_time() {
                format!("{} {}", t("fuite isolée après", "leak isolated after"), fmt_duration(ti, lang))
            } else if run.config.release.kind == anoxia_core::config::ReleaseKind::ConstantFlow {
                format!("{} {}", t("rejet pendant", "released over"), fmt_duration(run.config.release.constant_duration_min * 60.0, lang))
            } else {
                s.release_duration.map_or_else(
                    || t("vidange > durée simulée", "release > simulated time").to_string(),
                    |d| format!("{} {}", t("vidange en", "released in"), fmt_duration(d, lang)),
                )
            },
        ));

        // Responsive grid: as many columns as fit, tiles never overflow.
        let gap = 8.0;
        let avail = ui.available_width();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let cols = ((avail + gap) / (165.0 + gap)).floor().clamp(1.0, tiles.len() as f32) as usize;
        #[allow(clippy::cast_precision_loss)]
        let width = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        for row in tiles.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, value, sub) in row {
                    kpi(ui, width, label, value, sub, color);
                }
            });
            ui.add_space(gap * 0.5);
        }
    });
    let rect = response.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 7.0, rect.max.y)),
        egui::Rounding { nw: 10.0, sw: 10.0, ne: 0.0, se: 0.0 },
        color,
    );
}

/// Time window toggle stored per plot ("event" focus or full duration).
fn window_toggle(ui: &mut Ui, id: &str, lang: Language) -> bool {
    let mem = egui::Id::new(id).with("full_window");
    let mut full = ui.ctx().data(|d| d.get_temp::<bool>(mem)).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.label(RichText::new(lang.pick("Fenêtre :", "Window:")).small().color(MUTED));
        if ui.selectable_label(!full, lang.pick("événement", "event")).clicked() {
            full = false;
        }
        if ui.selectable_label(full, lang.pick("durée complète", "full duration")).clicked() {
            full = true;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(mem, full));
    full
}

fn o2_chart(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let r = &run.result;
    let a = &run.assessment;
    card(ui, |ui| {
        ui.horizontal(|ui| {
            section_title(ui, t("Concentration en oxygène", "Oxygen concentration"));
        });
        let full = window_toggle(ui, "o2_plot", lang);
        let mut curves = Vec::new();
        if let (Some(zone), Some(other)) = (&r.o2_zone, &r.o2_other) {
            let zone_name = match a.buoyancy {
                anoxia_core::gas::Buoyancy::Light => t("Zone haute (accumulation)", "Upper zone (accumulation)"),
                _ => t("Zone basse (accumulation)", "Lower zone (accumulation)"),
            };
            curves.push(Curve {
                name: zone_name.into(),
                x: &r.time,
                y: zone,
                color: theme::ACCENT,
                width: 2.6,
                dashed: false,
            });
            curves.push(Curve {
                name: t("Reste du local", "Rest of room").into(),
                x: &r.time,
                y: other,
                color: Color32::from_rgb(0, 131, 143),
                width: 1.5,
                dashed: true,
            });
            curves.push(Curve {
                name: t("Modèle bien mélangé", "Well-mixed model").into(),
                x: &r.time,
                y: &r.o2_mixed,
                color: Color32::from_rgb(84, 110, 122),
                width: 2.0,
                dashed: true,
            });
        } else {
            curves.push(Curve {
                name: t("O₂ (bien mélangé)", "O₂ (well-mixed)").into(),
                x: &r.time,
                y: &r.o2_mixed,
                color: theme::ACCENT,
                width: 2.6,
                dashed: false,
            });
        }
        let near_label = format!(
            "{} (r = {})",
            t("Près de la fuite (champ proche)", "Next to the release (near field)"),
            fmt_unit(run.config.model.near_field_radius_m, 1, "m", lang)
        );
        if let Some(near) = &r.o2_near {
            curves.push(Curve {
                name: near_label,
                x: &r.time,
                y: near,
                color: Color32::from_rgb(230, 81, 0),
                width: 1.6,
                dashed: true,
            });
        }
        let low = a.alarm1.min(r.summary.min_o2).min(r.summary.min_o2_near.unwrap_or(21.0));
        let markers = plots::event_markers(run, lang);
        let spec = PlotSpec {
            id: "o2_plot",
            y_label: t("O₂ (% vol)", "O₂ (% vol)"),
            y_unit: "%",
            y_include: Some((low - 0.5, 21.2)),
            height: 340.0,
            x_end: (!full).then(|| plots::event_window(&r.summary)),
            markers: &markers,
        };
        plots::time_plot(ui, &spec, &curves, &plots::o2_thresholds(a.alarm1, a.alarm2, lang), lang);
        hint(
            ui,
            t(
                "Survoler une courbe pour lire les valeurs — glisser : déplacer — Ctrl + molette : zoom — double-clic : vue initiale.",
                "Hover a curve to read values — drag: pan — Ctrl + wheel: zoom — double-click: reset view.",
            ),
        );
    });
}

fn co2_chart(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let r = &run.result;
    let Some(co2) = &r.co2 else { return };
    card(ui, |ui| {
        section_title(ui, t("Concentration en CO₂ (toxique)", "CO₂ concentration (toxic)"));
        let full = window_toggle(ui, "co2_plot", lang);
        let curves = [Curve {
            name: "CO₂".into(),
            x: &r.time,
            y: co2,
            color: Color32::from_rgb(55, 71, 79),
            width: 2.5,
            dashed: false,
        }];
        let th = [
            Threshold { y: 0.5, name: t("VLEP 8 h (0,5 %)", "8-h OEL (0.5%)").into(), color: theme::WARNING },
            Threshold { y: 3.0, name: t("VLCT (3 %)", "STEL (3%)").into(), color: theme::DANGER },
            Threshold { y: 4.0, name: t("DIVS / IDLH (4 %)", "IDLH (4%)").into(), color: theme::VITAL },
        ];
        // Peaks far above the limits would crush the threshold lines: offer a
        // view clipped to 0–10 % (the peak value is given in the tiles).
        let peak = co2.iter().copied().fold(0.0, f64::max);
        let zoom_mem = egui::Id::new("co2_zoom");
        let mut zoom = ui.ctx().data(|d| d.get_temp::<bool>(zoom_mem)).unwrap_or(peak > 12.0);
        if peak > 12.0 {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Échelle :", "Scale:")).small().color(MUTED));
                if ui.selectable_label(zoom, t("seuils (0–10 %)", "limits (0–10%)")).clicked() {
                    zoom = true;
                }
                if ui.selectable_label(!zoom, t("complète", "full")).clicked() {
                    zoom = false;
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(zoom_mem, zoom));
        }
        let clipped: Vec<f64>;
        let shown_curves = if zoom && peak > 12.0 {
            clipped = co2.iter().map(|v| v.min(10.5)).collect();
            vec![Curve {
                name: format!("CO₂ ({} {})", t("pic", "peak"), fmt_pct(peak, 1, lang)),
                x: &r.time,
                y: &clipped,
                color: Color32::from_rgb(55, 71, 79),
                width: 2.5,
                dashed: false,
            }]
        } else {
            curves.into_iter().collect()
        };
        let markers = plots::event_markers(run, lang);
        let spec = PlotSpec {
            id: "co2_plot",
            y_label: t("CO₂ (% vol)", "CO₂ (% vol)"),
            y_unit: "%",
            y_include: Some((0.0, 5.0)),
            height: 240.0,
            x_end: (!full).then(|| plots::event_window(&r.summary)),
            markers: &markers,
        };
        plots::time_plot(ui, &spec, &shown_curves, &th, lang);
    });
}

fn source_charts(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let r = &run.result;
    if !run.config.release.kind.uses_orifice() {
        return;
    }
    egui::CollapsingHeader::new(
        RichText::new(t("Source : débit de fuite et pression du récipient", "Source: leak flow and container pressure")).strong(),
    )
    .default_open(false)
    .show(ui, |ui| {
        // Show the release itself, not the whole recovery period.
        let t_end = r.summary.release_duration.unwrap_or(r.summary.simulated_duration) * 1.15;
        let n = r.time.iter().position(|&t| t > t_end).map_or(r.time.len(), |i| (i + 1).min(r.time.len()));
        let time = &r.time[..n];
        let flow_kgh: Vec<f64> = r.mass_flow[..n].iter().map(|q| q * 3600.0).collect();
        let pressure = &r.source_pressure_bar_g[..n];
        ui.columns(2, |cols| {
            let spec = PlotSpec {
                id: "flow_plot",
                y_label: t("Débit (kg/h)", "Flow (kg/h)"),
                y_unit: "kg/h",
                y_include: Some((0.0, 0.0)),
                height: 220.0,
                x_end: None,
                markers: &[],
            };
            let curve =
                Curve { name: t("Débit massique", "Mass flow").into(), x: time, y: &flow_kgh, color: theme::ACCENT, width: 2.0, dashed: false };
            plots::time_plot(&mut cols[0], &spec, &[curve], &[], lang);
            let spec = PlotSpec {
                id: "pressure_plot",
                y_label: t("Pression (bar rel.)", "Pressure (bar g)"),
                y_unit: "bar",
                y_include: Some((0.0, 0.0)),
                height: 220.0,
                x_end: None,
                markers: &[],
            };
            let curve = Curve {
                name: t("Pression relative", "Gauge pressure").into(),
                x: time,
                y: pressure,
                color: Color32::from_rgb(93, 64, 55),
                width: 2.0,
                dashed: false,
            };
            plots::time_plot(&mut cols[1], &spec, &[curve], &[], lang);
        });
        hint(
            ui,
            t(
                "Vidange du récipient : le débit décroît avec la pression (régime sonique puis subsonique). Modèle isotherme ou adiabatique dans les options avancées.",
                "Container blowdown: the flow decreases with pressure (choked then subsonic). Isothermal or adiabatic model in the advanced options.",
            ),
        );
    });
}

fn crossings(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let s = &run.result.summary;
    egui::CollapsingHeader::new(RichText::new(t("Franchissement des seuils", "Threshold crossings")).strong())
        .default_open(true)
        .show(ui, |ui| {
            let a = &run.assessment;
            hint(
                ui,
                &format!(
                    "{} {} ({} {} {} {})",
                    t("Pression partielle d'O₂ minimale :", "Minimum O₂ partial pressure:"),
                    fmt_unit(a.po2_min_kpa, 1, "kPa", lang),
                    t("air ambiant", "ambient air"),
                    fmt_unit(a.po2_ambient_kpa, 1, "kPa", lang),
                    t("à", "at"),
                    fmt_unit(a.room_pressure_hpa, 0, "hPa", lang)
                ),
            );
            egui::Grid::new("crossings").striped(true).num_columns(4).spacing([24.0, 4.0]).min_col_width(120.0).show(
                ui,
                |ui| {
                    for h in [
                        t("Seuil", "Level"),
                        t("Premier franchissement (min:s)", "First crossing (min:s)"),
                        t("Retour du bon côté (min:s)", "Back on the safe side (min:s)"),
                        t("Durée au-delà du seuil", "Time beyond the level"),
                    ] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for c in &s.o2_crossings {
                        let is_alarm = (c.level - run.assessment.alarm1).abs() < 1e-9
                            || (c.level - run.assessment.alarm2).abs() < 1e-9;
                        let lvl = format!("O₂ < {}", fmt_pct(c.level, 1, lang));
                        ui.label(if is_alarm {
                            RichText::new(format!("{lvl} ◀")).strong()
                        } else {
                            RichText::new(lvl)
                        });
                        ui.label(c.first.map_or_else(|| "—".into(), fmt_clock));
                        let not_observed =
                            t("non observé avant la fin du calcul", "not observed before the end of the computation");
                        let duration = |c: &anoxia_core::solver::Crossing| {
                            if c.censored {
                                format!("≥ {}", fmt_duration(c.total, lang))
                            } else if c.reached() {
                                fmt_duration(c.total, lang)
                            } else {
                                "—".into()
                            }
                        };
                        ui.label(match (c.first, c.back) {
                            (Some(_), Some(b)) => fmt_clock(b),
                            (Some(_), None) => not_observed.into(),
                            _ => "—".into(),
                        });
                        ui.label(duration(c));
                        ui.end_row();
                    }
                    for c in &s.co2_crossings {
                        ui.label(RichText::new(format!("CO₂ > {}", fmt_pct(c.level, 1, lang))).color(theme::DANGER));
                        ui.label(c.first.map_or_else(|| "—".into(), fmt_clock));
                        ui.label(match (c.first, c.back) {
                            (Some(_), Some(b)) => fmt_clock(b),
                            (Some(_), None) => {
                                t("non observé avant la fin du calcul", "not observed before the end of the computation").into()
                            }
                            _ => "—".into(),
                        });
                        ui.label(if c.censored {
                            format!("≥ {}", fmt_duration(c.total, lang))
                        } else if c.reached() {
                            fmt_duration(c.total, lang)
                        } else {
                            "—".into()
                        });
                        ui.end_row();
                    }
                },
            );
            hint(
                ui,
                t(
                    "◀ = seuils d'alarme configurés. Temps comptés depuis le début de la fuite, en min:s (h:min:s au-delà d'une heure). « ≥ » : encore sous le seuil à la fin du calcul, la durée est un minimum.",
                    "◀ = configured alarm levels. Times from the start of the release, in min:s (h:min:s beyond one hour). \"≥\": still below the level at the end of the computation, the time is a lower bound.",
                ),
            );
            if let Some(m) = run.assessment.escape_margin {
                hint(
                    ui,
                    &format!(
                        "{} {}",
                        t(
                            "Délai indicatif entre l'alarme 1 et le niveau dangereux suivant :",
                            "Indicative time between alarm 1 and the next dangerous level:"
                        ),
                        fmt_duration(m, lang)
                    ),
                );
            }
        });
}

fn checks(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let s = &run.result.summary;
    let a = &run.assessment;
    egui::CollapsingHeader::new(
        RichText::new(t("Contrôles indépendants (cas enveloppe sans ventilation)", "Independent checks (bounding case, no ventilation)"))
            .strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        egui::Grid::new("bounds").striped(true).num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
            for h in [t("Méthode", "Method"), t("O₂", "O₂"), t("Classement", "Classification")] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            let mut row = |label: String, v: f64| {
                ui.label(label);
                ui.label(fmt_pct(v, 1, lang));
                let level = RiskLevel::from_o2(v, a.alarm1, a.alarm2);
                ui.label(RichText::new(level.label(lang)).color(theme::risk_color(level)));
                ui.end_row();
            };
            row(t("Méthode par déplacement INRS ED 155 / BCGA CP 52 (gaz réel)", "INRS ED 155 / BCGA CP 52 displacement method (real gas)").into(), s.instant_displacement_o2);
            row(t("Efflux libre NFPA 2001 / ISO 14520 (meilleure estimation)", "NFPA 2001 / ISO 14520 free efflux (best estimate)").into(), s.instant_free_efflux_o2);
            row(
                format!(
                    "{} (Vg = {})",
                    t("Calcul manuel simplifié INRS (Vg = P × V, 21 %)", "INRS simplified hand calculation (Vg = P × V, 21%)"),
                    fmt_unit(s.simplified_method_volume_m3, 2, "m³", lang)
                ),
                s.simplified_method_o2,
            );
        });
        hint(
            ui,
            t(
                "Ces trois valeurs supposent tout l'inventaire libéré d'un coup, sans aucune ventilation. Le calcul manuel simplifié est celui qu'un auditeur refera à la main.",
                "These three values assume the whole inventory released at once, with no ventilation at all. The simplified hand calculation is the one an auditor will redo by hand.",
            ),
        );
    });
}

fn recommendations(ui: &mut Ui, run: &SimulationRun, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    egui::CollapsingHeader::new(RichText::new(t("Recommandations", "Recommendations")).strong()).default_open(true).show(ui, |ui| {
        hint(ui, t(
            "Propositions du logiciel à partir du scénario. Dans l'onglet Rapport, le rédacteur coche celles qu'il retient et commente les autres.",
            "Proposals from the software for this scenario. In the Report tab, the author ticks the ones retained and comments on the others.",
        ));
        for r in run.assessment.recommendations(lang) {
            let color = theme::priority_color(r.priority);
            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.5)))
                .rounding(4.0)
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(r.priority.label(lang).to_uppercase()).small().strong().color(color));
                        ui.label(RichText::new(&r.title).strong());
                    });
                    ui.label(RichText::new(&r.text).color(if ui.visuals().dark_mode {
                        Color32::LIGHT_GRAY
                    } else {
                        Color32::from_rgb(40, 40, 40)
                    }));
                });
            ui.add_space(2.0);
        }
        ui.label(
            RichText::new(t(
                "Les recommandations sont générées automatiquement à partir du scénario : elles complètent, sans la remplacer, l'analyse d'une personne compétente.",
                "Recommendations are generated automatically from the scenario: they complement, not replace, the analysis of a competent person.",
            ))
            .small()
            .color(MUTED),
        );
    });
}
