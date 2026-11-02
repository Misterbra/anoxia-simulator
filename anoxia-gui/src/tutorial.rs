//! "Getting started" tutorial: a short guided tour whose buttons perform the
//! actions in the application (load an example, open a tab, open a window).

use crate::analysis_tab::Study;
use crate::app::{AnoxiaApp, Pending, Tab};
use crate::examples;
use crate::theme::{self, MUTED};
use anoxia_core::Language;
use eframe::egui::{self, Color32, RichText, Ui};

/// Action requested from the tutorial, executed once the window is drawn.
#[derive(Clone, Copy)]
enum Action {
    LoadExample(usize),
    GoTo(Tab),
    GoToStudy(Study),
    OpenIdentification,
    OpenGuide,
    OpenValidation,
}

struct Step {
    title: (&'static str, &'static str),
    body: (&'static str, &'static str),
    actions: &'static [(&'static str, &'static str, Action)],
}

const STEPS: &[Step] = &[
    Step {
        title: ("Décrire le scénario", "Describe the scenario"),
        body: (
            "Le panneau de gauche rassemble toutes les données d'entrée : le gaz et son récipient, le type de rejet (fuite, robinet ouvert, rejet instantané, débit imposé), puis le local et sa ventilation. Chaque valeur se modifie en la faisant glisser ou en double-cliquant pour la saisir. Les résultats se recalculent aussitôt.",
            "The left panel gathers every input: the gas and its container, the type of release (leak, open valve, instantaneous release, imposed flow), then the room and its ventilation. Drag a value to change it, or double-click to type it. Results are recomputed immediately.",
        ),
        // The example buttons are generated from the list of examples (see `step_card`).
        actions: &[],
    },
    Step {
        title: ("Lire le résultat", "Read the result"),
        body: (
            "Dans l'onglet Résultats, le bandeau coloré donne le classement du scénario et les tuiles répondent aux questions clés : jusqu'où descend l'oxygène, et en combien de temps les seuils d'alarme sont atteints. Viennent ensuite les avertissements, la courbe d'oxygène, le tableau des seuils, les recommandations et les contrôles indépendants (méthode INRS/BCGA, calcul manuel).",
            "In the Results tab, the coloured banner gives the classification and the tiles answer the key questions: how low oxygen falls, and how soon the alarm levels are reached. Then come the warnings, the oxygen curve, the threshold table, the recommendations and the independent checks (INRS/BCGA method, hand calculation).",
        ),
        actions: &[("Aller aux résultats", "Go to the results", Action::GoTo(Tab::Results))],
    },
    Step {
        title: ("Tester la robustesse", "Test the robustness"),
        body: (
            "L'onglet Analyses compare des variantes du scénario : panne de ventilation, plusieurs diamètres de fuite, balayage d'un paramètre. La sensibilité (tornado) montre quelles données pèsent le plus sur le résultat, donc lesquelles fiabiliser en priorité.",
            "The Analyses tab compares variants of the scenario: ventilation failure, several leak diameters, parameter sweeps. The sensitivity (tornado) shows which inputs weigh most on the result, hence which ones to firm up first.",
        ),
        actions: &[
            ("Voir la panne de ventilation", "See the ventilation failure", Action::GoToStudy(Study::Failure)),
            ("Voir la sensibilité", "See the sensitivity", Action::GoToStudy(Study::Tornado)),
        ],
    },
    Step {
        title: ("Dimensionner les mesures", "Size the measures"),
        body: (
            "Analyses → Dimensionnement calcule le débit d'extraction nécessaire pour rester au-dessus du seuil choisi, et le diamètre de fuite maximal tolérable (utile pour un limiteur de débit). Le bouton « Appliquer » reporte le débit trouvé dans le scénario.",
            "Analyses → Sizing computes the extraction flow needed to stay above the chosen level, and the maximum tolerable leak diameter (useful for a flow limiter). The \"Apply\" button copies the flow into the scenario.",
        ),
        actions: &[("Ouvrir le dimensionnement", "Open the sizing", Action::GoToStudy(Study::Sizing))],
    },
    Step {
        title: ("Documenter et exporter", "Document and export"),
        body: (
            "Renseignez l'identification (site, local, rédacteur) puis exportez le rapport HTML depuis l'onglet Rapport : il contient les données d'entrée, les courbes, les seuils, les recommandations, les hypothèses et un bloc de validation. Il s'ouvre dans un navigateur ou Word, et s'imprime en PDF. Les courbes s'exportent aussi en CSV, et le scénario s'enregistre en JSON (Ctrl+S).",
            "Fill in the identification (site, room, author) then export the HTML report from the Report tab: it contains the inputs, curves, thresholds, recommendations, assumptions and an approval block. It opens in a browser or Word and prints to PDF. Curves can also be exported to CSV, and the scenario saved as JSON (Ctrl+S).",
        ),
        actions: &[
            ("Renseigner l'identification", "Fill in the identification", Action::OpenIdentification),
            ("Aller au rapport", "Go to the report", Action::GoTo(Tab::Report)),
        ],
    },
    Step {
        title: ("Connaître les limites", "Know the limits"),
        body: (
            "Le logiciel est un outil d'aide à l'évaluation : modèle bien mélangé, modèle bi-zone indicatif, pas de champ proche ni de flaques cryogéniques. Il ne remplace ni l'évaluation par une personne compétente ni la mesure par un détecteur. Le guide scientifique explique les équations et le dossier de validation compare les résultats à des références indépendantes.",
            "The software is an assessment aid: well-mixed model, indicative two-zone model, no near field nor cryogenic pools. It replaces neither the assessment by a competent person nor measurement with a detector. The scientific guide explains the equations and the validation dossier compares results with independent references.",
        ),
        actions: &[
            ("Guide scientifique (F1)", "Scientific guide (F1)", Action::OpenGuide),
            ("Dossier de validation", "Validation dossier", Action::OpenValidation),
        ],
    },
];

const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("F5", "Calculer", "Compute"),
    ("F1", "Guide scientifique", "Scientific guide"),
    ("Ctrl+N / Ctrl+O", "Nouveau / Ouvrir un scénario", "New / Open a scenario"),
    ("Ctrl+S / Ctrl+Maj+S", "Enregistrer / Enregistrer sous", "Save / Save as"),
    ("Ctrl+E", "Exporter le rapport HTML", "Export the HTML report"),
    ("Ctrl + molette", "Zoomer un graphique (double-clic : vue initiale)", "Zoom a chart (double-click: reset)"),
];

fn step_card(ui: &mut Ui, index: usize, step: &Step, lang: Language) -> Option<Action> {
    let mut requested = None;
    egui::Frame::group(ui.style()).rounding(6.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            egui::Frame::none()
                .fill(theme::ACCENT)
                .rounding(12.0)
                .inner_margin(egui::Margin::symmetric(8.0, 2.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("{}", index + 1)).strong().color(Color32::WHITE));
                });
            ui.label(RichText::new(lang.pick(step.title.0, step.title.1)).strong().size(15.0));
        });
        ui.label(anoxia_core::safety::typo(lang.pick(step.body.0, step.body.1), lang));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (fr, en, action) in step.actions {
                if ui.button(format!("▶ {}", lang.pick(fr, en))).clicked() {
                    requested = Some(*action);
                }
            }
            if index == 0 {
                ui.label(RichText::new(lang.pick("Charger un exemple :", "Load an example:")).color(MUTED));
                for (i, ex) in examples::all().iter().enumerate() {
                    if ui.button(format!("▶ {}", ex.name(lang))).on_hover_text(ex.description(lang)).clicked() {
                        requested = Some(Action::LoadExample(i));
                    }
                }
            }
        });
    });
    ui.add_space(4.0);
    requested
}

pub fn tutorial_window(app: &mut AnoxiaApp, ctx: &egui::Context) {
    // Remembered across frames so that reopening the window starts at the top.
    let was_open_id = egui::Id::new("tutorial_was_open");
    if !app.show_tutorial {
        ctx.data_mut(|d| d.insert_temp(was_open_id, false));
        return;
    }
    let was_open = ctx.data(|d| d.get_temp::<bool>(was_open_id)).unwrap_or(false);
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let mut open = true;
    let mut requested: Option<Action> = None;
    let mut at_start = app.settings.show_tutorial_at_start;
    let mut settings_changed = false;
    // On the right-hand side: the input panel (step 1) stays visible and the
    // left part of the results area too. The window can be moved, resized or
    // collapsed from its title bar.
    let screen = ctx.screen_rect();
    let width = 540.0_f32.min(screen.width() * 0.45).max(360.0);
    let height = (screen.height() - 180.0).clamp(360.0, 600.0);
    egui::Window::new(t("Prise en main", "Getting started"))
        .open(&mut open)
        .default_size([width, height])
        .default_pos(egui::pos2(screen.right() - width - 28.0, 100.0))
        .show(ctx, |ui| {
            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if !was_open {
                area = area.vertical_scroll_offset(0.0);
            }
            area.show(ui, |ui| {
                ui.label(RichText::new(t(
                    "Bienvenue. Ce simulateur estime la baisse d'oxygène dans un local après un rejet de gaz inerte (azote, argon, CO₂, hélium, néon) et vous aide à choisir les mesures de prévention. Six étapes suffisent pour une première étude. Un exemple (laboratoire, fuite d'azote) est déjà chargé : les résultats affichés sont les siens.",
                    "Welcome. This simulator estimates the oxygen drop in a room after an inert-gas release (nitrogen, argon, CO₂, helium, neon) and helps you choose prevention measures. Six steps are enough for a first study. An example (laboratory, nitrogen leak) is already loaded: the results shown are its own.",
                )).size(14.0));
                ui.add_space(8.0);
                for (i, step) in STEPS.iter().enumerate() {
                    if let Some(a) = step_card(ui, i, step, lang) {
                        requested = Some(a);
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t("Raccourcis", "Shortcuts")).strong());
                egui::Grid::new("tutorial_shortcuts").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                    for (key, fr, en) in SHORTCUTS {
                        ui.label(RichText::new(*key).monospace());
                        ui.label(lang.pick(fr, en));
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut at_start, t("Afficher au démarrage", "Show at start-up")).changed() {
                        settings_changed = true;
                    }
                    ui.label(RichText::new(t("(Aide → Prise en main pour le retrouver)", "(Help → Getting started to open it again)")).small().color(MUTED));
                });
            });
        });
    ctx.data_mut(|d| d.insert_temp(was_open_id, open));
    if settings_changed {
        app.settings.show_tutorial_at_start = at_start;
        app.persist();
    }
    app.show_tutorial = open;
    if let Some(a) = requested {
        match a {
            Action::LoadExample(i) => app.request(Pending::Example(i)),
            Action::GoTo(tab) => app.tab = tab,
            Action::GoToStudy(study) => {
                app.tab = Tab::Analyses;
                app.analysis.study = study;
            }
            Action::OpenIdentification => app.open_identification = true,
            Action::OpenGuide => app.show_guide = true,
            Action::OpenValidation => app.show_validation = true,
        }
    }
}
