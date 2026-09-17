//! Le flux des changements : l'ecran se met a jour sans jamais recharger la liste.
//!
//! **C'EST TOUT L'INTERET DE PARLER A L'API DIRECTEMENT.** Mesure sur un namespace reel : la
//! liste complete pese 6,3 Mo et prend une seconde. La redemander toutes les cinq secondes
//! ferait ce que fait l'ecran d'en face, et la regle du projet l'interdit (« tout ce qui est
//! appele sur un minuteur se mesure multiplie »). Le cluster sait envoyer ce qui CHANGE : on
//! ouvre un flux et on ne recoit plus que quelques centaines d'octets par evenement.
//!
//! Trois gardes qui viennent de regles deja payees ici :
//! **(1)** un seul suivi a la fois, arrete des qu'on quitte l'ecran — ce qui peut ne pas
//! tourner ne tourne pas ;
//! **(2)** la relance est BORNEE et espacee : une reconnexion en boucle sur un cluster
//! injoignable prendrait le poste en otage ;
//! **(3)** quand le cluster dit que notre point de reprise est trop vieux (410), on ne devine
//! pas : on demande a l'ecran de tout relire.

use super::modele::{self, Pod};
use crate::evenements::Emetteurs;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Ce que l'ecran ecoute.
pub const CHANGEMENT: &str = "k8s_changement";
pub const MESURES: &str = "k8s_mesures";
/// « Relis tout » : notre point de reprise n'est plus valable, ou le flux a rendu l'ame.
pub const RELIRE: &str = "k8s_relire";
pub const PANNE: &str = "k8s_panne";

/// Combien de fois on rouvre un flux qui tombe avant d'abandonner et de le dire.
const TENTATIVES: u32 = 3;
/// Entre deux tentatives. Un cluster qui redemarre met plus que ca, et l'ecran garde son
/// bouton pour reessayer : c'est un refus qui se voit, pas une boucle invisible.
const REPOS: Duration = Duration::from_secs(3);
/// Les mesures n'ont pas de flux : elles se redemandent. 0,08 s mesuree pour 300 pods, ce qui
/// laisse le choix du rythme a l'utilisateur — c'est lui qui regarde les courbes.
const PERIODE_PAR_DEFAUT: u64 = 5;
/// En dessous, on interrogerait le cluster plus vite qu'il ne mesure : ses propres mesures sont
/// rafraichies toutes les quinze secondes environ, et rien ne bougerait de plus.
const PERIODE_MINIMUM: u64 = 5;

/// Le suivi en cours. Un seul, parce qu'un seul ecran regarde.
pub struct Suivi {
    arret: std::sync::Mutex<Option<Arc<AtomicBool>>>,
    /// Le rythme des mesures, en secondes. Change sans couper le flux en cours : la boucle le
    /// relit a chaque tour, donc regler le rafraichissement ne fait pas repartir la courbe.
    periode: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for Suivi {
    fn default() -> Self {
        Self {
            arret: std::sync::Mutex::new(None),
            periode: Arc::new(std::sync::atomic::AtomicU64::new(PERIODE_PAR_DEFAUT)),
        }
    }
}

impl Suivi {
    /// Arrete le suivi precedent, s'il y en a un.
    pub fn arreter(&self) {
        if let Some(drapeau) = self.arret.lock().ok().and_then(|mut v| v.take()) {
            drapeau.store(true, Ordering::Relaxed);
        }
    }

    /// Le rythme des mesures, en secondes.
    pub fn regler_la_periode(&self, secondes: u64) {
        self.periode
            .store(secondes.max(PERIODE_MINIMUM), Ordering::Relaxed);
    }

    /// Demarre le suivi d'un namespace. Le precedent s'arrete.
    pub fn demarrer(
        &self,
        emetteur: Emetteurs,
        contexte: String,
        namespace: String,
        depuis: String,
    ) -> Result<(), String> {
        if !super::kubeconfig::nom_valide(&namespace) {
            return Err(format!("nom de namespace refuse : {namespace}"));
        }
        self.arreter();
        let drapeau = Arc::new(AtomicBool::new(false));
        *self.arret.lock().map_err(|_| "suivi inaccessible")? = Some(drapeau.clone());

        tokio::spawn(boucle(
            emetteur,
            contexte,
            namespace,
            depuis,
            drapeau,
            self.periode.clone(),
        ));
        Ok(())
    }
}

async fn boucle(
    emetteur: Emetteurs,
    contexte: String,
    namespace: String,
    depuis: String,
    arret: Arc<AtomicBool>,
    periode: Arc<std::sync::atomic::AtomicU64>,
) {
    let mesures = tokio::spawn(boucle_des_mesures(
        emetteur.clone(),
        contexte.clone(),
        namespace.clone(),
        arret.clone(),
        periode,
    ));

    let mut version = depuis;
    let mut echecs = 0u32;
    while !arret.load(Ordering::Relaxed) {
        match un_flux(&emetteur, &contexte, &namespace, &version, &arret).await {
            // Le serveur a referme proprement : on repart d'ou on en etait, sans compter
            // ca comme un echec.
            Ok(Some(suivante)) => {
                version = suivante;
                echecs = 0;
            }
            // Notre point de reprise est trop vieux : seul l'ecran peut repartir juste.
            Ok(None) => {
                emetteur.emettre(RELIRE, Value::Null);
                break;
            }
            Err(raison) => {
                echecs += 1;
                if echecs >= TENTATIVES || arret.load(Ordering::Relaxed) {
                    emetteur.emettre(PANNE, serde_json::json!(raison));
                    break;
                }
                tokio::time::sleep(REPOS).await;
            }
        }
    }
    arret.store(true, Ordering::Relaxed);
    mesures.abort();
}

/// Un flux, jusqu'a ce que le serveur le referme. Rend la version ou reprendre, ou `None`
/// quand il faut tout relire.
async fn un_flux(
    emetteur: &Emetteurs,
    contexte: &str,
    namespace: &str,
    depuis: &str,
    arret: &AtomicBool,
) -> Result<Option<String>, String> {
    let (client, _) = super::client_de(contexte)?;
    let chemin = format!(
        "/api/v1/namespaces/{namespace}/pods?watch=1&resourceVersion={depuis}&timeoutSeconds={}&allowWatchBookmarks=true",
        super::client::DUREE_DU_FLUX
    );
    let mut reponse = client.ouvrir_le_flux(&chemin).await?;
    let mut version = depuis.to_string();
    let mut reste = Vec::<u8>::new();

    while let Some(morceau) = reponse
        .chunk()
        .await
        .map_err(|e| format!("flux interrompu : {e}"))?
    {
        if arret.load(Ordering::Relaxed) {
            return Ok(Some(version));
        }
        reste.extend_from_slice(&morceau);
        // Un evenement par ligne, et un morceau peut en couper une en deux.
        while let Some(fin) = reste.iter().position(|o| *o == b'\n') {
            let ligne: Vec<u8> = reste.drain(..=fin).collect();
            let Ok(evenement) = serde_json::from_slice::<Value>(&ligne[..ligne.len() - 1]) else {
                continue;
            };
            match traiter(emetteur, &evenement) {
                Some(Suite::Version(v)) => version = v,
                Some(Suite::Relire) => return Ok(None),
                None => {}
            }
        }
    }
    Ok(Some(version))
}

enum Suite {
    Version(String),
    Relire,
}

fn traiter(emetteur: &Emetteurs, evenement: &Value) -> Option<Suite> {
    let sorte = evenement.get("type").and_then(Value::as_str).unwrap_or_default();
    let objet = evenement.get("object")?;
    let version = objet
        .pointer("/metadata/resourceVersion")
        .and_then(Value::as_str)
        .map(str::to_string);

    match sorte {
        // Le serveur nous donne un point de reprise sans rien d'autre a dire.
        "BOOKMARK" => version.map(Suite::Version),
        // **UNE ERREUR DANS LE FLUX NE SE RATTRAPE PAS AU JUGE.** Le cas courant est le 410
        // « Expired » : la fenetre d'historique du cluster est passee devant notre point de
        // reprise. Reprendre quand meme ferait manquer en silence tout ce qui a change
        // entre-temps, donc on demande a l'ecran de relire, quelle que soit l'erreur.
        "ERROR" => Some(Suite::Relire),
        "ADDED" | "MODIFIED" | "DELETED" => {
            let pod: Pod = modele::reduire(objet);
            emetteur.emettre(
                CHANGEMENT,
                serde_json::json!({ "sorte": sorte, "pod": pod }),
            );
            version.map(Suite::Version)
        }
        _ => None,
    }
}

/// Les mesures n'ont pas de flux : on les redemande, et seulement tant que l'ecran regarde.
async fn boucle_des_mesures(
    emetteur: Emetteurs,
    contexte: String,
    namespace: String,
    arret: Arc<AtomicBool>,
    periode: Arc<std::sync::atomic::AtomicU64>,
) {
    // Une premiere mesure tout de suite : sans elle, la courbe reste vide le temps d'un tour,
    // et sur un rythme lent ce serait long avant de voir quoi que ce soit.
    let mut attente = Duration::from_millis(50);
    while !arret.load(Ordering::Relaxed) {
        tokio::time::sleep(attente).await;
        attente = Duration::from_secs(periode.load(Ordering::Relaxed).max(PERIODE_MINIMUM));
        if arret.load(Ordering::Relaxed) {
            return;
        }
        let Ok((client, _)) = super::client_de(&contexte) else { return };
        let chemin = format!("/apis/metrics.k8s.io/v1beta1/namespaces/{namespace}/pods");
        // Un cluster sans serveur de mesures a deja dit pourquoi au premier chargement :
        // le redire toutes les quinze secondes n'apprendrait rien a personne.
        if let Ok(mesures) = client.json(&chemin).await {
            emetteur.emettre(MESURES, reduire_les_mesures(&mesures));
        }
    }
}

/// Les mesures, reduites a ce que l'ecran en fait : un nom, un CPU, une RAM.
///
/// Le brut pese plusieurs dizaines de kilo-octets toutes les quinze secondes pour trois
/// nombres par pod. Meme raison que pour les pods : le pont ne transporte pas ce que
/// personne n'affiche.
fn reduire_les_mesures(mesures: &Value) -> Value {
    let lignes: Vec<Value> = mesures
        .pointer("/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|m| {
            let (mut cpu, mut ram) = (0u64, 0u64);
            for c in m.pointer("/containers").and_then(Value::as_array).into_iter().flatten() {
                if let Some(v) = c.pointer("/usage/cpu").and_then(Value::as_str) {
                    cpu += modele::millicores(v).unwrap_or(0);
                }
                if let Some(v) = c.pointer("/usage/memory").and_then(Value::as_str) {
                    ram += modele::quantite(v).unwrap_or(0);
                }
            }
            serde_json::json!({
                "nom": m.pointer("/metadata/name").and_then(Value::as_str).unwrap_or_default(),
                "cpu": cpu,
                "ram": ram,
            })
        })
        .collect();
    Value::Array(lignes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Espion {
        recus: Mutex<Vec<(String, Value)>>,
    }
    impl crate::evenements::Emetteur for Espion {
        fn emettre(&self, evenement: &str, charge: Value) {
            self.recus.lock().unwrap().push((evenement.to_string(), charge));
        }
    }

    /// Rend l'espion ET sa vue en `Emetteurs` : garder les deux evite d'avoir a retrouver
    /// le type concret derriere le `dyn`.
    fn espion() -> (Arc<Espion>, Emetteurs) {
        let espion = Arc::new(Espion::default());
        (espion.clone(), espion)
    }

    fn evenement(sorte: &str) -> Value {
        serde_json::json!({
            "type": sorte,
            "object": {
                "metadata": { "name": "web-5cb5677dcc-8ms7z", "resourceVersion": "4242",
                              "ownerReferences": [{ "kind": "ReplicaSet", "name": "web-5cb5677dcc" }] },
                "spec": { "containers": [{ "name": "web", "image": "exemple:v2" }] },
                "status": { "phase": "Running" }
            }
        })
    }

    #[test]
    fn un_changement_part_a_l_ecran_deja_reduit() {
        let (espion, emetteur) = espion();
        let suite = traiter(&emetteur, &evenement("MODIFIED"));
        assert!(matches!(suite, Some(Suite::Version(v)) if v == "4242"));
        let recus = espion.recus.lock().unwrap().clone();
        assert_eq!(recus.len(), 1);
        assert_eq!(recus[0].0, CHANGEMENT);
        assert_eq!(recus[0].1["sorte"], "MODIFIED");
        assert_eq!(recus[0].1["pod"]["groupe"], "web", "l'ecran recoit une ligne, pas 20 Ko");
        assert!(recus[0].1["pod"].get("metadata").is_none(), "rien de brut ne passe");
    }

    #[test]
    fn un_point_de_reprise_avance_la_version_sans_rien_afficher() {
        // Sans ca, un namespace calme ferait reprendre le flux de plus en plus loin dans le
        // passe, jusqu'au 410 qui oblige a tout relire.
        let (espion, emetteur) = espion();
        let mut e = evenement("BOOKMARK");
        e["object"]["metadata"]["resourceVersion"] = serde_json::json!("9999");
        assert!(matches!(traiter(&emetteur, &e), Some(Suite::Version(v)) if v == "9999"));
        assert!(espion.recus.lock().unwrap().is_empty(), "rien a montrer");
    }

    #[test]
    fn une_erreur_du_flux_fait_tout_relire_au_lieu_de_deviner() {
        let (_, emetteur) = espion();
        let e = serde_json::json!({ "type": "ERROR", "object": { "code": 410, "reason": "Expired" } });
        assert!(matches!(traiter(&emetteur, &e), Some(Suite::Relire)));
    }

    #[test]
    fn une_ligne_incomprehensible_ne_fait_rien_planter() {
        let (_, emetteur) = espion();
        assert!(traiter(&emetteur, &serde_json::json!({ "type": "INCONNU" })).is_none());
        assert!(traiter(&emetteur, &serde_json::json!({})).is_none());
    }

    #[test]
    fn les_mesures_partent_reduites_et_additionnees() {
        let brut = serde_json::json!({ "items": [
            { "metadata": { "name": "web-1" }, "containers": [
                { "usage": { "cpu": "40m", "memory": "410Mi" } },
                { "usage": { "cpu": "1500000n", "memory": "10Mi" } }
            ]}
        ]});
        let reduit = reduire_les_mesures(&brut);
        assert_eq!(reduit[0]["nom"], "web-1");
        assert_eq!(reduit[0]["cpu"], 42, "les conteneurs s'additionnent, nanocores compris");
        assert_eq!(reduit[0]["ram"], 429_916_160u64 + 10_485_760u64);
        assert!(reduit[0].get("containers").is_none(), "le brut ne traverse pas le pont");
    }

    #[test]
    fn un_suivi_qui_demarre_arrete_le_precedent() {
        // Deux ecrans de suite sur deux namespaces : le premier flux doit mourir, sinon on
        // paie deux connexions et l'ecran recoit des pods qui ne sont plus les siens.
        let suivi = Suivi::default();
        let premier = Arc::new(AtomicBool::new(false));
        *suivi.arret.lock().unwrap() = Some(premier.clone());
        suivi.arreter();
        assert!(premier.load(Ordering::Relaxed), "le precedent a recu l'ordre d'arret");
    }

    #[test]
    fn un_namespace_au_nom_douteux_ne_demarre_aucun_flux() {
        let suivi = Suivi::default();
        let (_, emetteur) = espion();
        let erreur = suivi
            .demarrer(emetteur, "ctx".into(), "../autre".into(), "1".into())
            .unwrap_err();
        assert!(erreur.contains("refuse"), "{erreur}");
    }

}

#[cfg(test)]
mod tests_periode {
    use super::*;

    #[test]
    fn le_rythme_des_mesures_ne_descend_pas_sous_le_plancher() {
        // En dessous, on interrogerait le cluster plus vite qu'il ne mesure : rien ne bougerait
        // de plus, et on paierait la question a chaque tour.
        let suivi = Suivi::default();
        suivi.regler_la_periode(0);
        assert_eq!(suivi.periode.load(Ordering::Relaxed), PERIODE_MINIMUM);
        suivi.regler_la_periode(3600);
        assert_eq!(suivi.periode.load(Ordering::Relaxed), 3600);
    }
}
