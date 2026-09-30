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
const CLE_EMAIL: &str = "jira_email";
const CLE_TYPES: &str = "jira_types_branche";

#[derive(Debug, Serialize)]
pub struct ConfigJira {
    pub url: String,
    /// Le jeton est pose. **Jamais le jeton lui-meme.**
    pub jeton_pose: bool,
    /// L'e-mail n'est pas un secret (contrairement au jeton) : il peut remonter au front.
    pub email: String,
    pub types_branche: Correspondance,
}

/// Vrai si l'hote de l'URL est un domaine `*.atlassian.net` : c'est Jira Cloud, qui demande
/// une authentification differente (e-mail + jeton) de Server/Data Center (jeton seul).
///
/// **ANALYSE PAR UN VRAI PARSEUR D'URL, PAS UN DECOUPAGE MAISON** : un decoupage sur `://`
/// puis `/`/`:` se laisse piegier par `?.atlassian.net` (requete) ou `#.atlassian.net`
/// (fragment) pris pour l'hote, et rate `user:pass@hote` (les identifiants avant `@`).
/// `Url::parse` + `host_str()` isole l'hote correctement dans tous ces cas.
pub fn est_cloud(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_lowercase))
        .is_some_and(|hote| hote.ends_with(".atlassian.net"))
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
    // **LE JETON NE PASSE PAS EN CLAIR SUR LE RESEAU.** Il part a chaque requete : en http,
    // n'importe qui sur le chemin le lirait. Seule la machine elle-meme est admise en http.
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| url == *p || url.starts_with(&format!("{p}:")) || url.starts_with(&format!("{p}/")));
    if !url.starts_with("https://") && !local {
        return Err(format!(
            "adresse Jira refusee : https:// est obligatoire, le jeton ne doit pas circuler en clair ({url})"
        ));
    }
    Ok(url.to_string())
}

pub fn url(db: &Database) -> Option<String> {
    db.get_setting(CLE_URL).filter(|v| !v.trim().is_empty())
}

pub fn jeton(db: &Database) -> Option<String> {
    db.get_setting(CLE_JETON).filter(|v| !v.trim().is_empty())
}

pub fn email(db: &Database) -> Option<String> {
    db.get_setting(CLE_EMAIL).filter(|v| !v.trim().is_empty())
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
        email: email(db).unwrap_or_default(),
        types_branche: types_branche(db),
    }
}

/// Un jeton absent ou vide GARDE le precedent : le champ du jeton s'affiche vide une fois pose.
///
/// L'e-mail n'obeit pas a la meme regle : **`Some` (meme vide) remplace toujours l'e-mail**,
/// `None` le garde. C'est ce qui permet a l'interface d'effacer l'e-mail en envoyant une
/// chaine vide, la ou un jeton vide ne peut que vouloir dire « inchange ».
pub fn poser(
    db: &Database,
    url: &str,
    jeton: Option<&str>,
    email: Option<&str>,
    types: Option<Correspondance>,
) -> Result<ConfigJira, String> {
    db.set_setting(CLE_URL, &normaliser_url(url)?)?;
    if let Some(j) = jeton.map(str::trim).filter(|j| !j.is_empty()) {
        db.set_setting(CLE_JETON, j)?;
    }
    if let Some(e) = email {
        db.set_setting(CLE_EMAIL, e.trim())?;
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
    fn detecte_jira_cloud_a_l_hote() {
        assert!(est_cloud("https://exemple.atlassian.net"));
        assert!(est_cloud("https://exemple.atlassian.net/"));
        assert!(!est_cloud("https://jira.exemple.org"));
        assert!(
            !est_cloud("https://atlassian.net.exemple.org"),
            "piege : l'hote ne se termine pas par .atlassian.net"
        );
    }

    #[test]
    fn detecte_jira_cloud_sans_se_faire_piegier_par_la_requete_ou_le_fragment() {
        assert!(
            !est_cloud("https://evil.org?.atlassian.net"),
            "l'hote est evil.org, .atlassian.net n'est que la requete"
        );
        assert!(
            !est_cloud("https://evil.org#.atlassian.net"),
            "l'hote est evil.org, .atlassian.net n'est que le fragment"
        );
        assert!(est_cloud("https://u:p@x.atlassian.net"), "les identifiants ne changent pas l'hote");
        assert!(est_cloud("https://X.ATLASSIAN.NET"), "la casse de l'hote ne compte pas");
    }

    #[test]
    fn normalise_l_adresse() {
        assert_eq!(normaliser_url(" https://jira.exemple.org/ ").unwrap(), "https://jira.exemple.org");
        assert_eq!(normaliser_url("").unwrap(), "");
        assert!(normaliser_url("jira.exemple.org").is_err());
        assert!(normaliser_url("http://jira.exemple.org").is_err(), "le jeton passerait en clair");
        assert!(normaliser_url("http://localhost:8080").is_ok());
        assert!(normaliser_url("http://localhost.exemple.org").is_err());
    }

    #[test]
    fn le_jeton_n_est_jamais_rendu() {
        let db = base();
        let c = poser(&db, "https://jira.exemple.org/", Some(" secret "), None, None).unwrap();
        assert!(c.jeton_pose);
        assert_eq!(c.url, "https://jira.exemple.org");
        assert_eq!(jeton(&db).unwrap(), "secret");
        assert!(!serde_json::to_string(&c).unwrap().contains("secret"));
    }

    #[test]
    fn un_jeton_vide_garde_l_ancien() {
        let db = base();
        poser(&db, "https://j.org", Some("secret"), None, None).unwrap();
        poser(&db, "https://j.org", Some("  "), None, None).unwrap();
        poser(&db, "https://j.org", None, None, None).unwrap();
        assert_eq!(jeton(&db).unwrap(), "secret");
    }

    #[test]
    fn un_e_mail_pose_remplace_toujours_meme_vide_none_le_garde() {
        let db = base();
        assert_eq!(poser(&db, "https://j.org", None, None, None).unwrap().email, "");
        let c = poser(&db, "https://j.org", None, Some("prenom.nom@exemple.org"), None).unwrap();
        assert_eq!(c.email, "prenom.nom@exemple.org");
        let c = poser(&db, "https://j.org", None, None, None).unwrap();
        assert_eq!(c.email, "prenom.nom@exemple.org", "None garde l'e-mail precedent");
        let c = poser(&db, "https://j.org", None, Some(""), None).unwrap();
        assert_eq!(c.email, "", "Some vide efface l'e-mail, contrairement au jeton");
    }

    #[test]
    fn les_types_par_defaut_puis_poses() {
        let db = base();
        assert_eq!(types_branche(&db), correspondance_par_defaut());
        let t = HashMap::from([("Bogue".to_string(), "fix".to_string()), ("*".to_string(), "feat".to_string())]);
        poser(&db, "https://j.org", None, None, Some(t.clone())).unwrap();
        assert_eq!(types_branche(&db), t);
    }

    #[test]
    fn la_liaison_decoupe_les_cles_et_prend_le_gabarit_par_defaut() {
        let db = base();
        let l = poser_liaison(&db, "site", "proj, ABC", "").unwrap();
        assert_eq!(l.cles, vec!["PROJ", "ABC"]);
        assert_eq!(l.gabarit, GABARIT_PAR_DEFAUT);
        assert_eq!(liaisons(&db).unwrap(), vec![l]);
    }

    #[test]
    fn la_liaison_refuse_une_cle_ou_un_gabarit_invalide() {
        let db = base();
        assert!(poser_liaison(&db, "site", "PROJ-1", "").is_err());
        assert!(poser_liaison(&db, "site", "PROJ", "{type}/{slug}").is_err(), "sans {{cle}}, deux tickets auraient la meme branche");
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
