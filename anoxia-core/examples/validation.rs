//! Prints the validation dossier as a Markdown table:
//! cargo run -p anoxia-core --example validation --release
use anoxia_core::validation::{cases, count, score, Category, Nature};
use anoxia_core::Language;

fn main() {
    let lang = Language::French;
    let all = cases();
    for cat in [Category::Properties, Category::Source, Category::Method, Category::Experiment] {
        println!("\n### {}\n", cat.name(lang));
        println!("| Id | Nature | Cas | Référence | Publié | Calculé | Critère | Résultat |");
        println!("|---|---|---|---|---|---|---|---|");
        for c in all.iter().filter(|c| c.category == cat) {
            let status = if c.documented() {
                "ⓘ écart documenté"
            } else if c.passed() {
                "✅"
            } else {
                "❌"
            };
            let unit = c.unit.trim_start_matches('–');
            println!(
                "| {} | {} | {} | {} | {} {} | {} {} | {} | {} |",
                c.id,
                c.nature().short(lang),
                c.description(lang),
                c.reference,
                anoxia_core::i18n::fmt_num(c.published, 4, lang),
                unit,
                anoxia_core::i18n::fmt_num(c.computed, 4, lang),
                unit,
                c.criterion.describe(lang),
                status
            );
        }
    }
    let (ok, n) = score(&all);
    let documented = all.iter().filter(|c| c.documented()).count();
    println!(
        "\n**{} cas au total** : {} validations sur données indépendantes, {} reproductions de méthodes publiées, {} vérifications analytiques. **{ok} / {n} cas contrôlés conformes** et **{documented} cas documentés** (écarts méthodologiques ou données à confirmer, hors contrôle réussite/échec).",
        all.len(),
        count(&all, Nature::Validation),
        count(&all, Nature::Reproduction),
        count(&all, Nature::Verification)
    );
}
