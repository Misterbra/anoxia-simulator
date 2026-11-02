# Journal des modifications

## 2.1.0 — 2026-11-02 — première version publiée

### Modèle
- Débit de fuite isentropique (sonique et subsonique, coefficient de décharge 0,62 / 0,95 / 1,0), vidange réelle du récipient (isotherme par défaut, adiabatique indicative), gaz réel par table NIST REFPROP jusqu'à 750 bar, CO₂ liquéfié (Span–Wagner) y compris bouteille pleine de liquide.
- Quatre scénarios : fuite par orifice, ouverture totale ou arrachement, rejet instantané (méthode par déplacement INRS/BCGA), débit constant imposé avec aide de conversion pour les liquides cryogéniques.
- Local bien mélangé intégré exactement à chaque pas ; ventilation en extraction (air neuf = débit extrait − débit de gaz) ou en soufflage, efficacité appliquée au flux sortant total, infiltrations combinées en quadrature.
- Modèle bi-zone indicatif pour les gaz lourds et légers ; le classement retient la pire des zones et de la moyenne et indique la série qui fixe le minimum.
- Champ proche (demi-sphère autour de la fuite, modèle Nicas / AIHA IH Mod / ODHMOD), affiché à part et signalé s'il est plus sévère ; retenu pour le classement sur option.
- Instants de franchissement localisés sur la solution exacte du pas ; durées encore en cours à la fin du calcul marquées « ≥ ».
- Chronologie d'événements : arrêt ou démarrage d'une ventilation mécanique à un débit donné, isolement de la fuite, à tout instant après le début du rejet ; le pas s'arrête exactement sur chaque événement, l'état reste continu, les événements simultanés sont appliqués dans un ordre fixe ; marqueurs sur les courbes à l'écran et dans le rapport.

### Évaluation
- Classement par seuils d'alarme (France – INRS 19 / 17 %, OSHA 19,5 % et second niveau usuel 16 %, personnalisé) et par concentration de CO₂ (valeurs de la VLEP, de la VLCT et de la DIVS, comparées au pic).
- Pression partielle d'oxygène (minimale et ambiante à la pression du local) ; avertissement en altitude.
- Messages clés en clair, avertissements hiérarchisés, recommandations proposées par le logiciel et validées par le rédacteur, contrôles indépendants (déplacement INRS/BCGA, efflux libre NFPA 2001, calcul manuel).

### Analyses
- Comparaison de scénarios, balayage d'un paramètre, sensibilité (tornado), panne de ventilation.
- Étude avant / après installation : installation actuelle et prévue, chacune en fonctionnement normal et en panne de ventilation.
- Dimensionnement du débit de ventilation nécessaire (mode extraction ou soufflage explicite, calcul prolongé jusqu'à couvrir l'événement, résultat « non démontré » sinon) et du diamètre de fuite maximal tolérable.

### Rapport et fichiers
- Rapport HTML autonome bilingue : synthèse en clair, graphiques, données d'entrée, seuils, dimensionnement, panne de ventilation, étude avant / après, recommandations retenues, repères physiologiques, hypothèses et limites, bloc de validation.
- Export CSV des séries, scénarios enregistrés en JSON versionné (anciens fichiers convertis), préférences mémorisées.

### Validation
- Dossier de validation intégré de 59 cas (NIST, Linde, TNO, Crowl & Louvar, INRS, BCGA, York, Nottingham, Fermilab, NFPA 2001, NRC Canada, calorimètre, MiniCLEAN, Factory Mutual, ODHMOD, chronologie), exécuté par les tests et consultable dans l'application : 55 cas contrôlés conformes, 4 cas documentés.

### Prise en main
- Tutoriel interactif, six scénarios d'exemple, guides utilisateur illustrés en français et en anglais, interface bilingue, thème clair ou sombre.

### Licence
- Logiciel libre sous GNU AGPL v3 ou ultérieure ; licence commerciale sur demande ; fichier de citation (CITATION.cff).
