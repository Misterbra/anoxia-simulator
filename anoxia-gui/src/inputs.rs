//! Left panel: scenario inputs.

use crate::app::AnoxiaApp;
use crate::theme::{self, hint};
use anoxia_core::config::{
    Blowdown, BottleType, Co2Withdrawal, Event, EventAction, ExtractionLocation, ReleaseKind, StratificationMode,
    ThresholdPreset, VentilationMode,
};
use anoxia_core::flow::Source;
use anoxia_core::i18n::{fmt_num, fmt_pct, fmt_unit};
use anoxia_core::{GasType, Language};
use eframe::egui::{self, Color32, DragValue, RichText, Ui};

/// A labelled numeric field with its unit, inside a 3-column grid.
#[allow(clippy::too_many_arguments)]
fn num(
    ui: &mut Ui,
    lang: Language,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    unit: &str,
    decimals: usize,
    tip: &str,
) -> bool {
    let r = ui.label(label);
    if !tip.is_empty() {
        r.on_hover_text(tip);
    }
    let speed = ((*range.end() - *range.start()) / 2000.0).max(10f64.powi(-(decimals as i32)));
    let changed = ui
        .add(
            DragValue::new(value)
                .range(range)
                .speed(speed)
                .max_decimals(decimals)
                .min_decimals(decimals.min(1))
                .custom_formatter(move |v, _| fmt_num(v, decimals, lang).replace('\u{202f}', ""))
                .custom_parser(|s| s.replace(',', ".").replace(['\u{202f}', ' ', '\u{a0}'], "").parse().ok())
                .suffix(if unit.is_empty() { String::new() } else { format!(" {unit}") }),
        )
        .changed();
    ui.end_row();
    changed
}

fn text_row(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
    ui.end_row();
}

fn header(ui: &mut Ui, id: &str, title: &str, open: bool, add: impl FnOnce(&mut Ui)) {
    header_forced(ui, id, title, open, None, add);
}

/// Each input section sits in its own light card, so that the long left
/// panel reads as a sequence of blocks rather than a single column of fields.
fn header_forced(ui: &mut Ui, id: &str, title: &str, open: bool, force: Option<bool>, add: impl FnOnce(&mut Ui)) {
    let dark = ui.visuals().dark_mode;
    let (fill, stroke) = if dark {
        (Color32::from_rgb(34, 38, 42), Color32::from_rgb(62, 68, 74))
    } else {
        (Color32::from_rgb(247, 249, 251), Color32::from_rgb(220, 227, 233))
    };
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, stroke))
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(RichText::new(title).strong().size(15.0))
                .id_source(id)
                .default_open(open)
                .open(force)
                .show(ui, add);
        });
    ui.add_space(6.0);
}

pub fn show(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(4.0);
        ui.heading(t("Scénario", "Scenario"));
        hint(
            ui,
            t(
                "Toutes les valeurs sont modifiables par glisser ou saisie directe (double-clic).",
                "All values can be dragged or typed (double-click).",
            ),
        );
        ui.add_space(4.0);

        identification(app, ui);
        source(app, ui);
        release(app, ui);
        room(app, ui);
        thresholds(app, ui);
        advanced(app, ui);
        ui.add_space(12.0);
    });
}

fn identification(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let force = std::mem::take(&mut app.open_identification).then_some(true);
    header_forced(ui, "ident", t("Identification", "Identification"), false, force, |ui| {
        let id = &mut app.config.identification;
        egui::Grid::new("ident_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            text_row(ui, t("Référence", "Reference"), &mut id.reference);
            text_row(ui, t("Site / service", "Site / department"), &mut id.unit);
            text_row(ui, t("Bâtiment", "Building"), &mut id.building);
            text_row(ui, t("Local / installation", "Room / installation"), &mut id.installation);
            text_row(ui, t("Rédacteur", "Author"), &mut id.user);
            text_row(ui, t("Vérificateur", "Reviewer"), &mut id.reviewer);
        });
        ui.label(t("Commentaires", "Comments"));
        ui.add(egui::TextEdit::multiline(&mut id.comments).desired_rows(3).desired_width(f32::INFINITY));
    });
}

fn source(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    header(ui, "source", t("Gaz et récipient", "Gas and container"), true, |ui| {
        let c = &mut app.config;
        if c.release.kind == ReleaseKind::ConstantFlow {
            hint(
                ui,
                t(
                    "Débit imposé : seuls le gaz et sa température comptent ; le récipient n'est pas utilisé.",
                    "Imposed flow: only the gas matters; the container is not used.",
                ),
            );
        }
        egui::Grid::new("gas_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t("Gaz", "Gas"));
            let before = c.gas;
            egui::ComboBox::from_id_source("gas").selected_text(c.gas.name(lang)).width(200.0).show_ui(ui, |ui| {
                for g in GasType::all() {
                    ui.selectable_value(&mut c.gas, g, g.name(lang));
                }
            });
            ui.label("");
            ui.end_row();
            if c.gas != before && c.gas.is_liquefied() != before.is_liquefied() && !c.gas.is_liquefied() {
                c.source.pressure_bar_g = c.gas.typical_pressure_bar();
            }

            if c.release.kind != ReleaseKind::ConstantFlow {
                ui.label(t("Récipient", "Container"));
                egui::ComboBox::from_id_source("bottle")
                    .selected_text(c.source.bottle_type.name(lang))
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for b in BottleType::all() {
                            if ui.selectable_value(&mut c.source.bottle_type, b, b.name(lang)).clicked() {
                                if let Some(v) = b.volume_liters() {
                                    c.source.volume_liters = v;
                                }
                            }
                        }
                    });
                ui.label("");
                ui.end_row();

                if num(
                    ui,
                    lang,
                    t("Capacité en eau", "Water capacity"),
                    &mut c.source.volume_liters,
                    0.1..=100_000.0,
                    "L",
                    1,
                    t(
                        "Volume intérieur d'un récipient (marqué sur l'ogive).",
                        "Internal volume of one container (stamped on the shoulder).",
                    ),
                ) {
                    c.source.bottle_type = BottleType::from_volume(c.source.volume_liters);
                }
                ui.label(t("Nombre de récipients", "Number of containers")).on_hover_text(t(
                    "Cadre ou batterie de bouteilles raccordées sur un même collecteur.",
                    "Bundle or battery of cylinders on a common manifold.",
                ));
                ui.add(DragValue::new(&mut c.source.count).range(1..=100).speed(0.1));
                ui.label("");
                ui.end_row();

                if c.gas.is_liquefied() {
                    num(
                        ui,
                        lang,
                        t("Taux de remplissage", "Filling ratio"),
                        &mut c.source.co2_fill_ratio,
                        0.05..=0.75,
                        "kg/L",
                        3,
                        t(
                            "Masse de CO₂ par litre de capacité (max. réglementaire usuel 0,75 kg/L). B50 = 37,5 kg.",
                            "CO₂ mass per litre of capacity (usual regulatory max 0.75 kg/L). B50 = 37.5 kg.",
                        ),
                    );
                } else {
                    num(
                        ui,
                        lang,
                        t("Pression (manomètre)", "Pressure (gauge)"),
                        &mut c.source.pressure_bar_g,
                        0.1..=700.0,
                        t("bar rel.", "bar g"),
                        0,
                        t(
                            "Pression lue sur le manomètre (relative). Bouteilles usuelles : 200 ou 300 bar.",
                            "Pressure read on the gauge (relative). Usual cylinders: 200 or 300 bar.",
                        ),
                    );
                }
                num(
                    ui,
                    lang,
                    t("Température du récipient", "Container temperature"),
                    &mut c.source.temperature_c,
                    -30.0..=if c.gas.is_liquefied() { 30.0 } else { 60.0 },
                    "°C",
                    0,
                    "",
                );
            }
        });

        if c.gas.is_liquefied() && c.release.kind != ReleaseKind::ConstantFlow {
            ui.horizontal(|ui| {
                ui.label(t("Phase soutirée :", "Withdrawal:"));
                ui.radio_value(&mut c.source.co2_withdrawal, Co2Withdrawal::Vapour, t("gazeuse", "vapour"))
                    .on_hover_text(t("Bouteille debout sans tube plongeur.", "Upright cylinder without dip tube."));
                ui.radio_value(&mut c.source.co2_withdrawal, Co2Withdrawal::Liquid, t("liquide", "liquid"))
                    .on_hover_text(t(
                        "Tube plongeur ou bouteille couchée/inversée : débit bien plus élevé.",
                        "Dip tube or lying/inverted cylinder: much higher flow.",
                    ));
            });
        }

        // Live derived information
        let rel = c.gas.relative_density();
        hint(
            ui,
            &format!(
                "{} {} — {}",
                t("Densité relative à l'air :", "Density relative to air:"),
                fmt_num(rel, 2, lang),
                c.gas.buoyancy().description(lang)
            ),
        );
        if let Some(src) = Source::from_config(c).ok().filter(|_| c.release.kind != ReleaseKind::ConstantFlow) {
            let vol = src.volume_at(src.releasable_mass(), c.room.temperature_c + 273.15);
            let msg = match lang {
                Language::French => format!(
                    "Inventaire : {} soit {} de gaz à libérer ({} du volume du local).",
                    fmt_unit(src.initial_mass, 2, "kg", lang),
                    fmt_unit(vol, 2, "m³", lang),
                    fmt_pct(100.0 * vol / c.room.volume_m3, 0, lang)
                ),
                Language::English => format!(
                    "Inventory: {} i.e. {} of releasable gas ({} of the room volume).",
                    fmt_unit(src.initial_mass, 2, "kg", lang),
                    fmt_unit(vol, 2, "m³", lang),
                    fmt_pct(100.0 * vol / c.room.volume_m3, 0, lang)
                ),
            };
            ui.label(RichText::new(msg).small());
        }
        if c.gas.is_toxic() {
            theme::callout(
                ui,
                theme::DANGER,
                t(
                    "Le CO₂ est toxique par lui-même (VLEP 0,5 %, DIVS 4 %) : il est évalué en plus de l'oxygène.",
                    "CO₂ is toxic in its own right (OEL 0.5%, IDLH 4%): it is assessed in addition to oxygen.",
                ),
            );
        }
    });
}

fn release(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    header(ui, "release", t("Scénario de rejet", "Release scenario"), true, |ui| {
        let (room_t, room_p) = (app.config.room.temperature_c, app.config.room.pressure_hpa);
        let r = &mut app.config.release;
        for k in ReleaseKind::all() {
            if ui.radio(r.kind == k, k.name(lang)).clicked() && r.kind != k {
                r.kind = k;
                let (d, cd) = k.defaults();
                r.diameter_mm = d;
                r.discharge_coefficient = cd;
            }
        }
        if r.kind == ReleaseKind::Instantaneous {
            hint(
                ui,
                t(
                    "Tout l'inventaire est libéré d'un coup (méthode par déplacement INRS ED 155 / BCGA CP 52) : borne supérieure du risque.",
                    "The whole inventory is released at once (INRS ED 155 / BCGA CP 52 displacement method): upper bound of the risk.",
                ),
            );
            return;
        }
        if r.kind == ReleaseKind::ConstantFlow {
            egui::Grid::new("const_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                num(
                    ui,
                    lang,
                    t("Débit de gaz", "Gas flow"),
                    &mut r.constant_flow_m3h,
                    0.0001..=1_000_000.0,
                    "m³/h",
                    3,
                    t(
                        "Débit de gaz détendu aux conditions du local. 1 L/min = 0,06 m³/h.",
                        "Gas flow expanded to room conditions. 1 L/min = 0.06 m³/h.",
                    ),
                );
                num(
                    ui,
                    lang,
                    t("Durée du rejet", "Release duration"),
                    &mut r.constant_duration_min,
                    0.01..=10_080.0,
                    "min",
                    1,
                    "",
                );
            });
            hint(
                ui,
                &format!(
                    "{} {}",
                    t("Quantité totale rejetée :", "Total quantity released:"),
                    fmt_unit(r.constant_flow_m3h * r.constant_duration_min / 60.0, 2, "m³", lang)
                ),
            );
            ui.checkbox(&mut r.cold_gas, t("Gaz froid issu d'un liquide cryogénique", "Cold gas from a cryogenic liquid"))
                .on_hover_text(t(
                    "Le gaz froid est plus dense que l'air (même l'azote ou l'hélium) : il s'accumule d'abord au sol. Active le modèle bi-zone avec une couche au sol.",
                    "Cold gas is denser than air (even nitrogen or helium): it first accumulates at floor level. Enables the two-zone model with a floor layer.",
                ));
            ui.collapsing(t("Convertir un débit en L/min ou un liquide cryogénique", "Convert from L/min or a cryogenic liquid"), |ui| {
                let id = ui.id().with("cryo");
                let (mut lpm, mut liq_l, mut evap_min, mut factor) =
                    ui.data(|d| d.get_temp::<(f64, f64, f64, f64)>(id)).unwrap_or((15.0, 10.0, 10.0, 692.0));
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut lpm).range(0.0..=100_000.0).speed(0.5).suffix(" L/min"));
                    if ui.button(t("Appliquer", "Apply")).clicked() {
                        r.constant_flow_m3h = lpm * 0.06;
                    }
                });
                ui.separator();
                ui.label(t("Liquide cryogénique vaporisé :", "Vaporised cryogenic liquid:"));
                ui.horizontal_wrapped(|ui| {
                    for (name, f) in [("LN₂", 692.0), ("LAr", 840.0), ("LHe", 752.0)] {
                        ui.selectable_value(&mut factor, f, format!("{name} ({f:.0}×)"));
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut liq_l).range(0.01..=1_000_000.0).speed(0.5).suffix(" L"));
                    ui.label(t("en", "over"));
                    ui.add(DragValue::new(&mut evap_min).range(0.01..=10_080.0).speed(0.5).suffix(" min"));
                    if ui.button(t("Appliquer", "Apply")).clicked() {
                        // Expansion ratios are given at 20 °C, 1 atm: correct to the room conditions.
                        let k = (room_t + 273.15) / 293.15 * 1013.25 / room_p;
                        r.constant_flow_m3h = liq_l * factor * k / 1000.0 * 60.0 / evap_min;
                        r.constant_duration_min = evap_min;
                        r.cold_gas = true;
                    }
                });
                hint(ui, t(
                    "Taux d'expansion à 20 °C, 1 atm (NIST : liquide au point d'ébullition → gaz). Choisir le gaz correspondant dans « Gaz et récipient ».",
                    "Expansion ratios at 20 °C, 1 atm (NIST: liquid at normal boiling point → gas). Select the matching gas in \"Gas and container\".",
                ));
                ui.data_mut(|d| d.insert_temp(id, (lpm, liq_l, evap_min, factor)));
            });
            return;
        }
        egui::Grid::new("release_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            num(
                ui,
                lang,
                t("Diamètre équivalent", "Equivalent diameter"),
                &mut r.diameter_mm,
                0.05..=100.0,
                "mm",
                2,
                t(
                    "Fuite sur raccord : 0,5–3 mm. Robinet grand ouvert : siège ≈ 4 mm (Rotarex). Robinet arraché : col de bouteille ≈ 10–15 mm. Collecteur : diamètre intérieur.",
                    "Fitting leak: 0.5–3 mm. Valve fully open: seat ≈ 4 mm (Rotarex). Valve sheared off: cylinder neck ≈ 10–15 mm. Manifold: inner diameter.",
                ),
            );
        });
        ui.horizontal_wrapped(|ui| {
            for d in [0.5, 1.0, 2.0, 5.0, 10.0] {
                if ui.small_button(format!("{} mm", fmt_num(d, if d < 1.0 { 1 } else { 0 }, lang))).clicked() {
                    r.diameter_mm = d;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(t("Coefficient de décharge Cd", "Discharge coefficient Cd")).on_hover_text(t(
                "TNO Yellow Book : 0,62 orifice à arête vive, 0,95–0,99 orifice arrondi, 1,0 rupture franche (majorant).",
                "TNO Yellow Book: 0.62 sharp-edged orifice, 0.95–0.99 rounded orifice, 1.0 full-bore rupture (upper bound).",
            ));
            ui.add(
                DragValue::new(&mut r.discharge_coefficient)
                    .range(0.3..=1.0)
                    .speed(0.005)
                    .custom_formatter(move |v, _| fmt_num(v, 2, lang))
                    .custom_parser(|s| s.replace(',', ".").trim().parse().ok()),
            );
        });
        ui.horizontal(|ui| {
            for (v, label) in [
                (0.62, t("arête vive", "sharp")),
                (0.95, t("arrondi", "rounded")),
                (1.0, t("majorant", "conservative")),
            ] {
                if ui
                    .selectable_label(
                        (r.discharge_coefficient - v).abs() < 1e-6,
                        format!("{} ({label})", fmt_num(v, 2, lang)),
                    )
                    .clicked()
                {
                    r.discharge_coefficient = v;
                }
            }
        });
        if let Some(run) = &app.run {
            let s = &run.result.summary;
            if s.initial_mass_flow.is_finite() {
                hint(
                    ui,
                    &format!(
                        "{} {} ({})",
                        t("Débit initial :", "Initial flow:"),
                        fmt_unit(s.initial_volume_flow_m3h, 1, "m³/h", lang),
                        fmt_unit(s.initial_mass_flow * 3600.0, 1, "kg/h", lang)
                    ),
                );
            }
        }
    });
}

fn room(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    header(ui, "room", t("Local et ventilation", "Room and ventilation"), true, |ui| {
        let dims = &mut app.dims;
        let room = &mut app.config.room;
        egui::Grid::new("room_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            num(
                ui,
                lang,
                t("Volume libre", "Free volume"),
                &mut room.volume_m3,
                0.5..=1_000_000.0,
                "m³",
                1,
                t(
                    "Volume d'air du local, déduction faite des équipements volumineux.",
                    "Air volume of the room, minus bulky equipment.",
                ),
            );
            num(ui, lang, t("Hauteur sous plafond", "Ceiling height"), &mut room.height_m, 0.5..=100.0, "m", 2, "");
        });
        ui.collapsing(t("Calculer le volume à partir des dimensions", "Compute volume from dimensions"), |ui| {
            ui.horizontal(|ui| {
                ui.label("L");
                ui.add(DragValue::new(&mut dims[0]).range(0.1..=1000.0).speed(0.05).suffix(" m"));
                ui.label("×");
                ui.add(DragValue::new(&mut dims[1]).range(0.1..=1000.0).speed(0.05).suffix(" m"));
                ui.label("× H");
                ui.add(DragValue::new(&mut dims[2]).range(0.5..=100.0).speed(0.05).suffix(" m"));
            });
            let v = dims[0] * dims[1] * dims[2];
            if ui.button(format!("{} {}", t("Appliquer :", "Apply:"), fmt_unit(v, 1, "m³", lang))).clicked() {
                room.volume_m3 = v;
                room.height_m = dims[2];
            }
        });

        ui.add_space(4.0);
        ui.checkbox(
            &mut room.mechanical_enabled,
            t("Ventilation mécanique en service", "Mechanical ventilation running"),
        )
        .on_hover_text(t(
            "Décocher pour simuler une panne de ventilation.",
            "Uncheck to simulate a ventilation failure.",
        ));
        if room.mechanical_enabled {
            ui.horizontal(|ui| {
                for mode in [VentilationMode::Extraction, VentilationMode::Supply] {
                    ui.radio_value(&mut room.mechanical_mode, mode, mode.name(lang)).on_hover_text(match mode {
                        VentilationMode::Extraction => t(
                            "Ventilateur d'extraction : le gaz rejeté est aspiré avec l'air, seul (débit − fuite) d'air neuf entre. Hypothèse prudente (Fermilab FESHM 4240).",
                            "Exhaust fan: the released gas is extracted with the air, only (flow − leak) of fresh air enters. Conservative assumption (Fermilab FESHM 4240).",
                        ),
                        VentilationMode::Supply => t(
                            "Soufflage ou double flux : le débit du ventilateur est entièrement de l'air neuf.",
                            "Supply or balanced system: the fan flow is entirely fresh air.",
                        ),
                    });
                }
            });
        }
        egui::Grid::new("vent_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            if room.mechanical_enabled {
                let label = match room.mechanical_mode {
                    VentilationMode::Extraction => t("Débit d'extraction", "Extraction flow"),
                    VentilationMode::Supply => t("Débit d'air neuf", "Fresh-air flow"),
                };
                num(ui, lang, label, &mut room.mechanical_flow_m3h, 0.0..=1_000_000.0, "m³/h", 0, "");
            }
            num(ui, lang, t("Renouvellement naturel", "Natural air change"), &mut room.natural_ach, 0.0..=50.0, "vol/h", 2, t(
                "Infiltrations, toujours actives. Repères : 0–0,2 local très étanche (chambre froide) ; 0,2–0,5 local fermé ; 0,5–1 local technique peu étanche ; > 1 portes souvent ouvertes.",
                "Infiltration, always active. Guide: 0–0.2 very airtight (cold room); 0.2–0.5 closed room; 0.5–1 leaky plant room; > 1 doors often open.",
            ));
            num(ui, lang, t("Efficacité de ventilation η", "Ventilation effectiveness η"), &mut room.ventilation_efficiency, 0.05..=1.0, "", 2, t(
                "Part du débit réellement utile à la dilution (facteur de mélange). 1 : mélange parfait ; 0,6 : valeur prudente usuelle ; < 0,4 : court-circuit entre soufflage et extraction.",
                "Share of the flow actually diluting (mixing factor). 1: perfect mixing; 0.6: usual prudent value; < 0.4: short-circuit between supply and extract.",
            ));
        });
        ui.horizontal_wrapped(|ui| {
            for (v, fr, en) in [
                (1.0, "idéal", "ideal"),
                (0.8, "bon brassage", "good mixing"),
                (0.6, "prudent", "prudent"),
                (0.4, "médiocre", "poor"),
            ] {
                if ui
                    .selectable_label(
                        (room.ventilation_efficiency - v).abs() < 1e-6,
                        format!("{} {}", fmt_num(v, 1, lang), lang.pick(fr, en)),
                    )
                    .clicked()
                {
                    room.ventilation_efficiency = v;
                }
            }
        });
        let eff = room.effective_ventilation_m3s() * 3600.0;
        hint(
            ui,
            &format!(
                "{} {} — {} {} ({} {})",
                t("Renouvellement nominal :", "Nominal air change:"),
                fmt_unit(room.nominal_ach(), 2, "vol/h", lang),
                t("débit effectif η·Q :", "effective flow η·Q:"),
                fmt_unit(eff, usize::from(eff < 100.0), "m³/h", lang),
                fmt_unit(eff / room.volume_m3, 2, "vol/h", lang),
                t("efficaces", "effective"),
            ),
        );
        if eff <= 0.0 {
            theme::callout(
                ui,
                theme::WARNING,
                t(
                    "Aucune ventilation : la concentration ne redescendra pas après la fuite.",
                    "No ventilation: the concentration will not decrease after the release.",
                ),
            );
        }
        let default_flow = room.mechanical_flow_m3h;
        ui.add_space(4.0);
        events_editor(ui, &mut app.config.events, default_flow, lang);
    });
}

/// Timeline of the scenario: ventilation failure, backup fan or isolation of
/// the leak at a given time after the start of the release.
fn events_editor(ui: &mut Ui, events: &mut Vec<Event>, default_flow: f64, lang: Language) {
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    let base = t(
        "Événements pendant le rejet (panne, secours, isolement)",
        "Events during the release (failure, backup, isolation)",
    );
    let title = if events.is_empty() { base.to_string() } else { format!("{base} — {}", events.len()) };
    // Open the section as soon as events appear (example or file loaded), so
    // that an assumed failure or backup fan is never hidden from the user.
    let seen_id = ui.id().with("events_seen");
    let seen: usize = ui.ctx().data(|d| d.get_temp(seen_id)).unwrap_or(0);
    let force = if !events.is_empty() && seen == 0 { Some(true) } else { None };
    ui.ctx().data_mut(|d| d.insert_temp(seen_id, events.len()));
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_source("events")
        .default_open(!events.is_empty())
        .open(force)
        .show(ui, |ui| {
        hint(
            ui,
            t(
                "Instants comptés depuis le début du rejet. Sans événement, la ventilation décrite ci-dessus fonctionne pendant toute la durée. Le scénario « panne de ventilation » des analyses reste une panne totale dès le début, sans secours.",
                "Times counted from the start of the release. Without events, the ventilation above runs for the whole duration. The \"ventilation failure\" scenario of the analyses remains a total failure from the start, without backup.",
            ),
        );
        let fmt = move |v: f64, d: usize| fmt_num(v, d, lang).replace('\u{202f}', "");
        let parse = |s: &str| s.replace(',', ".").replace(['\u{202f}', ' ', '\u{a0}'], "").parse().ok();
        let mut remove: Option<usize> = None;
        egui::Grid::new("events_grid").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            for (i, e) in events.iter_mut().enumerate() {
                let mut minutes = e.time_s / 60.0;
                if ui
                    .add(
                        DragValue::new(&mut minutes)
                            .range(0.0..=10_080.0)
                            .speed(0.1)
                            .max_decimals(2)
                            .custom_formatter(move |v, _| fmt(v, 2))
                            .custom_parser(parse)
                            .suffix(" min"),
                    )
                    .changed()
                {
                    e.time_s = minutes * 60.0;
                }
                let is_stop = matches!(e.action, EventAction::VentilationStops);
                let is_start = matches!(e.action, EventAction::VentilationStarts { .. });
                let is_iso = matches!(e.action, EventAction::ReleaseIsolated);
                let (stop, start, iso) = (
                    t("Arrêt de la ventilation mécanique", "Mechanical ventilation stops"),
                    t("Démarrage d'une ventilation (secours, relance)", "A ventilation starts (backup, restart)"),
                    t("Isolement de la fuite", "Leak isolated"),
                );
                let selected = if is_stop { stop } else if is_start { start } else { iso };
                egui::ComboBox::from_id_source(("event_action", i)).selected_text(selected).width(260.0).show_ui(ui, |ui| {
                    if ui.selectable_label(is_stop, stop).clicked() {
                        e.action = EventAction::VentilationStops;
                    }
                    if ui.selectable_label(is_start, start).clicked() && !is_start {
                        e.action = EventAction::VentilationStarts {
                            flow_m3h: (2.0 * default_flow).max(300.0),
                            mode: VentilationMode::Extraction,
                        };
                    }
                    if ui.selectable_label(is_iso, iso).clicked() {
                        e.action = EventAction::ReleaseIsolated;
                    }
                });
                if let EventAction::VentilationStarts { flow_m3h, mode } = &mut e.action {
                    ui.horizontal(|ui| {
                        ui.add(
                            DragValue::new(flow_m3h)
                                .range(0.0..=1_000_000.0)
                                .speed(1.0)
                                .max_decimals(0)
                                .custom_formatter(move |v, _| fmt(v, 0))
                                .custom_parser(parse)
                                .suffix(" m³/h"),
                        );
                        let name = |m: VentilationMode| match m {
                            VentilationMode::Extraction => t("extraction", "exhaust"),
                            VentilationMode::Supply => t("soufflage", "supply"),
                        };
                        egui::ComboBox::from_id_source(("event_mode", i)).selected_text(name(*mode)).show_ui(ui, |ui| {
                            for m in [VentilationMode::Extraction, VentilationMode::Supply] {
                                ui.selectable_value(mode, m, name(m));
                            }
                        });
                    });
                } else {
                    ui.label("");
                }
                if ui.small_button("✕").on_hover_text(t("Supprimer", "Remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            events.remove(i);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(t("+ Panne de ventilation", "+ Ventilation failure")).clicked() {
                events.push(Event { time_s: 300.0, action: EventAction::VentilationStops });
            }
            if ui.button(t("+ Ventilation de secours", "+ Backup ventilation")).clicked() {
                events.push(Event {
                    time_s: 120.0,
                    action: EventAction::VentilationStarts {
                        flow_m3h: (2.0 * default_flow).max(300.0),
                        mode: VentilationMode::Extraction,
                    },
                });
            }
            if ui.button(t("+ Isolement de la fuite", "+ Leak isolated")).clicked() {
                events.push(Event { time_s: 600.0, action: EventAction::ReleaseIsolated });
            }
        });
        if !events.is_empty() {
            let mut sorted = events.clone();
            sorted.sort_by(|a, b| a.time_s.total_cmp(&b.time_s).then(a.action.rank().cmp(&b.action.rank())));
            for e in sorted {
                hint(ui, &format!("• {}", e.describe(lang)));
            }
        }
    });
}

fn thresholds(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    header(ui, "thr", t("Seuils d'alarme O₂", "O₂ alarm thresholds"), false, |ui| {
        let th = &mut app.config.thresholds;
        egui::ComboBox::from_id_source("thr_preset").selected_text(th.preset.name(lang)).width(260.0).show_ui(
            ui,
            |ui| {
                for p in ThresholdPreset::all() {
                    if ui.selectable_value(&mut th.preset, p, p.name(lang)).clicked() {
                        if let Some((a1, a2)) = p.levels() {
                            th.alarm1 = a1;
                            th.alarm2 = a2;
                        }
                    }
                }
            },
        );
        if th.preset == ThresholdPreset::Custom {
            egui::Grid::new("thr_grid").num_columns(3).show(ui, |ui| {
                num(ui, lang, t("Alarme 1", "Alarm 1"), &mut th.alarm1, 10.0..=20.8, "%", 1, "");
                let a1 = th.alarm1;
                num(ui, lang, t("Alarme 2", "Alarm 2"), &mut th.alarm2, 6.0..=a1, "%", 1, "");
            });
        }
        hint(ui, t("Les détecteurs O₂ (EN 50104) sont réglés en France le plus souvent à 19 % et 17 % (INRS). OSHA : atmosphère sous-oxygénée sous 19,5 %.", "O₂ detectors (EN 50104) are usually set at 19% and 17% in France (INRS). OSHA: oxygen-deficient below 19.5%."));
    });
}

fn advanced(app: &mut AnoxiaApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = |fr: &'static str, en: &'static str| lang.pick(fr, en);
    header(ui, "adv", t("Options avancées", "Advanced options"), false, |ui| {
        let buoyancy = app.config.buoyancy();
        let height = app.config.room.height_m;
        let m = &mut app.config.model;
        ui.label(RichText::new(t("Modèle bi-zone (stratification)", "Two-zone model (stratification)")).strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut m.stratification, StratificationMode::Auto, t("Auto", "Auto")).on_hover_text(t(
                "Activé pour les gaz lourds (Ar, CO₂) et légers (He, Ne).",
                "Enabled for heavy (Ar, CO₂) and light (He, Ne) gases.",
            ));
            ui.radio_value(&mut m.stratification, StratificationMode::On, t("Activé", "On"));
            ui.radio_value(&mut m.stratification, StratificationMode::Off, t("Désactivé", "Off"));
        });
        if m.two_zone_active(buoyancy) {
            egui::Grid::new("adv_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                num(ui, lang, t("Épaisseur de la zone d'accumulation", "Accumulation layer thickness"), &mut m.zone_height_m, 0.1..=(height * 0.9), "m", 2, t("Couche au sol (gaz lourd) ou sous plafond (gaz léger) où respire la personne exposée.", "Floor layer (heavy gas) or ceiling layer (light gas) where the exposed person breathes."));
                let mut pct = m.injection_fraction * 100.0;
                if num(ui, lang, t("Part du rejet dans cette zone", "Share of release into the zone"), &mut pct, 0.0..=100.0, "%", 0, "") {
                    m.injection_fraction = pct / 100.0;
                }
                num(ui, lang, t("Échange inter-zones", "Inter-zone exchange"), &mut m.exchange_ach, 0.0..=100.0, t("vol/h (zone)", "vol/h (zone)"), 1, t("Brassage entre les deux zones, en volumes de la zone d'accumulation par heure. Faible = forte stratification.", "Mixing between zones, in volumes of the accumulation zone per hour. Low = strong stratification."));
            });
            ui.horizontal(|ui| {
                ui.label(t("Extraction :", "Extraction:"));
                egui::ComboBox::from_id_source("extract").selected_text(extraction_name(m.extraction, lang)).show_ui(
                    ui,
                    |ui| {
                        for e in [
                            ExtractionLocation::Distributed,
                            ExtractionLocation::AccumulationZone,
                            ExtractionLocation::OppositeZone,
                        ] {
                            ui.selectable_value(&mut m.extraction, e, extraction_name(e, lang));
                        }
                    },
                );
            });
            hint(
                ui,
                t(
                    "Modèle indicatif : le classement retient le plus défavorable des deux modèles.",
                    "Indicative model: the classification uses the worse of the two models.",
                ),
            );
        }
        ui.separator();
        ui.label(
            RichText::new(t("Champ proche (exposition à la source)", "Near field (exposure at the source)")).strong(),
        );
        ui.checkbox(&mut m.near_field, t("Calculer l'oxygène près du point de fuite", "Compute oxygen next to the release point"))
            .on_hover_text(t(
                "Modèle champ proche / champ lointain (Nicas 1996, AIHA IH Mod, ODHMOD) : demi-sphère autour de la fuite échangeant avec le reste du local à ½ × surface × vitesse d'air.",
                "Near-field / far-field model (Nicas 1996, AIHA IH Mod, ODHMOD): hemisphere around the leak exchanging with the rest of the room at ½ × surface × air speed.",
            ));
        if m.near_field {
            egui::Grid::new("adv_grid_nf").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                num(ui, lang, t("Rayon du champ proche", "Near-field radius"), &mut m.near_field_radius_m, 0.3..=5.0, "m", 1, t(
                    "Distance de la personne au point de fuite (demi-sphère). 1 m : opérateur sur l'équipement.",
                    "Distance of the person to the leak (hemisphere). 1 m: operator at the equipment.",
                ));
                num(ui, lang, t("Vitesse d'air aléatoire", "Random air speed"), &mut m.near_field_air_speed_m_s, 0.01..=1.0, "m/s", 2, t(
                    "Mesurée en atelier : 0,05 à 0,3 m/s (Baldwin & Maynard 1998). 0,06 m/s = valeur prudente d'ODHMOD.",
                    "Measured in workplaces: 0.05 to 0.3 m/s (Baldwin & Maynard 1998). 0.06 m/s = ODHMOD's prudent value.",
                ));
            });
            ui.checkbox(
                &mut m.near_field_in_assessment,
                t("Retenir le champ proche pour le classement", "Include the near field in the classification"),
            )
            .on_hover_text(t(
                "Par défaut le classement suit les modèles de local (ce que mesurent les détecteurs) et le champ proche est affiché à part, avec un avertissement s'il est plus sévère.",
                "By default the classification follows the room models (what detectors measure) and the near field is shown separately, with a warning when it is more severe.",
            ));
            hint(ui, t(
                "Moyenne de zone pour un rejet à faible vitesse : pour un jet sous pression, l'entraînement d'air n'est pas résolu et la concentration dans l'axe du jet peut être bien plus élevée que cette moyenne.",
                "Zone average for a low-momentum release: for a pressurised jet, air entrainment is not resolved and the concentration in the jet axis can be much higher than this average.",
            ));
        }
        ui.separator();
        ui.label(RichText::new(t("Vidange du récipient", "Container blowdown")).strong());
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut m.blowdown,
                Blowdown::Isothermal,
                t("Isotherme (prudent)", "Isothermal (conservative)"),
            )
            .on_hover_text(t(
                "Le gaz reste à température ambiante : débit maximal. Adapté aux fuites lentes.",
                "Gas stays at ambient temperature: highest flow. Suited to slow leaks.",
            ));
            ui.radio_value(&mut m.blowdown, Blowdown::Adiabatic, t("Adiabatique", "Adiabatic")).on_hover_text(t(
                "Le gaz se refroidit en se détendant (vidanges rapides). Option indicative : loi isentropique de gaz parfait (TNO Yellow Book), vérifiée à basse pression.",
                "The gas cools as it expands (fast blowdowns). Indicative option: ideal-gas isentropic law (TNO Yellow Book), verified at low pressure.",
            ));
        });
        ui.separator();
        egui::Grid::new("adv_grid2").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            num(
                ui,
                lang,
                t("Durée maximale simulée", "Maximum simulated time"),
                &mut m.max_duration_h,
                0.1..=168.0,
                "h",
                1,
                "",
            );
            let room = &mut app.config.room;
            num(
                ui,
                lang,
                t("Température du local", "Room temperature"),
                &mut room.temperature_c,
                -30.0..=60.0,
                "°C",
                0,
                "",
            );
            num(
                ui,
                lang,
                t("Pression atmosphérique", "Atmospheric pressure"),
                &mut room.pressure_hpa,
                500.0..=1100.0,
                "hPa",
                0,
                t(
                    "≈ 1013 hPa au niveau de la mer, ≈ 900 hPa à 1000 m.",
                    "≈ 1013 hPa at sea level, ≈ 900 hPa at 1000 m.",
                ),
            );
        });
    });
}

pub fn extraction_name(e: ExtractionLocation, lang: Language) -> &'static str {
    match e {
        ExtractionLocation::Distributed => lang.pick("répartie sur la hauteur", "distributed over height"),
        ExtractionLocation::AccumulationZone => lang.pick("dans la zone d'accumulation", "in the accumulation zone"),
        ExtractionLocation::OppositeZone => lang.pick("à l'opposé de la zone", "opposite to the zone"),
    }
}
