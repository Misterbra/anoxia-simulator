//! Report tab and exports (HTML report, CSV time series).

use crate::analysis_tab::Study;
use crate::app::{AnoxiaApp, StatusKind, Tab};
use crate::theme::{self, card, hint, section_title};
use anoxia_core::i18n::fmt_pct;
use anoxia_core::report::{self, BeforeAfterSummary, FailureSummary, ReportOptions, SizingSummary};
use anoxia_core::Language;
use eframe::egui::{self, RichText, Ui};
use std::path::Path;

pub fn show(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            section_title(ui, t("Rapport d'évaluation (HTML)", "Assessment report (HTML)"));
            hint(ui, t(
                "Rapport autonome (graphiques intégrés, aucun accès Internet requis) : s'ouvre dans un navigateur, Word ou LibreOffice. Pour un PDF : Imprimer → Enregistrer au format PDF.",
                "Self-contained report (embedded charts, no Internet access needed): opens in a browser, Word or LibreOffice. For a PDF: Print → Save as PDF.",
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("Langue du rapport :", "Report language:"));
                for l in [Language::French, Language::English] {
                    ui.radio_value(&mut app.report_lang, l, l.native_name());
                }
            });
            if ui.checkbox(&mut app.open_after_export, t("Ouvrir le rapport après l'export", "Open the report after export")).changed() {
                app.persist();
            }

            let green = egui::Color32::from_rgb(27, 94, 32);
            let criterion = app.analysis.sizing_target.name(&app.config, lang);
            let mode_word = match app.analysis.sizing_mode_for(&app.config) {
                anoxia_core::config::VentilationMode::Extraction => t("extraction", "extraction"),
                anoxia_core::config::VentilationMode::Supply => t("soufflage", "supply"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "✔ {} ({} : {criterion} ; {} : {mode_word})",
                        t("Dimensionnement inclus", "Sizing included"),
                        t("critère", "criterion"),
                        t("mode", "mode")
                    ))
                    .color(green),
                );
                if ui.small_button(t("Changer le critère", "Change the criterion")).clicked() {
                    app.tab = Tab::Analyses;
                    app.analysis.study = Study::Sizing;
                }
            });
            let room = &app.config.room;
            if room.mechanical_enabled && room.mechanical_flow_m3h > 0.0 {
                ui.label(
                    RichText::new(format!(
                        "✔ {}",
                        t("Vérification « panne de ventilation » incluse", "Ventilation-failure check included")
                    ))
                    .color(green),
                );
            } else {
                ui.label(t(
                    "Vérification « panne de ventilation » sans objet (pas de ventilation mécanique dans le scénario).",
                    "Ventilation-failure check not applicable (no mechanical ventilation in the scenario).",
                ));
            }
            if app.config.future.enabled && !app.config.future.is_empty() {
                match app.config.future.apply(&app.config).validate() {
                    Ok(()) => ui.label(RichText::new(format!("✔ {}", t("Étude avant / après installation incluse", "Before / after installation study included"))).color(green)),
                    Err(e) => ui.label(
                        RichText::new(format!(
                            "✖ {} {e}",
                            t(
                                "Étude avant / après demandée mais impossible (le rapport le dira) : configuration future invalide :",
                                "Before / after study requested but impossible (the report will say so): invalid future configuration:"
                            )
                        ))
                        .color(theme::DANGER),
                    ),
                };
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t("Étude avant / après installation : non définie.", "Before / after installation study: not defined."));
                    if ui.small_button(t("La définir", "Define it")).clicked() {
                        app.tab = Tab::Analyses;
                        app.analysis.study = Study::BeforeAfter;
                    }
                });
            }
            hint(ui, t(
                "Ces analyses sont recalculées au moment de l'export, à partir des données affichées.",
                "These analyses are recomputed at export time, from the inputs shown.",
            ));
            if let Some(run) = &app.run {
                let recs = run.assessment.recommendations(lang);
                let n = recs.iter().filter(|r| app.config.report.is_retained(r.id)).count();
                ui.label(format!(
                    "{} : {n} / {} {}",
                    t("Recommandations", "Recommendations"),
                    recs.len(),
                    t("retenues par le rédacteur (validation ci-dessous).", "retained by the author (validation below).")
                ));
            }
            ui.add_space(6.0);
            let id = &app.config.identification;
            let missing: Vec<&str> = [
                (id.installation.trim().is_empty(), t("local", "room")),
                (id.user.trim().is_empty(), t("rédacteur", "author")),
                (id.reference.trim().is_empty(), t("référence", "reference")),
            ]
            .into_iter()
            .filter_map(|(m, name)| m.then_some(name))
            .collect();
            if !missing.is_empty() {
                let list = missing.join(", ");
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!(
                        "⚠ {} {list}.",
                        t("Identification incomplète – à renseigner :", "Incomplete identification – please fill in:")
                    )).color(theme::WARNING));
                    if ui.small_button(t("Compléter l'identification", "Complete the identification")).clicked() {
                        app.open_identification = true;
                    }
                });
            }
            ui.horizontal(|ui| {
                let primary = egui::Button::new(RichText::new(t("Exporter le rapport HTML…", "Export HTML report…")).strong().color(egui::Color32::WHITE))
                    .fill(theme::ACCENT)
                    .min_size(egui::vec2(220.0, 30.0));
                if ui.add(primary).clicked() {
                    export_html(app);
                }
                if ui.button(t("Exporter les séries temporelles (CSV)…", "Export time series (CSV)…")).clicked() {
                    export_csv(app);
                }
            });
            hint(ui, t(
                "CSV : une ligne par pas de temps — temps, O₂ (bien mélangé, retenu pour le classement, zones), CO₂ le cas échéant, débit massique, pression du récipient, masse rejetée. En français : séparateur « ; » et virgule décimale (Excel FR) ; en anglais : « , » et point.",
                "CSV: one row per time step — time, O₂ (well-mixed, used for classification, zones), CO₂ where relevant, mass flow, container pressure, released mass. French: \";\" separator and decimal comma (Excel FR); English: \",\" and point.",
            ));
        });

        if let Some(run) = app.run.clone() {
            recommendations_card(app, ui, &run);
        }

        if let Some(run) = &app.run {
            card(ui, |ui| {
                section_title(ui, t("Contenu du rapport", "Report contents"));
                let id = &run.config.identification;
                egui::Grid::new("report_preview").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    let empty = t("(non renseigné)", "(not filled in)");
                    let mut row = |k: &str, v: &str| {
                        ui.label(RichText::new(k).strong());
                        ui.label(if v.trim().is_empty() { empty } else { v });
                        ui.end_row();
                    };
                    row(t("Référence", "Reference"), &id.reference);
                    row(t("Site", "Site"), &id.unit);
                    row(t("Local", "Room"), &id.installation);
                    row(t("Rédacteur", "Author"), &id.user);
                    row(t("Vérificateur", "Reviewer"), &id.reviewer);
                    row(t("Gaz", "Gas"), run.config.gas.name(lang));
                    row(t("Classement", "Classification"), run.assessment.headline(lang));
                    row(t("O₂ minimal", "Minimum O₂"), &fmt_pct(run.result.summary.min_o2, 1, lang));
                });
                hint(ui, t(
                    "Sections : synthèse, messages clés en clair et classement, graphiques O₂ (et CO₂), données d'entrée, franchissements de seuils, dimensionnement, panne de ventilation, étude avant / après, recommandations retenues par le rédacteur, repères physiologiques, hypothèses et limites, commentaires, bloc de validation.",
                    "Sections: summary, plain-language key messages and classification, O₂ (and CO₂) charts, input data, threshold crossings, sizing, ventilation failure, before / after study, recommendations retained by the author, physiological reference, assumptions and limitations, comments, approval block.",
                ));
            });
        }
    });
}

/// The software proposes recommendations; the author who signs the report
/// decides which ones are retained and can justify the others. The choices
/// are stored in the scenario file.
fn recommendations_card(app: &mut AnoxiaApp, ui: &mut Ui, run: &anoxia_core::SimulationRun) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let recs = run.assessment.recommendations(lang);
    card(ui, |ui| {
        section_title(ui, t("Recommandations : validation par le rédacteur", "Recommendations: author's validation"));
        hint(ui, t(
            "Le logiciel propose ces mesures à partir du scénario ; c'est le rédacteur qui décide de celles qui figurent dans le rapport. Décochez une proposition pour l'écarter et indiquez pourquoi (« déjà en place », « sans objet car… ») : les propositions écartées sont listées en fin de section du rapport avec cette justification. Ces choix sont enregistrés avec le scénario.",
            "The software proposes these measures from the scenario; the author decides which ones appear in the report. Untick a proposal to exclude it and say why (\"already in place\", \"not applicable because…\"): excluded proposals are listed at the end of the report section with that justification. The choices are saved with the scenario.",
        ));
        ui.horizontal(|ui| {
            if ui.small_button(t("Tout retenir", "Retain all")).clicked() {
                app.config.report.excluded.clear();
            }
            if ui.small_button(t("Tout écarter", "Exclude all")).clicked() {
                for r in &recs {
                    app.config.report.set_retained(r.id, false);
                }
            }
            let n = recs.iter().filter(|r| app.config.report.is_retained(r.id)).count();
            ui.label(RichText::new(format!("{n} / {} {}", recs.len(), t("retenues", "retained"))).color(theme::MUTED));
        });
        ui.add_space(4.0);
        for r in &recs {
            let color = theme::priority_color(r.priority);
            let mut retained = app.config.report.is_retained(r.id);
            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(if retained { 0.5 } else { 0.2 })))
                .rounding(4.0)
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut retained, "").changed() {
                            app.config.report.set_retained(r.id, retained);
                        }
                        ui.label(RichText::new(r.priority.label(lang).to_uppercase()).small().strong().color(color));
                        ui.label(RichText::new(&r.title).strong());
                        if !retained {
                            ui.label(
                                RichText::new(t("(écartée du rapport)", "(excluded from the report)"))
                                    .color(theme::MUTED),
                            );
                        }
                    });
                    ui.label(RichText::new(&r.text).small().color(theme::MUTED));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t("Commentaire du rédacteur :", "Author's comment:")).small());
                        let mut note = app.config.report.note(r.id).to_string();
                        let edit = egui::TextEdit::singleline(&mut note).desired_width(f32::INFINITY).hint_text(t(
                            "déjà en place, sans objet car…",
                            "already in place, not applicable because…",
                        ));
                        if ui.add(edit).changed() {
                            app.config.report.set_note(r.id, &note);
                        }
                    });
                });
            ui.add_space(4.0);
        }
        ui.label(
            RichText::new(t(
                "Mesures complémentaires (rédigées par le rédacteur)",
                "Additional measures (written by the author)",
            ))
            .strong(),
        );
        ui.add(
            egui::TextEdit::multiline(&mut app.config.report.additional).desired_rows(3).desired_width(f32::INFINITY),
        );
    });
}

fn ensure_run(app: &mut AnoxiaApp) -> bool {
    if app.run.is_none() || app.stale() {
        app.compute();
    }
    if app.error.is_some() || app.run.is_none() {
        let msg =
            app.t("Export impossible : paramètres invalides.", "Export not possible: invalid inputs.").to_string();
        app.set_status(msg, StatusKind::Error);
        return false;
    }
    true
}

fn open_file(path: &Path) {
    // `cmd /C start "" <path>` opens with the default application and accepts
    // commas and spaces in the path (Explorer splits arguments on commas).
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

pub fn export_html(app: &mut AnoxiaApp) {
    if !ensure_run(app) {
        return;
    }
    let Some(path) =
        rfd::FileDialog::new().add_filter("HTML", &["html"]).set_file_name(app.suggested_name("html")).save_file()
    else {
        return;
    };
    let Some(run) = app.run.as_ref() else { return };
    // Sizing: reuse the Analyses result when it matches, otherwise compute it
    // now (a few hundred milliseconds at most) so that the report is complete.
    let sizing = app
        .analysis
        .sizing_for(&run.config)
        .map(|(v, d, c)| SizingSummary {
            required_ventilation: v,
            max_diameter: d,
            criterion_o2: c.min_o2,
            mode: app.analysis.sizing_mode_for(&run.config),
        })
        .or_else(|| {
            let mut c = run.config.clone();
            c.room.mechanical_mode = app.analysis.sizing_mode_for(&run.config);
            SizingSummary::compute(&c, app.analysis.sizing_target.level(&run.config)).ok()
        });
    let failure = FailureSummary::compute(&run.config);
    let before_after = BeforeAfterSummary::compute(&run.config, app.report_lang);
    let opts =
        ReportOptions { lang: app.report_lang, generated_at: app.run_time.clone(), sizing, failure, before_after };
    let html = report::html(run, &opts);
    match std::fs::write(&path, html) {
        Ok(()) => {
            let msg = format!("{} {}", app.t("Rapport exporté :", "Report exported:"), path.display());
            app.set_status(msg, StatusKind::Success);
            if app.open_after_export {
                open_file(&path);
            }
        }
        Err(e) => {
            let msg = format!("{} {e}", app.t("Échec de l'export :", "Export failed:"));
            app.set_status(msg, StatusKind::Error);
        }
    }
}

pub fn export_csv(app: &mut AnoxiaApp) {
    if !ensure_run(app) {
        return;
    }
    let Some(path) =
        rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(app.suggested_name("csv")).save_file()
    else {
        return;
    };
    let Some(run) = app.run.as_ref() else { return };
    // Readable headers with units; ";" and decimal comma in French (Excel FR).
    let out = report::csv_text(run, app.lang);
    let rows = run.result.time.len();
    match std::fs::write(&path, out) {
        Ok(()) => {
            let msg = format!(
                "{} ({rows} {}) : {}",
                app.t("Séries exportées", "Series exported"),
                app.t("lignes", "rows"),
                path.display()
            );
            app.set_status(msg, StatusKind::Success);
        }
        Err(e) => {
            let msg = format!("{} {e}", app.t("Échec de l'export :", "Export failed:"));
            app.set_status(msg, StatusKind::Error);
        }
    }
}
