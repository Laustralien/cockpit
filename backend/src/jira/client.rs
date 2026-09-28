//! Le dialogue HTTP avec Jira : un client partage, le jeton en `Bearer`, des refus lisibles.

use super::config;
use crate::storage::db::Database;
use std::sync::OnceLock;
use std::time::Duration;

/// Au-dela, on rend la main : mieux vaut « Jira ne repond pas » qu'un bouton qui tourne.
const DELAI: Duration = Duration::from_secs(20);

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(DELAI)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

pub struct Jira {
    base: String,
    jeton: String,
    email: String,
    cloud: bool,
}

/// La forme d'authentification a envoyer : Cloud veut du Basic (e-mail + jeton), Server/DC
/// du Bearer (jeton seul). Extrait en fonction pure pour la tester sans requete HTTP.
#[derive(PartialEq)]
pub(crate) enum Authentification {
    Basic { email: String, jeton: String },
    Bearer(String),
}

/// **LE JETON NE PARAIT JAMAIS EN CLAIR, MEME DANS UN LOG DE DEBUG** : un `#[derive(Debug)]`
/// l'aurait imprime tel quel a la moindre `dbg!`/`log::debug!` sur cette valeur.
impl std::fmt::Debug for Authentification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Authentification::Basic { email, .. } => {
                f.debug_struct("Basic").field("email", email).field("jeton", &"***").finish()
            }
            Authentification::Bearer(_) => f.debug_tuple("Bearer").field(&"***").finish(),
        }
    }
}

/// Basic seulement sur Cloud ET avec un e-mail renseigne ; Bearer sinon, y compris sur
/// Server/DC ou un e-mail trainerait dans les reglages (bascule Cloud -> Server/DC sans
/// l'effacer) : Server/DC ne sait pas faire de Basic, l'e-mail n'y change donc rien.
pub(crate) fn authentification(cloud: bool, email: &str, jeton: &str) -> Authentification {
    if cloud && !email.is_empty() {
        Authentification::Basic { email: email.to_string(), jeton: jeton.to_string() }
    } else {
        Authentification::Bearer(jeton.to_string())
    }
}

impl Jira {
    pub fn depuis(db: &Database) -> Result<Self, String> {
        let base = config::url(db).ok_or("Jira n'est pas configure : renseigne son adresse dans les reglages")?;
        let jeton = config::jeton(db).ok_or("Jira n'est pas configure : pose un jeton d'acces personnel dans les reglages")?;
        let email = config::email(db).unwrap_or_default();
        let cloud = config::est_cloud(&base);
        if cloud && email.is_empty() {
            return Err(
                "Jira Cloud demande l'e-mail du compte en plus du jeton d'API : renseigne-le dans les reglages"
                    .to_string(),
            );
        }
        Ok(Self { base, jeton, email, cloud })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Vrai si l'instance est Jira Cloud (voir `config::est_cloud`) : ce qui change la
    /// recherche (`/rest/api/3/search/jql`) et la forme de l'assignation (`accountId`).
    pub fn cloud(&self) -> bool {
        self.cloud
    }

    /// Rend le corps de la reponse en texte ; tout statut hors 2xx devient un message lisible.
    pub async fn envoyer(
        &self,
        methode: reqwest::Method,
        chemin: &str,
        requete: &[(&str, &str)],
        corps: Option<serde_json::Value>,
    ) -> Result<String, String> {
        let mut demande = client()
            .request(methode, format!("{}{chemin}", self.base))
            .header(reqwest::header::ACCEPT, "application/json");
        demande = match authentification(self.cloud, &self.email, &self.jeton) {
            Authentification::Basic { email, jeton } => demande.basic_auth(email, Some(jeton)),
            Authentification::Bearer(jeton) => demande.bearer_auth(jeton),
        };
        if !requete.is_empty() {
            demande = demande.query(requete);
        }
        if let Some(corps) = corps {
            demande = demande.json(&corps);
        }
        let reponse = demande.send().await.map_err(panne_reseau)?;
        let code = reponse.status().as_u16();
        let texte = reponse.text().await.map_err(panne_reseau)?;
        if !(200..300).contains(&code) {
            log::warn!("jira : {chemin} a repondu {code} — {texte}");
            return Err(motif_du_refus(code, &texte));
        }
        Ok(texte)
    }
}

#[derive(serde::Deserialize, Default)]
struct RefusJira {
    #[serde(default, rename = "errorMessages")]
    messages: Vec<String>,
    #[serde(default)]
    errors: std::collections::BTreeMap<String, String>,
}

/// Jira detaille ses refus dans `errorMessages` et `errors` : c'est ce qui dit QUEL champ
/// manque a une creation, et c'est ce qu'on montre.
pub fn motif_du_refus(code: u16, corps: &str) -> String {
    let refus: RefusJira = serde_json::from_str(corps).unwrap_or_default();
    let mut details = refus.messages;
    details.extend(refus.errors.into_iter().map(|(champ, m)| format!("{champ} : {m}")));
    let detail = details.join(" ; ");
    let motif = match code {
        401 => "jeton Jira invalide ou expire".to_string(),
        403 => "droits Jira insuffisants".to_string(),
        404 => "ticket ou projet Jira introuvable".to_string(),
        400 => "requete refusee par Jira".to_string(),
        _ => format!("Jira a repondu {code}"),
    };
    if detail.is_empty() { motif } else { format!("{motif} : {detail}") }
}

fn panne_reseau(e: reqwest::Error) -> String {
    log::warn!("jira : injoignable — {e}");
    if e.is_timeout() {
        "Jira ne repond pas".to_string()
    } else {
        format!("Jira est injoignable : {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduit_les_refus() {
        assert!(motif_du_refus(401, "").contains("jeton Jira invalide"));
        assert!(motif_du_refus(403, "").contains("droits"));
        assert!(motif_du_refus(404, "").contains("introuvable"));
        assert!(motif_du_refus(503, "").contains("503"));
    }

    #[test]
    fn remonte_les_messages_de_jira() {
        let corps = r#"{"errorMessages":["Transition invalide"],"errors":{"summary":"Le resume est obligatoire"}}"#;
        let m = motif_du_refus(400, corps);
        assert!(m.contains("Transition invalide"), "{m}");
        assert!(m.contains("summary : Le resume est obligatoire"), "{m}");
    }

    #[test]
    fn sans_configuration_le_message_le_dit() {
        let db = Database::new(":memory:").unwrap();
        let e = Jira::depuis(&db).err().unwrap();
        assert!(e.contains("pas configure"), "{e}");
    }

    #[test]
    fn choisit_basic_seulement_sur_cloud_avec_un_e_mail() {
        assert_eq!(
            authentification(true, "a@exemple.org", "tok"),
            Authentification::Basic { email: "a@exemple.org".to_string(), jeton: "tok".to_string() }
        );
        assert_eq!(
            authentification(true, "", "tok"),
            Authentification::Bearer("tok".to_string()),
            "Cloud sans e-mail retombe sur Bearer"
        );
    }

    #[test]
    fn server_dc_reste_en_bearer_meme_avec_un_e_mail_enregistre() {
        // Un e-mail peut trainer dans les reglages (bascule Cloud -> Server/DC sans l'effacer) :
        // Server/DC ignore l'e-mail et reste en Bearer, jamais en Basic.
        assert_eq!(
            authentification(false, "a@exemple.org", "tok"),
            Authentification::Bearer("tok".to_string())
        );
    }

    #[test]
    fn le_jeton_ne_parait_jamais_dans_le_debug_de_l_authentification() {
        let basic = authentification(true, "a@exemple.org", "secret-token");
        assert!(!format!("{basic:?}").contains("secret-token"), "{basic:?}");
        let bearer = authentification(false, "", "secret-token");
        assert!(!format!("{bearer:?}").contains("secret-token"), "{bearer:?}");
    }

    #[test]
    fn jira_cloud_sans_e_mail_est_un_refus_lisible() {
        let db = Database::new(":memory:").unwrap();
        config::poser(&db, "https://x.atlassian.net", Some("tok"), None, None).unwrap();
        let e = Jira::depuis(&db).err().unwrap();
        assert!(e.contains("Jira Cloud demande l'e-mail"), "{e}");
    }

    #[test]
    fn jira_cloud_avec_e_mail_est_accepte_et_se_sait_cloud() {
        let db = Database::new(":memory:").unwrap();
        config::poser(&db, "https://x.atlassian.net", Some("tok"), Some("a@exemple.org"), None).unwrap();
        let jira = Jira::depuis(&db).unwrap();
        assert!(jira.cloud());
    }

    #[test]
    fn jira_server_n_est_pas_cloud() {
        let db = Database::new(":memory:").unwrap();
        config::poser(&db, "https://jira.exemple.org", Some("tok"), None, None).unwrap();
        let jira = Jira::depuis(&db).unwrap();
        assert!(!jira.cloud());
    }
}
