//! Ce que Cockpit mesure en continu : ce que l'utilisateur a DEMANDE, et rien d'autre.
//!
//! **RIEN N'EST SURVEILLE PAR DEFAUT.** Le cluster ne garde aucun historique, donc une courbe
//! sur une heure suppose que quelqu'un ait mesure pendant cette heure. Faire ca dans le dos de
//! l'utilisateur, pour tous ses projets, serait exactement le genre de travail de fond que ce
//! projet refuse : une question au cluster toutes les N secondes se paie MULTIPLIEE par le
//! nombre de cibles. On lui laisse donc declarer ce qu'il veut suivre, un namespace a la fois.
//!
//! **UNE SEULE BOUCLE, ET ELLE NE TOURNE QUE S'IL Y A QUELQUE CHOSE A FAIRE.** Sans cible
//! active, aucune tache ne vit. La boucle relit les cibles a chaque tour : un reglage change
//! prend effet tout de suite, sans redemarrage et sans machinerie de notification.

use crate::storage::Database;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// La cle du reglage. Local, comme tous les reglages : il ne voyage pas entre machines.
pub const CLE_CIBLES: &str = "k8s.surveillance";
pub const CLE_RETENTION: &str = "k8s.retention_heures";

/// Ce qu'on garde par defaut, quand l'utilisateur n'a rien dit.
pub const RETENTION_PAR_DEFAUT: u32 = 24;
/// Le rythme le plus rapide qu'on accepte : en dessous, le serveur de mesures du cluster n'a
/// rien de neuf a dire, et on paierait la question pour rien.
pub const PERIODE_MINIMUM: u32 = 15;

/// Un namespace suivi, avec son rythme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cible {
    pub contexte: String,
    pub namespace: String,
    /// Secondes entre deux mesures.
    pub periode: u32,
    pub actif: bool,
}

/// Les reglages tels que l'interface les manipule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reglages {
    pub cibles: Vec<Cible>,
    /// Combien d'heures on garde. Au-dela, les points sont jetes.
    pub retention_heures: u32,
}

impl Default for Reglages {
    fn default() -> Self {
        Self { cibles: Vec::new(), retention_heures: RETENTION_PAR_DEFAUT }
    }
}

/// Lit les reglages. Un contenu illisible ne bloque pas l'ecran : on repart de rien plutot que
/// de refuser d'ouvrir, et l'utilisateur redeclare ce qu'il veut.
pub fn lire(db: &Database) -> Reglages {
    let brut = db.get_setting(CLE_CIBLES).unwrap_or_default();
    let cibles: Vec<Cible> = serde_json::from_str(&brut).unwrap_or_default();
    let retention = db
        .get_setting(CLE_RETENTION)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(RETENTION_PAR_DEFAUT)
        .clamp(1, 24 * 30);
    Reglages { cibles, retention_heures: retention }
}

/// Ecrit les reglages, en bornant ce qui doit l'etre.
///
/// **LE RYTHME SE BORNE ICI, PAS DANS L'INTERFACE.** Une valeur venue du client ne se croit
/// jamais : c'est la meme regle que pour les noms de namespace qui finissent dans une URL.
pub fn ecrire(db: &Database, mut reglages: Reglages) -> Result<Reglages, String> {
    for cible in &mut reglages.cibles {
        cible.periode = cible.periode.max(PERIODE_MINIMUM);
        if !super::kubeconfig::nom_valide(&cible.namespace) {
            return Err(format!("nom de namespace refuse : {}", cible.namespace));
        }
    }
    reglages.retention_heures = reglages.retention_heures.clamp(1, 24 * 30);
    let json = serde_json::to_string(&reglages.cibles).map_err(|e| e.to_string())?;
    db.set_setting(CLE_CIBLES, &json)?;
    db.set_setting(CLE_RETENTION, &reglages.retention_heures.to_string())?;
    Ok(reglages)
}

/// La boucle de mesure, et de quoi l'arreter.
#[derive(Default)]
pub struct Surveillance {
    arret: std::sync::Mutex<Option<Arc<AtomicBool>>>,
}

impl Surveillance {
    /// Met la boucle en accord avec les reglages : elle tourne s'il y a au moins une cible
    /// active, elle s'arrete sinon.
    ///
    /// **ELLE OUVRE SA PROPRE CONNEXION A LA BASE**, a partir du chemin. Partager celle de
    /// l'interface ferait attendre un clic derriere une ecriture de mesures : la regle du
    /// projet est que la boucle graphique n'attend jamais un verrou. SQLite est en WAL, deux
    /// connexions sur le meme fichier ne se genent pas.
    pub fn appliquer(&self, db: &Database, chemin_base: String) {
        let active = lire(db).cibles.iter().any(|c| c.actif);
        let mut garde = match self.arret.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        if let Some(drapeau) = garde.take() {
            drapeau.store(true, Ordering::Relaxed);
        }
        if !active {
            return;
        }
        let drapeau = Arc::new(AtomicBool::new(false));
        *garde = Some(drapeau.clone());
        tokio::spawn(async move {
            match Database::new(&chemin_base) {
                Ok(propre) => boucle(Arc::new(propre), drapeau).await,
                // Sans base, rien a enregistrer : on le DIT dans le journal et on s'arrete,
                // plutot que de reessayer sans fin ou d'echouer en silence.
                Err(e) => journaliser("k8s.surveillance", &format!("base inaccessible : {e}")),
            }
        });
    }

    pub fn arreter(&self) {
        if let Some(drapeau) = self.arret.lock().ok().and_then(|mut g| g.take()) {
            drapeau.store(true, Ordering::Relaxed);
        }
    }
}

/// Le tour de boucle : qui doit etre mesure maintenant ?
///
/// Rend les cibles dues et l'instant de la prochaine echeance, pour dormir juste ce qu'il faut.
pub fn a_mesurer(
    cibles: &[Cible],
    dernieres: &std::collections::HashMap<String, i64>,
    maintenant: i64,
) -> Vec<Cible> {
    cibles
        .iter()
        .filter(|c| c.actif)
        .filter(|c| {
            let cle = format!("{}\u{0}{}", c.contexte, c.namespace);
            let periode = c.periode.max(PERIODE_MINIMUM) as i64 * 1000;
            match dernieres.get(&cle) {
                Some(dernier) => maintenant - dernier >= periode,
                // Jamais mesuree : on commence tout de suite, sinon la premiere courbe
                // attendrait une periode entiere pour son premier point.
                None => true,
            }
        })
        .cloned()
        .collect()
}

async fn boucle(db: Arc<Database>, arret: Arc<AtomicBool>) {
    let mut dernieres: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut tours: u32 = 0;

    while !arret.load(Ordering::Relaxed) {
        let reglages = lire(&db);
        let maintenant = horodatage();
        for cible in a_mesurer(&reglages.cibles, &dernieres, maintenant) {
            if arret.load(Ordering::Relaxed) {
                return;
            }
            let cle = format!("{}\u{0}{}", cible.contexte, cible.namespace);
            // L'instant est note AVANT l'appel : un cluster injoignable met dix secondes a le
            // dire, et sans ca on le rappellerait en boucle.
            dernieres.insert(cle, maintenant);
            if let Err(e) = mesurer(&db, &cible, maintenant).await {
                // Un cluster qui refuse ne doit pas arreter la surveillance des autres, ni
                // remplir le journal a chaque tour : on se tait et on retentera au suivant.
                let _ = e;
            }
        }

        // L'elagage coute un balayage : une fois par minute suffit largement, et il n'a rien a
        // faire tant que rien n'a ete ecrit.
        tours = tours.wrapping_add(1);
        if tours % 12 == 0 {
            let garde = lire(&db).retention_heures as i64 * 3_600_000;
            let _ = db.k8s_elaguer_avec(horodatage(), garde);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn mesurer(db: &Database, cible: &Cible, maintenant: i64) -> Result<(), String> {
    let (client, _) = super::client_de(&cible.contexte)?;
    let chemin = format!(
        "/apis/metrics.k8s.io/v1beta1/namespaces/{}/pods",
        cible.namespace
    );
    let mesures = client.json(&chemin).await?;
    let points: Vec<(String, i64, i64)> = mesures
        .pointer("/items")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|m| {
            let (mut cpu, mut ram) = (0i64, 0i64);
            for c in m.pointer("/containers").and_then(serde_json::Value::as_array).into_iter().flatten() {
                if let Some(v) = c.pointer("/usage/cpu").and_then(serde_json::Value::as_str) {
                    cpu += super::modele::millicores(v).unwrap_or(0) as i64;
                }
                if let Some(v) = c.pointer("/usage/memory").and_then(serde_json::Value::as_str) {
                    ram += super::modele::quantite(v).unwrap_or(0) as i64;
                }
            }
            let nom = m
                .pointer("/metadata/name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            (nom, cpu, ram)
        })
        .filter(|(nom, _, _)| !nom.is_empty())
        .collect();

    db.k8s_noter(&cible.contexte, &cible.namespace, maintenant, &points)
        .map_err(|e| e.to_string())
}

/// Ecrit dans le journal local : une panne de fond n'a pas d'ecran ou s'afficher.
fn journaliser(scope: &str, message: &str) {
    if let Some(dir) = crate::chemins::dossier_donnees() {
        let quand = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        crate::report::append_log(&dir, &crate::report::format_log_line(&quand, scope, message));
    }
}

fn horodatage() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cible(ns: &str, periode: u32, actif: bool) -> Cible {
        Cible { contexte: "prod".into(), namespace: ns.into(), periode, actif }
    }

    #[test]
    fn une_cible_jamais_mesuree_l_est_tout_de_suite() {
        // Sinon la premiere courbe attend une periode entiere avant son premier point, et sur
        // un rythme lent c'est long avant de voir quoi que ce soit.
        let dues = a_mesurer(&[cible("a", 60, true)], &HashMap::new(), 1_000_000);
        assert_eq!(dues.len(), 1);
    }

    #[test]
    fn une_cible_inactive_n_est_jamais_mesuree() {
        let dues = a_mesurer(&[cible("a", 30, false)], &HashMap::new(), 1_000_000);
        assert!(dues.is_empty(), "rien n'est surveille sans qu'on l'ait demande");
    }

    #[test]
    fn on_respecte_le_rythme_de_chaque_cible() {
        let maintenant = 1_000_000;
        let mut dernieres = HashMap::new();
        dernieres.insert("prod\u{0}a".to_string(), maintenant - 20_000);
        dernieres.insert("prod\u{0}b".to_string(), maintenant - 70_000);
        let dues = a_mesurer(
            &[cible("a", 60, true), cible("b", 60, true)],
            &dernieres,
            maintenant,
        );
        assert_eq!(dues.len(), 1);
        assert_eq!(dues[0].namespace, "b", "seule celle dont l'heure est venue");
    }

    #[test]
    fn un_rythme_trop_rapide_est_ramene_au_plancher() {
        let maintenant = 1_000_000;
        let mut dernieres = HashMap::new();
        dernieres.insert("prod\u{0}a".to_string(), maintenant - 5_000);
        // Une periode de 1 s demandee : on ne mesure pas plus vite que le plancher.
        assert!(a_mesurer(&[cible("a", 1, true)], &dernieres, maintenant).is_empty());
    }

    #[test]
    fn les_reglages_se_lisent_meme_quand_rien_n_a_ete_ecrit() {
        let db = crate::storage::Database::new(":memory:").unwrap();
        let r = lire(&db);
        assert!(r.cibles.is_empty(), "rien n'est surveille par defaut");
        assert_eq!(r.retention_heures, RETENTION_PAR_DEFAUT);
    }

    #[test]
    fn ecrire_borne_le_rythme_et_la_retention() {
        let db = crate::storage::Database::new(":memory:").unwrap();
        let rendu = ecrire(
            &db,
            Reglages { cibles: vec![cible("a", 1, true)], retention_heures: 99_999 },
        )
        .unwrap();
        assert_eq!(rendu.cibles[0].periode, PERIODE_MINIMUM);
        assert_eq!(rendu.retention_heures, 24 * 30);
        // Et ce qui est ecrit se relit a l'identique.
        let relu = lire(&db);
        assert_eq!(relu.cibles[0].periode, PERIODE_MINIMUM);
        assert_eq!(relu.retention_heures, 24 * 30);
    }

    #[test]
    fn un_namespace_au_nom_douteux_est_refuse() {
        let db = crate::storage::Database::new(":memory:").unwrap();
        let erreur = ecrire(
            &db,
            Reglages { cibles: vec![cible("../autre", 60, true)], retention_heures: 24 },
        )
        .unwrap_err();
        assert!(erreur.contains("refuse"), "{erreur}");
        assert!(lire(&db).cibles.is_empty(), "et rien n'a ete enregistre");
    }
}
