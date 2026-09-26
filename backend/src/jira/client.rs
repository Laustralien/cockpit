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
}

impl Jira {
    pub fn depuis(db: &Database) -> Result<Self, String> {
        let base = config::url(db).ok_or("Jira n'est pas configure : renseigne son adresse dans les reglages")?;
        let jeton = config::jeton(db).ok_or("Jira n'est pas configure : pose un jeton d'acces personnel dans les reglages")?;
        Ok(Self { base, jeton })
    }

    pub fn base(&self) -> &str {
        &self.base
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
            .bearer_auth(&self.jeton)
            .header(reqwest::header::ACCEPT, "application/json");
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
}
