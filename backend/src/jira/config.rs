//! Les reglages Jira de cette machine et la liaison des projets.
//!
//! **LE JETON EST RANGE DANS `settings`, EN CLAIR**, comme les cles d'IA et le jeton du compte
//! (voir l'en-tete de `compte/mod.rs`). Cette table n'est pas synchronisee.

use super::branche::{correspondance_par_defaut, Correspondance, GABARIT_PAR_DEFAUT};
use super::jql::{cle_de_projet_valide, decouper_cles};
use crate::storage::db::Database;
use serde::Serialize;
use std::collections::HashMap;

const CLE_URL: &str = "jira_url";
pub const CLE_JETON: &str = "jira_jeton";
const CLE_TYPES: &str = "jira_types_branche";

#[derive(Debug, Serialize)]
pub struct ConfigJira {
    pub url: String,
    /// Le jeton est pose. **Jamais le jeton lui-meme.**
    pub jeton_pose: bool,
    pub types_branche: Correspondance,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiaisonJira {
    pub projet: String,
    pub cles: Vec<String>,
    pub gabarit: String,
}

/// Une adresse vide est permise : c'est ainsi qu'on deconnecte Jira.
pub fn normaliser_url(brut: &str) -> Result<String, String> {
    let url = brut.trim().trim_end_matches('/');
    if url.is_empty() {
        return Ok(String::new());
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("adresse Jira invalide (http:// ou https:// attendu) : {url}"));
    }
    Ok(url.to_string())
}

pub fn url(db: &Database) -> Option<String> {
    db.get_setting(CLE_URL).filter(|v| !v.trim().is_empty())
}

pub fn jeton(db: &Database) -> Option<String> {
    db.get_setting(CLE_JETON).filter(|v| !v.trim().is_empty())
}

pub fn types_branche(db: &Database) -> Correspondance {
    db.get_setting(CLE_TYPES)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_else(correspondance_par_defaut)
}

pub fn lire(db: &Database) -> ConfigJira {
    ConfigJira {
        url: url(db).unwrap_or_default(),
        jeton_pose: jeton(db).is_some(),
        types_branche: types_branche(db),
    }
}

/// Un jeton absent ou vide GARDE le precedent : le champ du jeton s'affiche vide une fois pose.
pub fn poser(db: &Database, url: &str, jeton: Option<&str>, types: Option<Correspondance>) -> Result<ConfigJira, String> {
    db.set_setting(CLE_URL, &normaliser_url(url)?)?;
    if let Some(j) = jeton.map(str::trim).filter(|j| !j.is_empty()) {
        db.set_setting(CLE_JETON, j)?;
    }
    if let Some(t) = types {
        db.set_setting(CLE_TYPES, &serde_json::to_string(&t).map_err(|e| e.to_string())?)?;
    }
    Ok(lire(db))
}

fn construire(projet: &str, cles: Option<String>, gabarit: Option<String>) -> LiaisonJira {
    LiaisonJira {
        projet: projet.to_string(),
        cles: decouper_cles(&cles.unwrap_or_default()),
        gabarit: gabarit
            .filter(|g| !g.trim().is_empty())
            .unwrap_or_else(|| GABARIT_PAR_DEFAUT.to_string()),
    }
}

pub fn liaison(db: &Database, projet: &str) -> Result<LiaisonJira, String> {
    let (cles, gabarit) = db.get_project_jira(projet)?;
    Ok(construire(projet, cles, gabarit))
}

pub fn liaisons(db: &Database) -> Result<Vec<LiaisonJira>, String> {
    Ok(db
        .list_project_jira()?
        .into_iter()
        .map(|(p, c, g)| construire(&p, Some(c), g))
        .filter(|l| !l.cles.is_empty())
        .collect())
}

/// **LE GABARIT DOIT CONTENIR `{cle}`** : sans elle, deux tickets partageraient une branche.
pub fn poser_liaison(db: &Database, projet: &str, cles: &str, gabarit: &str) -> Result<LiaisonJira, String> {
    let liste = decouper_cles(cles);
    if let Some(mauvaise) = liste.iter().find(|c| !cle_de_projet_valide(c)) {
        return Err(format!("cle de projet Jira invalide : {mauvaise}"));
    }
    let gabarit = gabarit.trim();
    if !gabarit.is_empty() && !gabarit.contains("{cle}") {
        return Err("le gabarit de branche doit contenir {cle}".to_string());
    }
    let cles = (!liste.is_empty()).then(|| liste.join(","));
    let gabarit = (!gabarit.is_empty()).then_some(gabarit);
    db.set_project_jira(projet, cles.as_deref(), gabarit)?;
    liaison(db, projet)
}

/// Retire le jeton des reglages envoyes tels quels a l'interface (`get_app_settings`).
pub fn masquer(reglages: &mut HashMap<String, String>) {
    reglages.remove(CLE_JETON);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Database {
        let db = Database::new(":memory:").unwrap();
        db.create_project("site", "/tmp/site", "", "", &[]).unwrap();
        db
    }

    #[test]
    fn normalise_l_adresse() {
        assert_eq!(normaliser_url(" https://jira.exemple.org/ ").unwrap(), "https://jira.exemple.org");
        assert_eq!(normaliser_url("").unwrap(), "");
        assert!(normaliser_url("jira.exemple.org").is_err());
    }

    #[test]
    fn le_jeton_n_est_jamais_rendu() {
        let db = base();
        let c = poser(&db, "https://jira.exemple.org/", Some(" secret "), None).unwrap();
        assert!(c.jeton_pose);
        assert_eq!(c.url, "https://jira.exemple.org");
        assert_eq!(jeton(&db).unwrap(), "secret");
        assert!(!serde_json::to_string(&c).unwrap().contains("secret"));
    }

    #[test]
    fn un_jeton_vide_garde_l_ancien() {
        let db = base();
        poser(&db, "https://j.org", Some("secret"), None).unwrap();
        poser(&db, "https://j.org", Some("  "), None).unwrap();
        poser(&db, "https://j.org", None, None).unwrap();
        assert_eq!(jeton(&db).unwrap(), "secret");
    }

    #[test]
    fn les_types_par_defaut_puis_poses() {
        let db = base();
        assert_eq!(types_branche(&db), correspondance_par_defaut());
        let t = HashMap::from([("Bogue".to_string(), "fix".to_string()), ("*".to_string(), "feat".to_string())]);
        poser(&db, "https://j.org", None, Some(t.clone())).unwrap();
        assert_eq!(types_branche(&db), t);
    }

    #[test]
    fn la_liaison_decoupe_les_cles_et_prend_le_gabarit_par_defaut() {
        let db = base();
        let l = poser_liaison(&db, "site", "ccm, ABC", "").unwrap();
        assert_eq!(l.cles, vec!["CCM", "ABC"]);
        assert_eq!(l.gabarit, GABARIT_PAR_DEFAUT);
        assert_eq!(liaisons(&db).unwrap(), vec![l]);
    }

    #[test]
    fn la_liaison_refuse_une_cle_ou_un_gabarit_invalide() {
        let db = base();
        assert!(poser_liaison(&db, "site", "CCM-1", "").is_err());
        assert!(poser_liaison(&db, "site", "CCM", "{type}/{slug}").is_err(), "sans {{cle}}, deux tickets auraient la meme branche");
    }

    #[test]
    fn un_projet_sans_cle_n_est_pas_lie() {
        let db = base();
        poser_liaison(&db, "site", "", "").unwrap();
        assert!(liaisons(&db).unwrap().is_empty());
    }

    #[test]
    fn masque_le_jeton_des_reglages_generaux() {
        let mut r = HashMap::from([(CLE_JETON.to_string(), "secret".to_string()), ("jira_url".to_string(), "u".to_string())]);
        masquer(&mut r);
        assert!(!r.contains_key(CLE_JETON));
        assert!(r.contains_key("jira_url"));
    }
}
