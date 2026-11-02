//! "About" and "Scientific guide" windows.

use crate::app::AnoxiaApp;
use crate::theme::{hint, MUTED};
use anoxia_core::safety::{co2_effects_table, disclaimer, model_limitations, o2_effects_table, typo};
use anoxia_core::Language;
use eframe::egui::{self, RichText, Ui};

/// Built-in validation dossier, recomputed live.
pub fn validation_window(app: &mut AnoxiaApp, ctx: &egui::Context) {
    use anoxia_core::validation::{cases, count, score, Category, Nature};
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let mut open = app.show_validation;
    if !open {
        return;
    }
    let all = app.validation_cache.get_or_insert_with(cases).clone();
    let (ok, n) = score(&all);
    egui::Window::new(t("Dossier de validation", "Validation dossier"))
        .open(&mut open)
        .default_size([980.0, 680.0])
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("{ok} / {n} {}", t("cas conformes", "cases passed"))).size(18.0).strong().color(
                if ok == n { egui::Color32::from_rgb(27, 94, 32) } else { crate::theme::DANGER },
            ));
            let documented = all.iter().filter(|c| c.documented()).count();
            ui.label(
                RichText::new(format!(
                    "{} {} — {} {} — {} {} — {} {}",
                    count(&all, Nature::Validation),
                    t("validations (données indépendantes)", "validations (independent data)"),
                    count(&all, Nature::Reproduction),
                    t("reproductions de méthodes publiées", "reproductions of published methods"),
                    count(&all, Nature::Verification),
                    t("vérifications analytiques", "analytic verifications"),
                    documented,
                    t("écarts documentés", "documented deviations"),
                ))
                .small(),
            );

            hint(ui, t(
                "Chaque cas recalcule avec le moteur du logiciel une valeur publiée (NIST, fournisseurs, TNO, INRS, BCGA, Fermilab, NFPA 2001, essais). Les écarts sont expliqués : une source peut elle-même arrondir ou se tromper.",
                "Each case recomputes with the software engine a published value (NIST, suppliers, TNO, INRS, BCGA, Fermilab, NFPA 2001, experiments). Deviations are explained: a source may itself round or be wrong.",
            ));
            for cat in [Category::Properties, Category::Source, Category::Method, Category::Experiment] {
                ui.add_space(8.0);
                ui.label(RichText::new(cat.name(lang)).strong().size(16.0));
                egui::Grid::new(format!("val_{cat:?}")).striped(true).num_columns(7).spacing([12.0, 4.0]).show(ui, |ui| {
                    for h in [t("Id", "Id"), t("Nature", "Type"), t("Cas", "Case"), t("Publié", "Published"), t("Calculé", "Computed"), t("Critère", "Criterion"), ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for c in all.iter().filter(|c| c.category == cat) {
                        ui.label(c.id);
                        ui.label(RichText::new(c.nature().short(lang)).small());
                        let resp = ui.label(c.description(lang));
                        let tip = if c.note(lang).is_empty() { c.reference.to_string() } else { format!("{}\n{}", c.reference, c.note(lang)) };
                        resp.on_hover_text(tip);
                        ui.label(format!("{} {}", anoxia_core::i18n::fmt_num(c.published, 3, lang), c.unit.trim_start_matches('–')));
                        ui.label(format!("{} {}", anoxia_core::i18n::fmt_num(c.computed, 3, lang), c.unit.trim_start_matches('–')));
                        ui.label(RichText::new(c.criterion.describe(lang)).small());
                        ui.label(if c.documented() {
                            RichText::new("ⓘ").color(crate::theme::WARNING)
                        } else if c.passed() { RichText::new("✔").color(egui::Color32::from_rgb(27, 94, 32)) } else { RichText::new("✘").color(crate::theme::DANGER) });
                        ui.end_row();
                    }
                });
            }
            ui.add_space(6.0);
            hint(ui, t("Survoler un cas pour voir sa référence et l'explication des écarts. Détails : VALIDATION.md.", "Hover a case to see its reference and deviation notes. Details: VALIDATION.md."));
        });
    app.show_validation = open;
}

pub fn about_window(app: &mut AnoxiaApp, ctx: &egui::Context) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let mut open = app.show_about;
    egui::Window::new(t("À propos", "About"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.heading(format!("Anoxia Simulator {}", anoxia_core::VERSION));
            ui.label(t(
                "Simulation de l'appauvrissement en oxygène lors d'un rejet de gaz inerte dans un local.",
                "Simulation of oxygen depletion caused by an inert-gas release in a room.",
            ));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(format!("{} {}", t("Développé par", "Developed by"), anoxia_core::AUTHOR));
                ui.label("—");
                ui.hyperlink_to("LinkedIn", anoxia_core::AUTHOR_LINKEDIN_URL);
                ui.label("·");
                ui.hyperlink_to(t("code source (GitHub)", "source code (GitHub)"), anoxia_core::REPOSITORY_URL);
            });
            ui.label(t(
                "Licence GNU AGPL v3 : logiciel libre ; toute version modifiée ou mise en service doit rester libre et créditer l'auteur. Licence commerciale sur demande.",
                "GNU AGPL v3 licence: free software; any modified or hosted version must stay free and credit the author. Commercial licence on request.",
            ));
            ui.label(t(
                "Interface : Rust, egui. Moteur de calcul testé (solutions analytiques, conservation de la masse).",
                "Interface: Rust, egui. Tested computation engine (analytic solutions, mass conservation).",
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("Vérifier les mises à jour", "Check for updates")).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(anoxia_core::RELEASES_URL));
                }
                hint(ui, t("Ouvre la page des versions publiées.", "Opens the published releases page."));
            });
            ui.add_space(8.0);
            ui.label(RichText::new(t("Avertissement", "Disclaimer")).strong());
            ui.label(disclaimer(lang));
        });
    app.show_about = open;
}

fn h(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).strong().size(16.0));
}

fn p(ui: &mut Ui, text: &str, lang: Language) {
    ui.label(typo(text, lang));
}

fn eq(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).monospace());
}

pub fn guide_window(app: &mut AnoxiaApp, ctx: &egui::Context) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let mut open = app.show_guide;
    egui::Window::new(t("Guide scientifique et méthode", "Scientific guide and method"))
        .open(&mut open)
        .default_size([720.0, 640.0])
        .vscroll(true)
        .show(ctx, |ui| {
            h(ui, t("1. Démarche", "1. Approach"));
            p(ui, t(
                "Les six étapes du tutoriel (Aide → Prise en main) : 1) décrire le scénario — source (gaz, récipient, pression), type de rejet (fuite, rupture, cas enveloppe, débit imposé), local et ventilation ; 2) lire le classement et les délais d'alarme ; 3) tester la robustesse (onglet Analyses : panne de ventilation, sensibilité) ; 4) dimensionner les mesures (ventilation nécessaire, limiteur de débit) ; 5) documenter et exporter le rapport ; 6) garder en tête les limites du modèle (§ 7).",
                "The six steps of the tutorial (Help → Getting started): 1) describe the scenario — source (gas, container, pressure), release type (leak, rupture, bounding case, imposed flow), room and ventilation; 2) read the classification and alarm times; 3) test robustness (Analyses tab: ventilation failure, sensitivity); 4) size the measures (required ventilation, flow limiter); 5) document and export the report; 6) keep the model limits in mind (§ 7).",
            ), lang);

            h(ui, t("2. Débit de fuite", "2. Leak flow"));
            p(ui, t("Écoulement isentropique à travers un orifice de diamètre d et de coefficient de décharge Cd, à partir de la pression P0 et de la masse volumique ρ0 (gaz réel) dans le récipient (q_m : débit massique) :", "Isentropic flow through an orifice of diameter d and discharge coefficient Cd, from the container pressure P0 and density ρ0 (real gas) (q_m: mass flow):"), lang);
            eq(ui, "sonique (Pa/P0 ≤ r*) : q_m = Cd·A·√(γ·ρ0·P0)·(2/(γ+1))^((γ+1)/(2(γ−1)))");
            eq(ui, "subsonique         : q_m = Cd·A·√(2·ρ0·P0·γ/(γ−1)·(r^(2/γ) − r^((γ+1)/γ)))");
            p(ui, t("Cd : 0,62 pour un orifice à arête vive, 0,95–0,99 arrondi, 1,0 en rupture franche (TNO Yellow Book). Le récipient se vide de façon isotherme par défaut (prudent) ou adiabatique (indicatif) : la pression et le débit décroissent au cours du temps. Facteur de compressibilité : table NIST REFPROP (−40 à +70 °C, jusqu'à 750 bar), Peng-Robinson ou viriel hors table.", "Cd: 0.62 for a sharp-edged orifice, 0.95–0.99 rounded, 1.0 full-bore rupture (TNO Yellow Book). The container empties isothermally by default (conservative) or adiabatically (indicative): pressure and flow decrease over time. Compressibility: NIST REFPROP table (−40 to +70 °C, up to 750 bar), Peng–Robinson or virial outside the table."), lang);
            p(ui, t("CO₂ : stocké liquéfié (≈ 57 bar à 20 °C). Inventaire = taux de remplissage × capacité (B50 à 0,75 kg/L = 37,5 kg ≈ 20 m³ de gaz). Tant qu'il reste du liquide, la pression est la pression de vapeur saturante (Span & Wagner).", "CO₂: stored liquefied (≈ 57 bar at 20 °C). Inventory = filling ratio × capacity (B50 at 0.75 kg/L = 37.5 kg ≈ 20 m³ of gas). While liquid remains, the pressure is the saturation pressure (Span & Wagner)."), lang);

            h(ui, t("3. Bilan dans le local", "3. Room balance"));
            p(ui, t("Modèle bien mélangé, local à pression constante (le gaz rejeté chasse un volume égal de mélange) :", "Well-mixed model, room at constant pressure (the released gas pushes out an equal volume of mixture):"), lang);
            eq(ui, "V·dx/dt = Q_gaz − x·max(Q_gaz, η·(Q_gaz + Q_air))        O₂ = 20,95 %·(1 − x)");
            p(ui, t("Q_air = air neuf entrant : en extraction, (débit extrait − Q_gaz) combiné en quadrature avec les infiltrations (ASHRAE) ; en soufflage, débit soufflé + infiltrations. η = efficacité de ventilation, appliquée au flux sortant total (régime permanent x = Q_gaz / (η·Q_extrait), méthode Fermilab FESHM 4240). Les équations sont intégrées exactement sur chaque pas : pas d'instabilité numérique, masse conservée.", "Q_air = fresh air entering: with exhaust, (extracted flow − Q_gas) combined in quadrature with infiltration (ASHRAE); with supply, supplied flow + infiltration. η = ventilation effectiveness, applied to the whole outflow (steady state x = Q_gas / (η·Q_extract), Fermilab FESHM 4240 method). Equations are integrated exactly on each step: no numerical instability, mass conserved."), lang);
            p(ui, t("Repères utiles, sans ventilation, pour un volume de gaz Vg dans un local V : méthode par déplacement INRS/BCGA, O₂ = 20,95 % × (1 − Vg/V) ; mélange parfait (NFPA 2001), O₂ = 20,95 % × exp(−Vg/V). Exemple : 10 m³ d'azote dans 50 m³ → 16,8 % (déplacement) et 17,2 % (mélange).", "Rules of thumb without ventilation, for a gas volume Vg in a room V: INRS/BCGA displacement method, O₂ = 20.95% × (1 − Vg/V); perfect mixing (NFPA 2001), O₂ = 20.95% × exp(−Vg/V). Example: 10 m³ of nitrogen in 50 m³ → 16.8% (displacement) and 17.2% (mixing)."), lang);

            p(ui, t(
                "Chronologie d'événements : la ventilation mécanique peut s'arrêter (panne) ou démarrer (secours, extraction d'urgence asservie au détecteur) à un instant donné, et la fuite peut être isolée (vanne fermée, coupure automatique). Les coefficients du bilan sont constants entre deux événements : le pas s'arrête exactement sur chaque date, la solution reste exacte et l'état continu. Les événements simultanés sont appliqués dans un ordre fixe (arrêt, démarrage, isolement). Le scénario « panne de ventilation » des analyses reste une panne totale dès le début, sans secours.",
                "Timeline of events: the mechanical ventilation can stop (failure) or start (backup fan, emergency extraction triggered by the detector) at a given time, and the leak can be isolated (valve closed, automatic shut-off). The coefficients of the balance are constant between two events: the step lands exactly on each date, the solution stays exact and the state continuous. Simultaneous events are applied in a fixed order (stop, start, isolation). The \"ventilation failure\" scenario of the analyses remains a total failure from the start, without backup.",
            ), lang);

            h(ui, t("4. Modèle bi-zone (gaz lourds et légers)", "4. Two-zone model (heavy and light gases)"));
            p(ui, t("Le local est divisé en une zone d'accumulation (couche au sol pour Ar et CO₂, sous plafond pour He et Ne) et le reste du volume, couplés par un débit d'échange. Ses paramètres sont des hypothèses : le résultat est indicatif et le classement retient le plus défavorable des deux modèles. En pratique, les fosses et caniveaux peuvent être bien plus dangereux.", "The room is split into an accumulation zone (floor layer for Ar and CO₂, ceiling layer for He and Ne) and the rest of the volume, coupled by an exchange flow. Its parameters are assumptions: the result is indicative and the classification uses the worse of the two models. In practice, pits and trenches can be much more dangerous."), lang);

            h(ui, t("5. Champ proche (exposition à la source)", "5. Near field (exposure at the source)"));
            p(ui, t(
                "Modèle champ proche / champ lointain classique de l'hygiène industrielle (Nicas 1996 ; AIHA IH Mod ; Stefana et al. 2021, ODHMOD). Une demi-sphère de rayon r autour du point de fuite échange avec le reste du local au débit β = ½·FSA·s (FSA = 2πr², s vitesse d'air aléatoire, 0,05 à 0,3 m/s en atelier d'après Baldwin & Maynard 1998). Le gaz entre dans le champ proche, le surplus de volume passe au champ lointain, qui est ventilé et évacue vers l'extérieur :",
                "Classical near-field / far-field model of industrial hygiene (Nicas 1996; AIHA IH Mod; Stefana et al. 2021, ODHMOD). A hemisphere of radius r around the release point exchanges with the rest of the room at β = ½·FSA·s (FSA = 2πr², s random air speed, 0.05 to 0.3 m/s in workplaces after Baldwin & Maynard 1998). The gas enters the near field, the excess volume flows to the far field, which is ventilated and vents outside:",
            ), lang);
            eq(ui, "V_nf·dx_nf/dt = Q_gaz·(1 − x_nf) + β·(x_ff − x_nf)");
            eq(ui, "V_ff·dx_ff/dt = (Q_gaz + β)·x_nf − β·x_ff − Q_sortie·x_ff");
            p(ui, t(
                "La somme des deux équations donne le bilan du local avec la concentration du champ lointain à l'évacuation ; elle coïncide avec le bilan bien mélangé quand l'air évacué est représentatif de la moyenne (écart O4 du dossier de validation). Le modèle donne une moyenne de zone pour un rejet à faible vitesse : pour un jet sous pression, l'entraînement d'air n'est pas résolu, la moyenne réelle est sans doute plus faible mais la concentration dans l'axe du jet peut être bien plus élevée ; ce n'est pas une borne en tout point. Par défaut il est affiché à part et signalé s'il est plus sévère ; le classement suit les modèles de local, qui correspondent à ce que mesurent les détecteurs. Sur l'exemple publié d'ODHMOD (10 m³, azote à 0,1 m³/s pendant 120 s, demi-sphère de 1 m), le logiciel retrouve 12 % d'O₂ à 26 s près de la fuite contre 56 s pour la moyenne du local.",
                "Summing both equations gives the room balance with the far-field concentration at the outlet; it coincides with the well-mixed balance when the expelled air is representative of the average (deviation O4 of the validation dossier). The model gives a zone average for a low-momentum release: for a pressurised jet, air entrainment is not resolved, the real average is probably lower but the concentration in the jet axis can be much higher; it is not a bound at every point. By default it is shown separately and flagged when more severe; the classification follows the room models, which correspond to what detectors measure. On the published ODHMOD example (10 m³, nitrogen at 0.1 m³/s for 120 s, 1 m hemisphere), the software finds 12% O₂ after 26 s next to the leak against 56 s for the room average.",
            ), lang);

            h(ui, t("6. Seuils et effets", "6. Thresholds and effects"));
            egui::Grid::new("guide_o2").striped(true).num_columns(2).show(ui, |ui| {
                ui.label(RichText::new("O₂ (%)").strong());
                ui.label(RichText::new(t("Effets / statut", "Effects / status")).strong());
                ui.end_row();
                for (k, v) in o2_effects_table(lang) {
                    ui.label(if lang == Language::English { k.replace(',', ".") } else { k.to_string() });
                    ui.label(typo(v, lang));
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            egui::Grid::new("guide_co2").striped(true).num_columns(2).show(ui, |ui| {
                ui.label(RichText::new("CO₂ (%)").strong());
                ui.label(RichText::new(t("Effets / valeur limite", "Effects / limit value")).strong());
                ui.end_row();
                for (k, v) in co2_effects_table(lang) {
                    ui.label(if lang == Language::English { k.replace(',', ".") } else { k.to_string() });
                    ui.label(typo(v, lang));
                    ui.end_row();
                }
            });
            p(ui, t("L'anoxie ne provoque aucune sensation d'étouffement : la respiration est commandée par le CO₂, pas par l'O₂. Une à deux inspirations de gaz inerte pur peuvent suffire à perdre connaissance.", "Anoxia causes no feeling of suffocation: breathing is driven by CO₂, not O₂. One or two breaths of pure inert gas can cause collapse."), lang);

            p(ui, t(
                "Pression partielle : ce que reçoit l'organisme est la pression partielle d'oxygène pO₂ = fraction d'O₂ × pression atmosphérique (21,2 kPa dans l'air au niveau de la mer). Les seuils d'alarme restent exprimés en %, comme les détecteurs, mais le logiciel affiche aussi la pO₂ minimale et ambiante ; en altitude (pression < 900 hPa) un même pourcentage est plus hypoxiant et un avertissement le rappelle.",
                "Partial pressure: what the body receives is the oxygen partial pressure pO₂ = O₂ fraction × atmospheric pressure (21.2 kPa in air at sea level). The alarm levels stay in %, like the detectors, but the software also shows the minimum and ambient pO₂; at altitude (pressure < 900 hPa) the same percentage is more hypoxic and a warning recalls it.",
            ), lang);

            h(ui, t("7. Hypothèses et limites", "7. Assumptions and limitations"));
            for l in model_limitations(lang) {
                ui.label(format!("• {l}"));
            }

            h(ui, t("8. Références", "8. References"));
            for r in [
                "INRS ED 967 — Les espaces confinés ; INRS ED 6088 — Détecteurs portables de gaz",
                "EIGA Doc. 44 — Hazards of inert gases and oxygen depletion",
                "OSHA 29 CFR 1910.146 — Permit-required confined spaces",
                "NIOSH Pocket Guide to Chemical Hazards (CO₂) ; NIOSH 86-110 / 87-113",
                "Arrêté du 30 juin 2004 modifié (VLEP indicative CO₂ 5 000 ppm) ; directive 2006/15/CE — comparaison au pic, pas à une moyenne 8 h",
                "EN 50104 — Détecteurs d'oxygène",
                "TNO Yellow Book (CPR 14E) — Methods for the calculation of physical effects",
                "Span R., Wagner W. (1996), J. Phys. Chem. Ref. Data 25, 1509 — CO₂",
                "Peng D.-Y., Robinson D. B. (1976), Ind. Eng. Chem. Fundam. 15, 59",
                "Nicas M. (1996), Am. Ind. Hyg. Assoc. J. 57, 542 — modèle champ proche / champ lointain ; AIHA IH Mod",
                "Stefana E., Marciano F., Drolet D., Armstrong T. W. (2021), Process Saf. Environ. Prot. 149, 537 — ODHMOD",
                "Baldwin P. E. J., Maynard A. D. (1998), Ann. Occup. Hyg. 42, 303 — vitesses d'air en milieu de travail",
            ] {
                ui.label(format!("• {r}"));
            }
            ui.add_space(8.0);
            hint(ui, &disclaimer(lang));
            ui.label(RichText::new("").color(MUTED));
        });
    app.show_guide = open;
}
