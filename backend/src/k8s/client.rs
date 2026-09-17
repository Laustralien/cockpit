//! Parler a l'API d'un cluster, en HTTPS, sans kubectl.
//!
//! **AUCUNE ATTENTE SANS BORNE.** Un cluster injoignable ne doit pas figer l'ecran : les appels
//! ordinaires ont un delai, et le flux est borne PAR LE SERVEUR (`timeoutSeconds`), qui le
//! referme proprement. Poser un delai de lecture sur le flux serait une erreur : un namespace ou
//! rien ne bouge n'envoie rien pendant des minutes, et c'est normal.

use super::kubeconfig::{Acces, Identite};
use serde_json::Value;
use std::time::Duration;

/// Un appel ordinaire. Au-dela, on rend une erreur qui nomme le cluster.
const DELAI: Duration = Duration::from_secs(20);
/// Combien de temps le serveur garde un flux ouvert avant de le refermer lui-meme. On relance
/// derriere : c'est ce que fait tout client Kubernetes, et ca evite une connexion eternelle.
pub const DUREE_DU_FLUX: u64 = 280;

pub struct Client {
    http: reqwest::Client,
    flux: reqwest::Client,
    serveur: String,
    jeton: Option<String>,
}

impl Client {
    pub fn depuis(acces: &Acces) -> Result<Self, String> {
        let construire = |delai: Option<Duration>| -> Result<reqwest::Client, String> {
            let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
            if let Some(d) = delai {
                b = b.timeout(d);
            }
            if let Some(pem) = &acces.autorite {
                let cert = reqwest::Certificate::from_pem(pem)
                    .map_err(|e| format!("l'autorite de certification du cluster est illisible : {e}"))?;
                b = b.add_root_certificate(cert);
            }
            if acces.sans_verification {
                // Le kubeconfig le demande explicitement. On obeit, et l'interface le dit.
                b = b.danger_accept_invalid_certs(true);
            }
            if let Identite::Certificat { pem } = &acces.identite {
                let identite = reqwest::Identity::from_pem(pem)
                    .map_err(|e| format!("le certificat client est illisible : {e}"))?;
                b = b.identity(identite);
            }
            b.build().map_err(|e| format!("client HTTPS impossible a construire : {e}"))
        };
        Ok(Self {
            http: construire(Some(DELAI))?,
            flux: construire(None)?,
            serveur: acces.serveur.clone(),
            jeton: match &acces.identite {
                Identite::Jeton(j) => Some(j.clone()),
                Identite::Certificat { .. } => None,
            },
        })
    }

    fn requete(&self, client: &reqwest::Client, chemin: &str) -> reqwest::RequestBuilder {
        let r = client.get(format!("{}{chemin}", self.serveur));
        match &self.jeton {
            Some(j) => r.bearer_auth(j),
            None => r,
        }
    }

    pub async fn json(&self, chemin: &str) -> Result<Value, String> {
        let reponse = self
            .requete(&self.http, chemin)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("cluster injoignable : {e}"))?;
        lire_json(reponse).await
    }

    pub async fn poster(&self, chemin: &str, corps: Value) -> Result<Value, String> {
        let r = self.http.post(format!("{}{chemin}", self.serveur)).json(&corps);
        let r = match &self.jeton {
            Some(j) => r.bearer_auth(j),
            None => r,
        };
        let reponse = r.send().await.map_err(|e| format!("cluster injoignable : {e}"))?;
        lire_json(reponse).await
    }

    /// Du texte brut : les logs, le YAML d'un objet.
    pub async fn texte(&self, chemin: &str, accept: &str) -> Result<String, String> {
        let reponse = self
            .requete(&self.http, chemin)
            .header("Accept", accept)
            .send()
            .await
            .map_err(|e| format!("cluster injoignable : {e}"))?;
        let statut = reponse.status();
        let corps = reponse.text().await.unwrap_or_default();
        if !statut.is_success() {
            return Err(message_d_erreur(statut, &corps));
        }
        Ok(corps)
    }

    /// Ouvre un flux et rend la reponse : l'appelant lit les lignes au fur et a mesure.
    pub async fn ouvrir_le_flux(&self, chemin: &str) -> Result<reqwest::Response, String> {
        let reponse = self
            .requete(&self.flux, chemin)
            .send()
            .await
            .map_err(|e| format!("flux impossible a ouvrir : {e}"))?;
        let statut = reponse.status();
        if !statut.is_success() {
            let corps = reponse.text().await.unwrap_or_default();
            return Err(message_d_erreur(statut, &corps));
        }
        Ok(reponse)
    }
}

async fn lire_json(reponse: reqwest::Response) -> Result<Value, String> {
    let statut = reponse.status();
    let corps = reponse.text().await.map_err(|e| format!("reponse illisible : {e}"))?;
    if !statut.is_success() {
        return Err(message_d_erreur(statut, &corps));
    }
    serde_json::from_str(&corps).map_err(|e| format!("reponse JSON illisible : {e}"))
}

/// Le message du cluster, pas le notre.
///
/// **UN 403 DE KUBERNETES DIT EXACTEMENT CE QUI MANQUE** (« cannot list resource pods in API
/// group … at the cluster scope »). Le remplacer par « acces refuse » ferait chercher pendant
/// une heure un droit qu'on aurait pu lire. On garde donc SON texte, en le bornant : un corps
/// d'erreur peut etre long, et il finit dans une infobulle.
pub fn message_d_erreur(statut: reqwest::StatusCode, corps: &str) -> String {
    let detail = serde_json::from_str::<Value>(corps)
        .ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| corps.chars().take(300).collect());
    if detail.trim().is_empty() {
        return format!("le cluster a repondu {statut}");
    }
    format!("{statut} : {}", detail.chars().take(300).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn l_erreur_rendue_est_celle_du_cluster() {
        let corps = json!({
            "kind": "Status", "status": "Failure",
            "message": "pods.metrics.k8s.io is forbidden: User \"u-2s\" cannot list resource"
        })
        .to_string();
        let m = message_d_erreur(reqwest::StatusCode::FORBIDDEN, &corps);
        assert!(m.contains("cannot list resource"), "le texte du cluster doit survivre : {m}");
        assert!(m.contains("403"));
    }

    #[test]
    fn une_erreur_sans_message_reste_lisible() {
        let m = message_d_erreur(reqwest::StatusCode::BAD_GATEWAY, "");
        assert!(m.contains("502"), "{m}");
    }

    #[test]
    fn un_corps_immense_ne_part_pas_dans_une_infobulle() {
        let m = message_d_erreur(reqwest::StatusCode::INTERNAL_SERVER_ERROR, &"x".repeat(10_000));
        assert!(m.len() < 400, "borne a la lecture, pas a l'affichage : {} caracteres", m.len());
    }
}
