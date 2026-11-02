//! Application state and main loop.

use crate::analysis_tab::AnalysisState;
use crate::settings::Settings;
use crate::{analysis_tab, examples, help, inputs, report_tab, results, theme, tutorial};
use anoxia_core::report::FailureSummary;
use anoxia_core::{Configuration, Language, SimulationRun};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Main tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Results,
    Analyses,
    Report,
}

/// Status message severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Success,
    Error,
}

/// Action waiting for a "discard unsaved changes?" confirmation.
#[derive(Debug, Clone)]
pub enum Pending {
    New,
    Open,
    Example(usize),
    Quit,
}

pub struct AnoxiaApp {
    pub lang: Language,
    pub dark: bool,
    pub config: Configuration,
    /// Last successful run (inputs are stored inside).
    pub run: Option<Arc<SimulationRun>>,
    /// Validation / simulation error for the current inputs.
    pub error: Option<String>,
    /// Recompute automatically when inputs change.
    pub auto_compute: bool,
    /// Formatted date/time of the last run.
    pub run_time: String,
    pub tab: Tab,
    pub file_path: Option<PathBuf>,
    /// Configuration as last saved/loaded (dirty tracking).
    pub saved_config: Configuration,
    pub status: Option<(String, StatusKind, Instant)>,
    pub show_about: bool,
    pub show_guide: bool,
    pub show_validation: bool,
    pub show_tutorial: bool,
    pub settings: Settings,
    pub validation_cache: Option<Vec<anoxia_core::validation::Case>>,
    pub pending: Option<Pending>,
    pub allow_close: bool,
    pub analysis: AnalysisState,
    pub last_change: Instant,
    /// Room dimension helper (L, W, H)
    pub dims: [f64; 3],
    pub report_lang: Language,
    pub open_after_export: bool,
    title: String,
    /// Duration of the last simulation (ms): slow scenarios are recomputed
    /// only once the inputs are stable, to keep the interface fluid.
    last_compute_ms: f64,
    pending_compute: bool,
    frames: u32,
    /// Expand the identification section once (requested from the Report tab)
    pub open_identification: bool,
    /// Ventilation-failure check for the current run (mechanical ventilation only)
    pub failure: Option<FailureSummary>,
}

impl AnoxiaApp {
    pub fn new() -> Self {
        let settings = Settings::load();
        let config = examples::default_config();
        let mut app = Self {
            lang: settings.lang,
            dark: settings.dark,
            saved_config: config.clone(),
            config,
            run: None,
            error: None,
            auto_compute: true,
            run_time: String::new(),
            tab: Tab::Results,
            file_path: None,
            status: None,
            show_about: false,
            show_guide: false,
            show_validation: false,
            show_tutorial: settings.show_tutorial_at_start,
            validation_cache: None,
            pending: None,
            allow_close: false,
            analysis: AnalysisState::default(),
            last_change: Instant::now(),
            dims: [5.0, 4.0, 3.0],
            report_lang: settings.lang,
            open_after_export: settings.open_report_after_export,
            settings,
            title: String::new(),
            last_compute_ms: 0.0,
            pending_compute: false,
            frames: 0,
            open_identification: false,
            failure: None,
        };
        app.compute();
        app
    }

    pub fn t<'a>(&self, fr: &'a str, en: &'a str) -> &'a str {
        self.lang.pick(fr, en)
    }

    /// Save the user preferences.
    pub fn persist(&mut self) {
        self.settings.lang = self.lang;
        self.settings.dark = self.dark;
        self.settings.open_report_after_export = self.open_after_export;
        self.settings.save();
    }

    pub fn dirty(&self) -> bool {
        self.config != self.saved_config
    }

    /// Results no longer match the current inputs.
    pub fn stale(&self) -> bool {
        self.run.as_ref().is_some_and(|r| !r.config.same_physics(&self.config))
    }

    pub fn set_status(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = Some((text.into(), kind, Instant::now()));
    }

    /// Validate and run the simulation for the current inputs.
    pub fn compute(&mut self) {
        let started = Instant::now();
        self.pending_compute = false;
        let outcome = anoxia_core::run(&self.config);
        self.last_compute_ms = started.elapsed().as_secs_f64() * 1000.0;
        match outcome {
            Ok(run) => {
                self.run = Some(Arc::new(run));
                self.error = None;
                self.run_time = chrono::Local::now().format("%d/%m/%Y %H:%M").to_string();
                // Two more simulations (ventilation on / off): a few milliseconds.
                self.failure = FailureSummary::compute(&self.config);
            }
            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    pub fn file_name(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| self.t("sans titre", "untitled").to_string(), |n| n.to_string_lossy().into_owned())
    }

    // ---------- File operations ----------

    pub fn request(&mut self, action: Pending) {
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.execute(action);
        }
    }

    fn execute(&mut self, action: Pending) {
        match action {
            Pending::New => {
                self.config = examples::default_config();
                self.saved_config = self.config.clone();
                self.file_path = None;
                self.compute();
            }
            Pending::Open => self.open(),
            Pending::Example(i) => {
                if let Some(ex) = examples::all().into_iter().nth(i) {
                    let name = ex.name(self.lang).to_string();
                    self.config = ex.config;
                    self.saved_config = self.config.clone();
                    self.file_path = None;
                    self.compute();
                    self.set_status(
                        format!("{} : {name}", self.t("Exemple chargé", "Example loaded")),
                        StatusKind::Success,
                    );
                }
            }
            Pending::Quit => {
                self.allow_close = true;
            }
        }
    }

    fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(self.t("Scénario anoxie (JSON)", "Anoxia scenario (JSON)"), &["json"])
            .pick_file()
        else {
            return;
        };
        match Configuration::load_from_file(&path) {
            Ok(c) => {
                self.config = c;
                self.saved_config = self.config.clone();
                self.file_path = Some(path);
                self.compute();
                let msg = format!("{} {}", self.t("Scénario ouvert :", "Scenario opened:"), self.file_name());
                self.set_status(msg, StatusKind::Success);
            }
            Err(e) => {
                let msg = format!("{} {e}", self.t("Impossible d'ouvrir le fichier :", "Cannot open file:"));
                self.set_status(msg, StatusKind::Error);
            }
        }
    }

    pub fn save(&mut self, save_as: bool) {
        let path = match (&self.file_path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter(self.t("Scénario anoxie (JSON)", "Anoxia scenario (JSON)"), &["json"])
                .set_file_name(self.suggested_name("json"))
                .save_file(),
        };
        let Some(path) = path else { return };
        match self.config.save_to_file(&path) {
            Ok(()) => {
                self.saved_config = self.config.clone();
                self.file_path = Some(path);
                let msg = format!("{} {}", self.t("Enregistré :", "Saved:"), self.file_name());
                self.set_status(msg, StatusKind::Success);
            }
            Err(e) => {
                let msg = format!("{} {e}", self.t("Échec de l'enregistrement :", "Save failed:"));
                self.set_status(msg, StatusKind::Error);
            }
        }
    }

    /// File name suggestion based on the reference / installation.
    pub fn suggested_name(&self, ext: &str) -> String {
        let id = &self.config.identification;
        let base = [&id.reference, &id.installation, &id.unit]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.t("scenario_anoxie", "anoxia_scenario").to_string());
        let clean: String =
            base.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        format!("{clean}.{ext}")
    }

    // ---------- UI ----------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let save_as = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
        let (f1, f5, s_as, s, o, n, e) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::F1),
                i.consume_key(Modifiers::NONE, Key::F5),
                i.consume_shortcut(&save_as),
                i.consume_shortcut(&cmd(Key::S)),
                i.consume_shortcut(&cmd(Key::O)),
                i.consume_shortcut(&cmd(Key::N)),
                i.consume_shortcut(&cmd(Key::E)),
            )
        });
        if f1 {
            self.show_guide = true;
        }
        if f5 {
            self.compute();
        }
        if s_as {
            self.save(true);
        } else if s {
            self.save(false);
        }
        if o {
            self.request(Pending::Open);
        }
        if n {
            self.request(Pending::New);
        }
        if e {
            report_tab::export_html(self);
        }
    }

    fn menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(self.t("Fichier", "File"), |ui| {
                    if ui.add(egui::Button::new(self.t("Nouveau", "New")).shortcut_text("Ctrl+N")).clicked() {
                        ui.close_menu();
                        self.request(Pending::New);
                    }
                    if ui.add(egui::Button::new(self.t("Ouvrir…", "Open…")).shortcut_text("Ctrl+O")).clicked() {
                        ui.close_menu();
                        self.request(Pending::Open);
                    }
                    if ui.add(egui::Button::new(self.t("Enregistrer", "Save")).shortcut_text("Ctrl+S")).clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui
                        .add(egui::Button::new(self.t("Enregistrer sous…", "Save as…")).shortcut_text("Ctrl+Maj+S"))
                        .clicked()
                    {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    ui.menu_button(self.t("Exemples de scénarios", "Example scenarios"), |ui| {
                        for (i, ex) in examples::all().iter().enumerate() {
                            if ui.button(ex.name(self.lang)).on_hover_text(ex.description(self.lang)).clicked() {
                                ui.close_menu();
                                self.request(Pending::Example(i));
                            }
                        }
                    });
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(self.t("Exporter le rapport HTML…", "Export HTML report…"))
                                .shortcut_text("Ctrl+E"),
                        )
                        .clicked()
                    {
                        ui.close_menu();
                        report_tab::export_html(self);
                    }
                    if ui.button(self.t("Exporter les séries (CSV)…", "Export time series (CSV)…")).clicked() {
                        ui.close_menu();
                        report_tab::export_csv(self);
                    }
                    ui.separator();
                    if ui.button(self.t("Quitter", "Quit")).clicked() {
                        ui.close_menu();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(self.t("Affichage", "View"), |ui| {
                    ui.label(RichText::new(self.t("Langue", "Language")).strong());
                    for l in [Language::French, Language::English] {
                        if ui.radio(self.lang == l, l.native_name()).clicked() {
                            self.lang = l;
                            self.report_lang = l;
                            self.persist();
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    let label = self.t("Thème sombre", "Dark theme");
                    if ui.checkbox(&mut self.dark, label).changed() {
                        theme::apply_visuals(ctx, self.dark);
                        self.persist();
                    }
                });
                ui.menu_button(self.t("Aide", "Help"), |ui| {
                    if ui.button(self.t("Prise en main (tutoriel)", "Getting started (tutorial)")).clicked() {
                        self.show_tutorial = true;
                        ui.close_menu();
                    }
                    if ui
                        .add(
                            egui::Button::new(self.t("Guide scientifique et méthode", "Scientific guide and method"))
                                .shortcut_text("F1"),
                        )
                        .clicked()
                    {
                        self.show_guide = true;
                        ui.close_menu();
                    }
                    if ui.button(self.t("Dossier de validation", "Validation dossier")).clicked() {
                        self.show_validation = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(self.t("Vérifier les mises à jour…", "Check for updates…")).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(anoxia_core::RELEASES_URL));
                        ui.close_menu();
                    }
                    if ui.button(self.t("À propos", "About")).clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let other = match self.lang {
                        Language::French => Language::English,
                        Language::English => Language::French,
                    };
                    if ui
                        .small_button(other.code().to_uppercase())
                        .on_hover_text(self.t("Switch to English", "Passer en français"))
                        .clicked()
                    {
                        self.lang = other;
                        self.report_lang = other;
                        self.persist();
                    }
                });
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let calc = egui::Button::new(RichText::new(self.t("▶ Calculer", "▶ Compute")).strong())
                    .fill(theme::ACCENT.gamma_multiply(0.15))
                    .shortcut_text("F5");
                if ui.add(calc).clicked() {
                    self.compute();
                }
                let (label, tip) = (
                    self.t("Calcul automatique", "Auto compute"),
                    self.t(
                        "Recalcule les résultats à chaque modification (quelques millisecondes).",
                        "Recompute results on every change (a few milliseconds).",
                    ),
                );
                ui.checkbox(&mut self.auto_compute, label).on_hover_text(tip);
                ui.separator();
                if let Some(err) = &self.error {
                    ui.label(RichText::new(format!("⚠ {err}")).color(theme::DANGER));
                } else if let Some((msg, kind, at)) = &self.status {
                    if at.elapsed() < Duration::from_secs(12) {
                        let color = match kind {
                            StatusKind::Success => egui::Color32::from_rgb(27, 94, 32),
                            StatusKind::Error => theme::DANGER,
                        };
                        ui.label(RichText::new(msg).color(color));
                    }
                } else if self.stale() {
                    ui.label(
                        RichText::new(self.t("Résultats à recalculer (F5)", "Results out of date (F5)"))
                            .color(theme::WARNING),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.hyperlink_to(
                        RichText::new(format!("v{}", anoxia_core::VERSION)).small().color(theme::MUTED),
                        anoxia_core::RELEASES_URL,
                    )
                    .on_hover_text(self.t("Vérifier les mises à jour (GitHub)", "Check for updates (GitHub)"));
                    ui.separator();
                    let name = format!("{}{}", self.file_name(), if self.dirty() { " •" } else { "" });
                    ui.label(RichText::new(name).small()).on_hover_text(if self.dirty() {
                        self.t("Modifications non enregistrées", "Unsaved changes")
                    } else {
                        self.t("Enregistré", "Saved")
                    });
                });
            });
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else { return };
        let mut decision: Option<bool> = None;
        egui::Window::new(self.t("Modifications non enregistrées", "Unsaved changes"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(self.t(
                    "Le scénario actuel contient des modifications non enregistrées.",
                    "The current scenario has unsaved changes.",
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(self.t("Enregistrer", "Save")).clicked() {
                        self.save(false);
                        if !self.dirty() {
                            decision = Some(true);
                        }
                    }
                    if ui.button(self.t("Ne pas enregistrer", "Don't save")).clicked() {
                        decision = Some(true);
                    }
                    if ui.button(self.t("Annuler", "Cancel")).clicked() {
                        decision = Some(false);
                    }
                });
            });
        match decision {
            Some(true) => {
                self.pending = None;
                self.saved_config = self.config.clone();
                self.execute(action.clone());
                if matches!(action, Pending::Quit) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }
}

impl eframe::App for AnoxiaApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Close guard
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }

        let title = format!(
            "Anoxia Simulator {} — {}{}",
            anoxia_core::VERSION,
            self.file_name(),
            if self.dirty() { " •" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        self.shortcuts(ctx);
        self.menu(ctx);
        self.status_bar(ctx);

        let before = self.config.clone();
        egui::SidePanel::left("inputs")
            .resizable(true)
            .default_width(400.0)
            .width_range(340.0..=560.0)
            .show(ctx, |ui| inputs::show(self, ui));
        if !self.config.same_physics(&before) {
            self.last_change = Instant::now();
            if self.auto_compute && self.last_compute_ms < 40.0 {
                self.compute();
            } else if self.auto_compute {
                self.pending_compute = true;
            } else if let Err(e) = self.config.validate() {
                self.error = Some(e.to_string());
            } else {
                self.error = None;
            }
        }

        if self.pending_compute && self.last_change.elapsed() > Duration::from_millis(250) {
            self.compute();
        }

        if self.pending_compute {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Make sure the first frames are presented even without user input.

        if self.frames < 3 {
            self.frames += 1;

            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 22.0;
                let dark = ui.visuals().dark_mode;
                for (tab, fr, en) in [
                    (Tab::Results, "Résultats", "Results"),
                    (Tab::Analyses, "Analyses", "Analyses"),
                    (Tab::Report, "Rapport", "Report"),
                ] {
                    // Text tabs with an accent underline: the active tab is
                    // read at a glance without a filled button.
                    let selected = self.tab == tab;
                    let color = if selected { theme::readable(theme::ACCENT, dark) } else { ui.visuals().text_color() };
                    let text = RichText::new(self.t(fr, en)).size(16.0).color(color);
                    let text = if selected { text.strong() } else { text };
                    let r = ui.add(egui::Button::new(text).frame(false));
                    if selected {
                        let y = r.rect.bottom() + 3.0;
                        ui.painter().hline(r.rect.x_range(), y, egui::Stroke::new(3.0_f32, theme::ACCENT));
                    }
                    if r.clicked() {
                        self.tab = tab;
                    }
                }
            });
            ui.separator();
            match self.tab {
                Tab::Results => results::show(self, ui),
                Tab::Analyses => analysis_tab::show(self, ui),
                Tab::Report => report_tab::show(self, ui),
            }
        });

        help::about_window(self, ctx);
        help::guide_window(self, ctx);
        help::validation_window(self, ctx);
        tutorial::tutorial_window(self, ctx);
        self.confirm_dialog(ctx);

        if self.analysis.busy() || self.status.as_ref().is_some_and(|s| s.2.elapsed() < Duration::from_secs(13)) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}
