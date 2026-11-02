# Dossier de vérification et de validation

Ce dossier compare Anoxia Simulator à des références indépendantes, dans l'esprit de l'ASTM E1355 (vérification, validation, domaine d'emploi).

Chaque cas est **codé dans le logiciel** (`anoxia-core/src/validation.rs`) :
- il est exécuté par la suite de tests à chaque modification ; tout écart hors tolérance fait échouer l'intégration continue ;
- il est consultable dans l'application (*Aide → Dossier de validation*) et recalculé en direct ;
- le tableau ci-dessous est régénéré par `cargo run -p anoxia-core --example validation --release`.

## 1. Trois natures de cas, à ne pas confondre

| Nature | Ce que le cas démontre | Exemples |
|---|---|---|
| **Validation** | Accord avec des données indépendantes du logiciel : base de référence, données fournisseur, mesures. | NIST REFPROP, fiches Linde, essais NRC Canada, calorimètre, MiniCLEAN, Factory Mutual |
| **Reproduction** | Le logiciel refait correctement le calcul d'une méthode publiée. Il s'agit souvent des mêmes équations, donc ce n'est pas une preuve de réalisme. | INRS ED 155, BCGA CP 52, Fermilab FESHM 4240, TNO, Crowl & Louvar, York, Nottingham |
| **Vérification** | Le schéma numérique résout exactement les équations (comparaison à une solution analytique). | Vidange isotherme et adiabatique, formule d'efflux libre NFPA 2001 |

Un **écart documenté** (ⓘ) n'est pas un succès. C'est une différence de méthode, expliquée, entre le logiciel et une référence. Il est laissé visible plutôt que masqué par une tolérance élargie.

**Esprit critique sur les sources.** Une référence peut elle-même arrondir ou se tromper. Quand une source isolée s'écarte des autres, l'écart est analysé avant toute modification du modèle. Exemples rencontrés :
- le TNO Yellow Book donne, pour le même exemple, 12,814 bar dans son tableau et 12,417 bar dans son encadré ;
- l'INRS et la BCGA arrondissent le contenu d'une B50 d'azote à 200 bar à 10 m³ (P × V, gaz parfait), alors que NIST et Linde donnent 9,4 à 9,5 m³ ;
- l'université de Nottingham calcule l'argon en gaz parfait (9,87 m³), alors que l'argon réel (Z = 0,944) en contient environ 10,4 m³ ;
- la page de Monash annonce un local de 270 m³ mais calcule avec 250 m³ (cas non retenu).

## 2. Résultats
### Propriétés des gaz

| Id | Nature | Cas | Référence | Publié | Calculé | Critère | Résultat |
|---|---|---|---|---|---|---|---|
| P1 | Validation | Facteur de compressibilité Z — azote, 200 bar, 15 °C | NIST Chemistry WebBook (REFPROP), isotherme 15 °C | 1,0478  | 1,0476  | ± 0,3 % | ✅ |
| P2 | Validation | Facteur de compressibilité Z — argon, 200 bar, 15 °C | NIST Chemistry WebBook (REFPROP), isotherme 15 °C | 0,9368  | 0,9365  | ± 0,3 % | ✅ |
| P3 | Validation | Facteur de compressibilité Z — hélium, 300 bar, 15 °C | NIST Chemistry WebBook (REFPROP), isotherme 15 °C | 1,1440  | 1,1440  | ± 0,3 % | ✅ |
| P4 | Validation | Facteur de compressibilité Z — néon, 300 bar, 15 °C | NIST Chemistry WebBook (REFPROP), isotherme 15 °C | 1,1523  | 1,1523  | ± 0,3 % | ✅ |
| P1b | Validation | Facteur Z — argon, 162 bar, 20 °C (minimum de Z, hors grille) | NIST Chemistry WebBook (REFPROP), point hors grille | 0,9400  | 0,9404  | ± 0,3 % | ✅ |
| P2b | Validation | Facteur Z — argon, 162 bar, 15 °C (hors grille) | NIST Chemistry WebBook (REFPROP), point hors grille | 0,9329  | 0,9331  | ± 0,3 % | ✅ |
| P3b | Validation | Facteur Z — azote, 212 bar, 20 °C (hors grille) | NIST Chemistry WebBook (REFPROP), point hors grille | 1,0608  | 1,0611  | ± 0,3 % | ✅ |
| P4b | Validation | Facteur Z — azote, 333 bar, −5 °C (hors grille) | NIST Chemistry WebBook (REFPROP), point hors grille | 1,1656  | 1,1657  | ± 0,3 % | ✅ |
| P5b | Validation | Facteur Z — hélium, 137 bar, 20 °C (hors grille) | NIST Chemistry WebBook (REFPROP), point hors grille | 1,0656  | 1,0656  | ± 0,3 % | ✅ |
| P5 | Validation | Contenu d'une bouteille B50 d'azote à 200 bar (m³ à 15 °C, 1 bar) | Linde GmbH, fiches produit (Nitrogen, Argon 5.0, Helium 4.6) | 9,5300 m³ | 9,5420 m³ | ± 1,5 % | ✅ |
| P6 | Validation | Contenu d'une bouteille B50 d'argon à 200 bar (m³ à 15 °C, 1 bar) | Linde GmbH, fiches produit (Nitrogen, Argon 5.0, Helium 4.6) | 10,7000 m³ | 10,6695 m³ | ± 1,5 % | ✅ |
| P7 | Validation | Contenu d'une bouteille B50 d'hélium à 200 bar (m³ à 15 °C, 1 bar) | Linde GmbH, fiches produit (Nitrogen, Argon 5.0, Helium 4.6) | 9,1300 m³ | 9,1197 m³ | ± 1,5 % | ✅ |
| P8 | Validation | Pression de vapeur saturante du CO₂ à 0 °C | NIST Chemistry WebBook — CO₂, propriétés à saturation | 34,8514 bar | 34,8504 bar | ± 0,5 % | ✅ |
| P9 | Validation | Pression de vapeur saturante du CO₂ à 10 °C | NIST Chemistry WebBook — CO₂, propriétés à saturation | 45,0218 bar | 45,0221 bar | ± 0,5 % | ✅ |
| P10 | Validation | Pression de vapeur saturante du CO₂ à 20 °C | NIST Chemistry WebBook — CO₂, propriétés à saturation | 57,2905 bar | 57,2922 bar | ± 0,5 % | ✅ |
| P11 | Validation | Pression de vapeur saturante du CO₂ à 25 °C | NIST Chemistry WebBook — CO₂, propriétés à saturation | 64,3424 bar | 64,3434 bar | ± 0,5 % | ✅ |
| P12 | Validation | Masse volumique du CO₂ liquide saturé à 20 °C | NIST Chemistry WebBook — CO₂ à saturation, 20 °C | 773,3870 kg/m³ | 773,3901 kg/m³ | ± 1,0 % | ✅ |
| P13 | Validation | Masse volumique de la vapeur de CO₂ saturée à 20 °C | NIST Chemistry WebBook — CO₂ à saturation, 20 °C | 194,2020 kg/m³ | 194,2057 kg/m³ | ± 1,0 % | ✅ |
| P14 | Validation | Volume de gaz d'une bouteille B50 de CO₂ (37,5 kg) à 20 °C, 1 atm | NIST WebBook (ρ = 1,839 kg/m³ à 20 °C, 1 atm) ; Linde BIOGON C (B50 = 37,5 kg) | 20,3900 m³ | 20,4971 m³ | plus prudent, écart ≤ 0,25 | ✅ |

### Terme source (débit, vidange)

| Id | Nature | Cas | Référence | Publié | Calculé | Critère | Résultat |
|---|---|---|---|---|---|---|---|
| S1 | Reproduction | Débit sonique d'hydrogène, trou de 100 mm, 50 bar, Cd = 0,62 | TNO Yellow Book CPR 14E (2005), §2.6.2.1, tableau 2.3 | 15,3100 kg/s | 15,3305 kg/s | ± 0,5 % | ✅ |
| S2 | Reproduction | Fuite d'azote par un trou de 0,1 in, 200 psig, 80 °F, C0 = 1 | Crowl & Louvar, Chemical Process Safety, 2e éd., exemple 4-4 | 0,0172 kg/s | 0,0173 kg/s | ± 1,0 % | ✅ |
| S3 | Vérification | Vidange adiabatique d'un réservoir d'hydrogène : pression à 10,2 s (équation du modèle adiabatique) | TNO Yellow Book CPR 14E, tableau 2.3 (t = 10,2 s) | 30,2300 bar | 30,1751 bar | ± 1,0 % | ✅ |
| S4 | Vérification | Vidange adiabatique d'une B50 d'azote à 10 bar par 1 mm : pression à t = τ | Solution analytique (gaz parfait, régime sonique) | 2,7915 bar | 2,7843 bar | ± 1,5 % | ✅ |
| S5 | Vérification | Vidange isotherme d'une B50 d'azote à 10 bar par 1 mm : pression à t = τ | Solution analytique (gaz parfait, régime sonique) | 3,6788 bar | 3,6819 bar | ± 1,5 % | ✅ |

### Méthodes de calcul publiées

| Id | Nature | Cas | Référence | Publié | Calculé | Critère | Résultat |
|---|---|---|---|---|---|---|---|
| M1 | Reproduction | B50 d'azote à 200 bar libérée d'un coup dans 75 m³, sans ventilation | BCGA CP 52 (2023), annexe 1, feuille 1 | 18,2000 % O₂ | 18,3316 % O₂ | ± 0,15 | ✅ |
| M2 | Reproduction | B50 d'azote à 200 bar dans un local de 65 m³, sans apport d'air | INRS ED 155 (2024), exemple 1 | 17,8000 % O₂ | 17,9288 % O₂ | ± 0,15 | ✅ |
| M3 | Reproduction | B50 d'azote à 200 bar dans un local de 23,4 m³ | INRS ED 155 (2024), exemple 1, variante | 12,0000 % O₂ | 12,5577 % O₂ | écart documenté | ⓘ écart documenté |
| M3b | Reproduction | Même cas, contrôle « méthode simplifiée INRS / BCGA » affiché par le logiciel | INRS ED 155 (2024), exemple 1, variante — méthode simplifiée | 12,0000 % O₂ | 12,0256 % O₂ | ± 0,05 | ✅ |
| M4 | Reproduction | B50 d'argon à 200 bar dans 18 m³ d'air libre | University of Nottingham, Gas calculation guidance (2025), exemple 1 | 9,4600 % O₂ | 8,7855 % O₂ | plus prudent, écart ≤ 0,7 | ✅ |
| M5 | Reproduction | Argon 40 L/min pendant 1 h 50 dans 6 m³ sans apport d'air : classement | INRS ED 155 (2024), exemple 2 (soudage à l'argon) | 5,6000 % O₂ | 9,6788 % O₂ | écart documenté | ⓘ écart documenté |
| M6 | Reproduction | Fuite de 1,1 m³/h dans 32 m³ à 0,4 vol/h : O₂ après 2 h | BCGA CP 52 (2023), §9.3 et annexe 1, feuilles 3–4 | 20,0400 % O₂ | 20,0352 % O₂ | ± 0,03 | ✅ |
| M7 | Reproduction | Même cas : O₂ en régime permanent | BCGA CP 52 (2023), §9.3 (régime permanent) | 19,3400 % O₂ | 19,3381 % O₂ | ± 0,03 | ✅ |
| M8 | Reproduction | Fuite de CO₂ de 1,2 m³/h dans 240 m³ ventilés à 5 vol/h : CO₂ en régime permanent | University of York, Biology H&S, « Leakage calculations » | 0,1000 % CO₂ | 0,0999 % CO₂ | ± 0,005 | ✅ |
| M9 | Reproduction | Évaporation d'azote liquide (0,171 m³/h) dans 33,6 m³ à 0,5 vol/h | University of York, Biology H&S, ventilation LN₂, exemple (a) | 20,8000 % O₂ | 20,7884 % O₂ | ± 0,05 | ✅ |
| M10 | Reproduction | Vaporisation totale de 100 L d'azote liquide (68,3 m³) dans 140 m³ : méthode par déplacement | University of York, Biology H&S, ventilation LN₂, exemple (b) | 10,7000 % O₂ | 10,7294 % O₂ | ± 0,1 | ✅ |
| M11 | Reproduction | Hall de 14 063 ft³, extraction 1 800 cfm, rejet 818 scfm : O₂ permanent | Fermilab FESHM 4240 — analyse ODH du hall MTA (Geynisman 2008) | 11,4600 % O₂ | 11,4567 % O₂ | ± 0,03 | ✅ |
| M12 | Reproduction | Hall de 14 063 ft³, extraction 1 800 cfm, rejet 530 scfm : O₂ permanent | Fermilab FESHM 4240 — analyse ODH du hall MTA (Geynisman 2008) | 14,8200 % O₂ | 14,8167 % O₂ | ± 0,03 | ✅ |
| M13 | Reproduction | Hall de 14 063 ft³, extraction 1 800 cfm, rejet 259 scfm : O₂ permanent | Fermilab FESHM 4240 — analyse ODH du hall MTA (Geynisman 2008) | 17,9800 % O₂ | 17,9783 % O₂ | ± 0,03 | ✅ |
| M14 | Reproduction | Hall de 14 063 ft³, extraction 1 800 cfm, rejet 65 scfm : O₂ permanent | Fermilab FESHM 4240 — analyse ODH du hall MTA (Geynisman 2008) | 20,2400 % O₂ | 20,2417 % O₂ | ± 0,03 | ✅ |
| M15 | Reproduction | Hall de 14 063 ft³, extraction 1 800 cfm, rejet 1 859 scfm (> extraction) | Fermilab FESHM 4240 — analyse ODH du hall MTA (Geynisman 2008) | 0,0000 % O₂ | 0,0000 % O₂ | ± 0,03 | ✅ |
| O1 | Reproduction | Exemple 10 (local de 10 m³, N₂ 0,1 m³/s, 120 s) : temps pour 12 % d'O₂ près de la fuite (r = 1 m) | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 26,2000 s | 26,4295 s | ± 5,0 % | ✅ |
| O2 | Reproduction | Exemple 10 : O₂ minimal près de la fuite | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 4,3600 % O₂ | 4,3742 % O₂ | ± 0,2 | ✅ |
| O3 | Reproduction | Exemple 10 : temps pour 12 % d'O₂ dans le reste du local (champ lointain) | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 58,0000 s | 55,3544 s | ± 6,0 % | ✅ |
| O4 | Reproduction | Exemple 10 : O₂ final du local après le rejet (champ lointain) | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 5,8000 % O₂ | 6,2598 % O₂ | écart documenté | ⓘ écart documenté |
| O5 | Reproduction | Exemple 1 (100 m³, N₂ + Ar 0,1 m³/s, soufflage 1,5 m³/s) : temps pour 18 % d'O₂ près de la fuite (r = 2 m) | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 69,2000 s | 72,4392 s | ± 6,0 % | ✅ |
| O6 | Reproduction | Exemple 1 : O₂ près de la fuite à la fin du rejet (1 000 s) | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 17,2200 % O₂ | 17,2555 % O₂ | ± 0,2 | ✅ |
| O7 | Reproduction | Exemple 1 : O₂ du reste du local à la fin du rejet (1 000 s) | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 19,5600 % O₂ | 19,6070 % O₂ | ± 0,1 | ✅ |
| O8 | Reproduction | Exemple 7 (250 m³, N₂ froid 0,1 m³/s, soufflage froid 1 m³/s) : temps pour 18 % d'O₂ près de la fuite | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 94,3500 s | 95,7965 s | ± 8,0 % | ✅ |
| O9 | Reproduction | Exemple 7 : O₂ minimal près de la fuite | Stefana et al. (2021), PSEP 149, 537 — classeur ODHMOD v1.02 (AIHA) | 17,8500 % O₂ | 17,8490 % O₂ | ± 0,3 | ✅ |
| V1 | Vérification | B50 d'azote, fuite Ø 2 mm, 45 m³ : panne de la ventilation dès le début du rejet contre ventilation à l'arrêt | Vérification de la chronologie : panne à t = 0 ≡ scénario sans ventilation mécanique | 17,1248 % O₂ | 17,1248 % O₂ | ± 0,001 | ✅ |
| V2 | Vérification | 10 m³/h d'argon pendant 30 min dans 50 m³ sans ventilation, soufflage de 100 m³/h démarré à 60 min : fraction de gaz restante à t₀ + V/Q | Solution analytique : décroissance exponentielle après le démarrage d'une ventilation (x/x₀ = e⁻¹ à t₀ + V/Q) | 0,3679  | 0,3679  | ± 0,5 % | ✅ |

### Essais et normes de conception

| Id | Nature | Cas | Référence | Publié | Calculé | Critère | Résultat |
|---|---|---|---|---|---|---|---|
| E1 | Vérification | Noyage à l'azote : 0,4155 m³/m³ doit donner 34 % d'agent | NFPA 2001 (2004), tableau A.5.5.2(d) — IG-100, 20 °C | 34,0000 % | 33,9990 % | ± 0,2 | ✅ |
| E2 | Vérification | Noyage à l'azote : 0,5109 m³/m³ doit donner 40 % d'agent | NFPA 2001 (2004), tableau A.5.5.2(d) — IG-100, 20 °C | 40,0000 % | 40,0045 % | ± 0,2 | ✅ |
| E3 | Vérification | Noyage à l'azote : 0,6734 m³/m³ doit donner 49 % d'agent | NFPA 2001 (2004), tableau A.5.5.2(d) — IG-100, 20 °C | 49,0000 % | 49,0028 % | ± 0,2 | ✅ |
| E4 | Vérification | Noyage à l'azote : 0,9677 m³/m³ doit donner 62 % d'agent | NFPA 2001 (2004), tableau A.5.5.2(d) — IG-100, 20 °C | 62,0000 % | 62,0044 % | ± 0,2 | ✅ |
| E5 | Validation | Essai : décharge d'argon à 40 % en 58 s dans un local de 121 m³ — O₂ stabilisé mesuré 12,5–12,8 % | Su, Kim, Crampton & Liu (NRC Canada), J. Fire Prot. Eng. 11 (2001) 72–87 | 12,6500 % O₂ | 12,5703 % O₂ | ± 0,2 | ✅ |
| E6 | Validation | Infusion de 1,59 L/min dans une chambre de 21,4 m³ balayée à 80 L/min : O₂ en régime permanent | Calorimètre de chambre entière, infusion d'azote — Physiological Reports 5 (2017), PMC5704087 | 20,5300 % O₂ | 20,5345 % O₂ | ± 0,05 | ✅ |
| E7 | Validation | 105 kg d'azote vaporisés dans un hall de 5 700 m³ à 1,18 atm : baisse d'O₂ mesurée 0,25–0,30 point | MiniCLEAN, essais de déversement d'azote liquide à SNOLAB, essai 1 (arXiv:1711.02117) | 0,2750 pts O₂ | 0,2790 pts O₂ | écart documenté | ⓘ écart documenté |
| E8 | Validation | Décharge de 1 255 kg de CO₂ en 207 s dans 993 m³ : pic de CO₂ mesuré ≈ 40 % | Zalosh & Hung, Fire Safety Science 4 (1994) 889–900 — essai FM n° 2 | 40,0000 % CO₂ | 49,9028 % CO₂ | plus prudent, écart ≤ 12 | ✅ |

**59 cas au total** : 23 validations sur données indépendantes, 27 reproductions de méthodes publiées, 9 vérifications analytiques. **55 / 55 cas contrôlés conformes** et **4 cas documentés** (écarts méthodologiques ou données à confirmer, hors contrôle réussite/échec).

## 3. Écarts expliqués

| Cas | Écart | Explication | Ce que fait le logiciel |
|---|---|---|---|
| M1, M2 | +0,13 point d'O₂ | La BCGA et l'INRS prennent 10 m³ d'azote (gaz parfait) ; le gaz réel en contient 9,37 m³ à 20 °C. | Il affiche aussi le **calcul manuel simplifié INRS/BCGA**, reproduit exactement (M3b). |
| M3 (ⓘ) | +0,56 point | Même cause, plus marquée dans un petit local. | Idem. |
| M4 | −0,68 point (plus prudent) | Nottingham suppose un gaz parfait pour l'argon. | — |
| M5 (ⓘ) | 9,7 % contre 5,6 % | L'INRS applique la formule de déplacement (sans mélange) à un débit continu ; le modèle bien mélangé donne 10,1 %, la zone basse du modèle bi-zone ≈ 9,7 % selon ses paramètres. | Il **signale** que la méthode par déplacement donne un classement plus sévère, avec sa valeur. |
| E5 | — | La quantité d'argon a été dimensionnée par les expérimentateurs avec la formule d'efflux libre : la mesure confirme cette relation, ce n'est pas une prédiction aveugle. | — |
| E8 | +9,8 points de CO₂ (plus prudent) | Neige carbonique sublimée plus tard, fuites pendant la décharge, stratification du nuage froid. | Surestimation du côté de la sécurité. |
| E7 (ⓘ) | — | Donnée MiniCLEAN (annexe B, arXiv:1711.02117) non vérifiée sur le document source : page, figure et incertitude à confirmer. | Compté comme cas documenté, pas comme validation acquise. |
| O4 (ⓘ) | 6,3 % contre 5,8 % | ODHMOD expulse l'air du champ lointain, moins appauvri que la moyenne, donc le local perd plus d'O₂ ; la relation d'efflux libre (NFPA 2001, INRS) suppose un mélange expulsé uniforme. | Il conserve la relation normalisée pour la moyenne du local et **reproduit le champ proche** d'ODHMOD (O1, O2, O5, O6, O8, O9). |

## 4. Choix de modélisation issus des comparaisons

Les comparaisons ci-dessus ont fixé les choix suivants :
1. **Ventilation par extraction.** L'air neuf entrant vaut (débit extrait − débit de fuite), et non le débit extrait : compter le débit extrait en air neuf surestime la dilution (16,2 % au lieu de 14,8 % sur le cas Fermilab M12).
2. **Efficacité de ventilation.** Elle s'applique au **flux sortant total**, gaz compris (définition usuelle en hygiène industrielle) : x = Q_gaz / (η·Q_extrait). L'appliquer au seul air neuf serait optimiste : 12,4 % au lieu de 10,7 % sur un cas Fermilab avec η = 0,6. Extraction et infiltrations se combinent en quadrature (ASHRAE).
3. **Rejet instantané.** Il est classé avec la méthode par déplacement INRS/BCGA (prudente) ; la relation à efflux libre NFPA 2001 est affichée en meilleure estimation.
4. **CO₂ en bouteille pleine de liquide** (au-delà de ≈ 22 °C pour 0,75 kg/L). La bouteille rejette du liquide même debout, avec un débit 3 à 4 fois supérieur ; la pression est prise à la saturation, ce qui sous-estime le liquide comprimé (domaine signalé comme indicatif).
5. **Facteur de compressibilité.** Il est interpolé dans une table NIST REFPROP (−40 à +70 °C, 1 à 750 bar) ; pour le CO₂, l'interpolation se fait en pression réduite jusqu'à la vapeur saturante. Peng-Robinson s'écartait de 2 à 2,5 %.
6. **Gaz froid issu d'un liquide cryogénique.** Il est traité comme plus lourd que l'air (couche au sol), quel que soit le gaz.
7. **Libellés.** Ils sont propres au CO₂ quand c'est lui qui fixe le classement (« CO₂ > DIVS »).
8. **Vidange adiabatique.** Elle est présentée comme indicative : loi isentropique de gaz parfait, vérifiée à basse pression seulement. Le mode isotherme, plus prudent pour l'oxygène, reste la référence.

## 5. Domaine d'emploi

| Domaine | Statut |
|---|---|
| Contenu des récipients (N₂, Ar, He, Ne jusqu'à 750 bar ; CO₂ liquéfié jusqu'à 25 °C) | **Validé** (NIST, Linde) à mieux que 1,5 % |
| Débit à l'orifice (sonique et subsonique) | **Reproduit** (TNO, Crowl & Louvar) à mieux que 1 % |
| Vidange isotherme | **Vérifiée** (analytique) |
| Vidange adiabatique | **Vérifiée** à basse pression ; indicative à haute pression |
| Bilan bien mélangé, avec ou sans ventilation | **Reproduit** (BCGA, Fermilab, York) et **validé** (calorimètre, MiniCLEAN) |
| Rejet massif rapide dans un local fermé | **Validé** (NRC Canada) ; prudent pour le CO₂ (Factory Mutual) |
| Stratification (modèle bi-zone) | **Indicatif** : tendance qualitative confirmée (NRC Canada, LAPD, MiniCLEAN), paramètres non calés |
| Champ proche (exposition à la source) | **Reproduit** (ODHMOD, 8 cas) : moyenne de zone pour un rejet à faible vitesse ; l'entraînement d'un jet sous pression n'est pas résolu |
| Plusieurs locaux, fosses, flaques cryogéniques | **Hors domaine** : CFD ou étude spécifique |

## 6. Pistes pour aller plus loin

- **Caler le modèle bi-zone** sur des mesures à plusieurs hauteurs (O'Rourke 2005, Hetrick & Rangwala 2010, Su et al. 2001, à numériser).
- **Valider en aveugle E5** avec la masse d'argon réellement déchargée, si elle peut être obtenue.
- **Comparer à ODHMOD** : fait (cas O1 à O9) ; proposer aux auteurs un échange sur leurs treize exemples.
- **Mener une campagne de mesure propre** : bouteille dans un local d'essai, capteurs O₂ à plusieurs hauteurs.
- **Obtenir une revue indépendante** (INERIS, université) et publier.

## Annexe — Positionnement par rapport aux outils existants

Positionnement par rapport aux outils gratuits et payants identifiés (novembre 2026) :

| Outil | Prix | Temporel | Vidange réelle | CO₂ toxique | Stratification | Dimensionnement | Validation publiée |
|---|---|---|---|---|---|---|---|
| **Anoxia Simulator** | Gratuit (AGPL) | Oui (exact, chronologie d'événements) | Oui (gaz réel NIST) | Oui | Bi-zone indicatif + champ proche | Ventilation + diamètre | Ce dossier |
| Calculateurs web (Nottingham, Cryolab, ToolGrit…) | Gratuit | Non | Non | Partiel | Non | Non | Non |
| Méthode Fermilab / JLab / LBNL | Gratuit | Analytique | Non (débit constant) | Non | Non | Manuel | Institutionnelle |
| ODHMOD (AIHA) | Gratuit | Oui (Euler, 5 000 pas) | Non (débit imposé) | Non | Champ proche / lointain | Non | Article à comité de lecture |
| DNV PHAST, Gexcon EFFECTS | €€€€ | Oui | Oui | Oui | Partiel | Non | Dossiers éditeur |
| CFD (FDS, Fluent, FLACS) | €€€€ + expert | Oui | Terme source | Oui | 3D | Non | Au cas par cas |

Le logiciel se situe entre les calculateurs simplifiés et les outils de conséquences ou de CFD. C'est un outil de criblage, qui indique lui-même quand une étude plus poussée est nécessaire.
