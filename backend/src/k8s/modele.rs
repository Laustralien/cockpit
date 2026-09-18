//! Reduire ce que le cluster envoie a ce que l'ecran affiche.
//!
//! **LE BACKEND REDUIT, L'INTERFACE N'AVALE PAS 6 Mo.** Mesure sur un namespace reel : la liste
//! complete des pods pese 6,3 Mo de JSON pour 300 pods, dont l'ecran n'utilise qu'une douzaine
//! de champs. Traverser le pont avec tout ca ferait payer a l'interface une seconde de lecture
//! et de rendu a chaque rafraichissement, ce que la regle du projet interdit. Reduit, le meme
//! namespace tient dans quelques dizaines de kilo-octets.
//!
//! Tout ce qui est ici est PUR : un objet JSON entre, une ligne d'ecran sort.

use serde::Serialize;
use serde_json::Value;

/// Une ligne de la liste.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pod {
    pub nom: String,
    /// Ce qui a cree ce pod, sans le suffixe technique : c'est le titre du groupe a l'ecran.
    pub groupe: String,
    /// Ce qui a cree le pod : `Deployment`, `Job`, `StatefulSet`… ou vide.
    pub sorte: String,
    /// La version livree, telle qu'elle se lit sur l'image (le tag). Vide s'il n'y en a pas.
    pub version: String,
    /// L'etat tel que kubectl l'afficherait : la phase, ou la vraie raison quand il y en a une.
    pub etat: String,
    /// Vrai quand cet etat demande qu'on aille voir.
    pub ennuyeux: bool,
    /// Conteneurs prets sur conteneurs declares.
    pub prets: u32,
    pub conteneurs: u32,
    pub redemarrages: u32,
    /// Quand il a demarre, en ISO. L'age se calcule a l'affichage, sinon il se fige.
    pub depuis: Option<String>,
    pub machine: String,
    /// Les noms des conteneurs : le choix « logs de quel conteneur ».
    pub noms_conteneurs: Vec<String>,
    /// Millicores et octets, quand les mesures sont disponibles.
    pub cpu: Option<u64>,
    pub ram: Option<u64>,
}

/// L'etat affiche par kubectl, qui n'est PAS la phase.
///
/// **UN POD « Running » PEUT ETRE EN TRAIN D'ECHOUER EN BOUCLE.** La phase vaut `Running` des
/// que le pod est accepte par une machine ; ce qu'on veut lire, c'est `CrashLoopBackOff` ou
/// `ImagePullBackOff`, qui vivent dans l'etat de chaque conteneur. Afficher la phase seule
/// montrerait un namespace tout vert avec un service qui ne demarre pas — exactement ce qu'on
/// reproche a l'ecran d'en face.
pub fn etat_lisible(pod: &Value) -> (String, bool) {
    if pod.pointer("/metadata/deletionTimestamp").is_some() {
        return ("Terminating".into(), false);
    }
    let phase = pod.pointer("/status/phase").and_then(Value::as_str).unwrap_or("Unknown");

    // L'attente ou l'arret d'un conteneur porte la vraie raison.
    for liste in ["/status/initContainerStatuses", "/status/containerStatuses"] {
        for c in pod.pointer(liste).and_then(Value::as_array).into_iter().flatten() {
            if let Some(raison) = c.pointer("/state/waiting/reason").and_then(Value::as_str) {
                // `ContainerCreating` et `PodInitializing` sont l'ordinaire d'un demarrage.
                let ordinaire = matches!(raison, "ContainerCreating" | "PodInitializing");
                return (raison.to_string(), !ordinaire);
            }
            if let Some(raison) = c.pointer("/state/terminated/reason").and_then(Value::as_str) {
                if raison != "Completed" {
                    return (raison.to_string(), true);
                }
            }
        }
    }
    let ennuyeux = matches!(phase, "Failed" | "Unknown");
    (phase.to_string(), ennuyeux)
}

/// Le groupe auquel rattacher le pod, et la sorte de ce qui l'a cree.
///
/// **LE SUFFIXE D'UN REPLICASET EST CELUI D'UNE LIVRAISON, PAS D'UN SERVICE.** `web-5cb5677dcc`
/// devient `web` : c'est le service, et deux livraisons du meme service se retrouvent donc dans
/// le meme groupe, ce qui permet de voir une bascule en cours au lieu de deux blocs etrangers.
pub fn groupe_de(pod: &Value) -> (String, String) {
    let proprietaire = pod
        .pointer("/metadata/ownerReferences")
        .and_then(Value::as_array)
        .and_then(|l| l.first());
    let (sorte, nom) = match proprietaire {
        Some(p) => (
            p.get("kind").and_then(Value::as_str).unwrap_or_default().to_string(),
            p.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        ),
        None => (String::new(), String::new()),
    };
    let groupe = match sorte.as_str() {
        // Un ReplicaSet est fabrique par un Deployment : son suffixe change a chaque livraison.
        "ReplicaSet" => sans_dernier_morceau(&nom),
        // Un Job cree par un CronJob porte l'horodatage de son declenchement.
        "Job" => sans_dernier_morceau(&nom),
        // Un pod SANS proprietaire : son createur a disparu, mais son nom porte encore sa
        // trace. Vu sur un vrai namespace : quatorze pods en echec, tous issus du meme
        // travail planifie, faisaient quatorze groupes d'une ligne, tous nommes pareil au
        // suffixe pres. On retire donc ce que Kubernetes a ajoute, et rien d'autre.
        "" => sans_les_suffixes_generes(
            pod.pointer("/metadata/name").and_then(Value::as_str).unwrap_or_default(),
        ),
        _ => nom.clone(),
    };
    let sorte = match sorte.as_str() {
        "ReplicaSet" => "Deployment".to_string(),
        "Job" => "CronJob".to_string(),
        autre => autre.to_string(),
    };
    (groupe, sorte)
}

/// Retire d'un nom de pod ce que Kubernetes y a ajoute : le suffixe aleatoire, puis
/// l'horodatage d'un declenchement planifie.
///
/// **ON NE RETIRE QUE CE QU'ON RECONNAIT.** Un pod cree a la main garde son nom entier :
/// inventer un groupe a partir d'un nom quelconque rassemblerait des choses sans rapport.
fn sans_les_suffixes_generes(nom: &str) -> String {
    let mut morceaux: Vec<&str> = nom.split('-').collect();
    // Le suffixe d'un pod : cinq caracteres, chiffres et lettres minuscules.
    if morceaux.len() > 1 {
        let dernier = morceaux[morceaux.len() - 1];
        if dernier.len() == 5 && dernier.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
            morceaux.pop();
        }
    }
    // L'horodatage que pose un travail planifie : huit chiffres ou plus.
    if morceaux.len() > 1 {
        let dernier = morceaux[morceaux.len() - 1];
        if dernier.len() >= 8 && dernier.chars().all(|c| c.is_ascii_digit()) {
            morceaux.pop();
        }
    }
    morceaux.join("-")
}

fn sans_dernier_morceau(nom: &str) -> String {
    match nom.rsplit_once('-') {
        Some((debut, _)) if !debut.is_empty() => debut.to_string(),
        _ => nom.to_string(),
    }
}

/// Le tag de la premiere image : la version livree, telle qu'on en parle.
pub fn version_de(pod: &Value) -> String {
    version_depuis_image(
        pod.pointer("/spec/containers/0/image")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

/// Le tag d'une image. Partage avec les objets declares, qui rangent leur image ailleurs.
///
/// Une image s'ecrit `depot/nom:tag`, et le depot peut porter un port (`hote:5000/nom`) : le
/// `:` d'un port n'est pas celui d'un tag.
pub fn version_depuis_image(image: &str) -> String {
    match image.rsplit_once(':') {
        Some((_, tag)) if !tag.contains('/') => tag.to_string(),
        _ => String::new(),
    }
}

/// Reduit un pod du cluster a sa ligne d'ecran.
pub fn reduire(pod: &Value) -> Pod {
    let (etat, ennuyeux) = etat_lisible(pod);
    let (groupe, sorte) = groupe_de(pod);
    let statuts = pod
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let declares = pod
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .map(|c| c.len() as u32)
        .unwrap_or(statuts.len() as u32);
    Pod {
        nom: texte(pod, "/metadata/name"),
        groupe,
        sorte,
        version: version_de(pod),
        etat,
        ennuyeux,
        prets: statuts
            .iter()
            .filter(|c| c.get("ready").and_then(Value::as_bool).unwrap_or(false))
            .count() as u32,
        conteneurs: declares,
        redemarrages: statuts
            .iter()
            .filter_map(|c| c.get("restartCount").and_then(Value::as_u64))
            .sum::<u64>() as u32,
        depuis: pod
            .pointer("/status/startTime")
            .and_then(Value::as_str)
            .map(str::to_string),
        machine: texte(pod, "/spec/nodeName"),
        noms_conteneurs: pod
            .pointer("/spec/containers")
            .and_then(Value::as_array)
            .map(|l| l.iter().map(|c| texte(c, "/name")).collect())
            .unwrap_or_default(),
        cpu: None,
        ram: None,
    }
}

fn texte(valeur: &Value, chemin: &str) -> String {
    valeur.pointer(chemin).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Une quantite Kubernetes (`250m`, `1`, `410Mi`, `2Gi`) en unite de base.
///
/// Le CPU sort en millicores, la memoire en octets. **Les suffixes binaires et decimaux
/// coexistent** (`Mi` vaut 1 048 576, `M` vaut 1 000 000) : les confondre afficherait 5 % de
/// memoire en trop, ce qui suffit a faire douter de tout le reste.
pub fn quantite(valeur: &str) -> Option<u64> {
    let v = valeur.trim();
    if v.is_empty() {
        return None;
    }
    let (nombre, suffixe) = v.split_at(v.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(v.len()));
    let nombre: f64 = nombre.parse().ok()?;
    let facteur: f64 = match suffixe {
        "" => 1.0,
        "n" => 1e-9,
        "u" => 1e-6,
        "m" => 1e-3,
        "k" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "P" => 1e15,
        "Ki" => 1024.0,
        "Mi" => 1024f64.powi(2),
        "Gi" => 1024f64.powi(3),
        "Ti" => 1024f64.powi(4),
        "Pi" => 1024f64.powi(5),
        _ => return None,
    };
    Some((nombre * facteur).round() as u64)
}

/// Le CPU d'un conteneur en MILLICORES : `250m` vaut 250, `1` vaut 1000.
pub fn millicores(valeur: &str) -> Option<u64> {
    let v = valeur.trim();
    if let Some(sans) = v.strip_suffix('m') {
        sans.parse::<f64>().ok().map(|n| n.round() as u64)
    } else if let Some(sans) = v.strip_suffix('n') {
        // Le serveur de mesures repond en nanocores, jamais en millicores.
        sans.parse::<f64>().ok().map(|n| (n / 1e6).round() as u64)
    } else if let Some(sans) = v.strip_suffix('u') {
        sans.parse::<f64>().ok().map(|n| (n / 1e3).round() as u64)
    } else {
        v.parse::<f64>().ok().map(|n| (n * 1000.0).round() as u64)
    }
}

/// Colle les mesures (CPU, RAM) sur les pods, par nom.
pub fn poser_les_mesures(pods: &mut [Pod], mesures: &Value) {
    for m in mesures.pointer("/items").and_then(Value::as_array).into_iter().flatten() {
        let nom = texte(m, "/metadata/name");
        let Some(pod) = pods.iter_mut().find(|p| p.nom == nom) else { continue };
        let conteneurs = m.pointer("/containers").and_then(Value::as_array);
        let (mut cpu, mut ram) = (0u64, 0u64);
        for c in conteneurs.into_iter().flatten() {
            if let Some(v) = c.pointer("/usage/cpu").and_then(Value::as_str) {
                cpu += millicores(v).unwrap_or(0);
            }
            if let Some(v) = c.pointer("/usage/memory").and_then(Value::as_str) {
                ram += quantite(v).unwrap_or(0);
            }
        }
        pod.cpu = Some(cpu);
        pod.ram = Some(ram);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod_qui_tourne() -> Value {
        json!({
            "metadata": {
                "name": "web-5cb5677dcc-8ms7z",
                "ownerReferences": [{ "kind": "ReplicaSet", "name": "web-5cb5677dcc" }]
            },
            "spec": {
                "nodeName": "machine-12",
                "containers": [{ "name": "web", "image": "depot.exemple.test/appli:v2-ec7a2209" }]
            },
            "status": {
                "phase": "Running",
                "startTime": "2026-09-15T08:00:00Z",
                "containerStatuses": [{ "name": "web", "ready": true, "restartCount": 2,
                                        "state": { "running": { "startedAt": "2026-09-15T08:00:01Z" } } }]
            }
        })
    }

    #[test]
    fn un_pod_se_reduit_a_sa_ligne_d_ecran() {
        let p = reduire(&pod_qui_tourne());
        assert_eq!(p.nom, "web-5cb5677dcc-8ms7z");
        assert_eq!(p.groupe, "web", "le suffixe de livraison ne fait pas partie du service");
        assert_eq!(p.sorte, "Deployment");
        assert_eq!(p.version, "v2-ec7a2209");
        assert_eq!((p.prets, p.conteneurs), (1, 1));
        assert_eq!(p.redemarrages, 2);
        assert_eq!(p.machine, "machine-12");
        assert_eq!(p.noms_conteneurs, vec!["web"]);
        assert_eq!(p.etat, "Running");
        assert!(!p.ennuyeux);
    }

    #[test]
    fn un_pod_qui_echoue_en_boucle_ne_s_annonce_pas_comme_en_marche() {
        // Le cas qui justifie ce module : la phase dit « Running », le conteneur dit autre chose.
        let mut pod = pod_qui_tourne();
        pod["status"]["containerStatuses"][0]["ready"] = json!(false);
        pod["status"]["containerStatuses"][0]["state"] =
            json!({ "waiting": { "reason": "CrashLoopBackOff" } });
        let p = reduire(&pod);
        assert_eq!(p.etat, "CrashLoopBackOff");
        assert!(p.ennuyeux, "c'est exactement ce qu'on veut voir remonter");
        assert_eq!(p.prets, 0);
    }

    #[test]
    fn un_demarrage_ordinaire_n_alerte_personne() {
        // Un pod qui se cree passe par la : le signaler ferait clignoter l'ecran a chaque MEP.
        let mut pod = pod_qui_tourne();
        pod["status"]["containerStatuses"][0]["state"] =
            json!({ "waiting": { "reason": "ContainerCreating" } });
        let p = reduire(&pod);
        assert_eq!(p.etat, "ContainerCreating");
        assert!(!p.ennuyeux);
    }

    #[test]
    fn un_pod_qu_on_supprime_le_dit() {
        let mut pod = pod_qui_tourne();
        pod["metadata"]["deletionTimestamp"] = json!("2026-09-16T10:00:00Z");
        assert_eq!(reduire(&pod).etat, "Terminating");
    }

    #[test]
    fn un_job_termine_n_est_pas_un_echec() {
        let pod = json!({
            "metadata": { "name": "nettoyage-28123-abcde",
                          "ownerReferences": [{ "kind": "Job", "name": "nettoyage-28123" }] },
            "spec": { "containers": [{ "name": "c", "image": "exemple:1" }] },
            "status": { "phase": "Succeeded",
                        "containerStatuses": [{ "name": "c", "ready": false, "restartCount": 0,
                                                "state": { "terminated": { "reason": "Completed" } } }] }
        });
        let p = reduire(&pod);
        assert_eq!(p.etat, "Succeeded");
        assert!(!p.ennuyeux, "258 jobs finis ne doivent pas peindre l'ecran en rouge");
        assert_eq!(p.groupe, "nettoyage", "tous les declenchements d'un meme travail ensemble");
        assert_eq!(p.sorte, "CronJob");
    }

    #[test]
    fn un_pod_sans_proprietaire_est_son_propre_groupe() {
        // Les 14 pods orphelins vus sur un vrai namespace : ils viennent de nulle part et
        // doivent rester lisibles au lieu de tomber dans un groupe vide.
        let pod = json!({
            "metadata": { "name": "essai-a-la-main" },
            "spec": { "containers": [{ "name": "c", "image": "exemple:1" }] },
            "status": { "phase": "Failed" }
        });
        let p = reduire(&pod);
        assert_eq!(p.groupe, "essai-a-la-main");
        assert_eq!(p.sorte, "");
        assert!(p.ennuyeux);
    }

    #[test]
    fn des_pods_orphelins_du_meme_travail_se_retrouvent_ensemble() {
        // Vu sur un vrai namespace : le travail planifie a ete supprime, ses quatorze pods en
        // echec sont restes. Un groupe par pod rendait la liste illisible.
        let orphelin = |nom: &str| {
            json!({
                "metadata": { "name": nom },
                "spec": { "containers": [{ "name": "c", "image": "exemple:1" }] },
                "status": { "phase": "Failed" }
            })
        };
        assert_eq!(groupe_de(&orphelin("compute-forum-volumes-29819670-6xj44")).0, "compute-forum-volumes");
        assert_eq!(groupe_de(&orphelin("compute-forum-volumes-29821110-2tfnf")).0, "compute-forum-volumes");
    }

    #[test]
    fn un_nom_qu_on_ne_reconnait_pas_reste_entier() {
        let pod = json!({
            "metadata": { "name": "essai-a-la-main" },
            "spec": { "containers": [{ "name": "c", "image": "exemple:1" }] },
            "status": { "phase": "Running" }
        });
        assert_eq!(groupe_de(&pod).0, "essai-a-la-main", "on n'invente pas un groupe");
        assert_eq!(sans_les_suffixes_generes("prometheus"), "prometheus");
        assert_eq!(sans_les_suffixes_generes("29819670"), "29819670", "un seul morceau reste");
    }

    #[test]
    fn une_image_sans_tag_ou_avec_un_port_ne_fabrique_pas_une_fausse_version() {
        let mut pod = pod_qui_tourne();
        pod["spec"]["containers"][0]["image"] = json!("depot.exemple.test/appli");
        assert_eq!(version_de(&pod), "", "pas de tag, pas de version inventee");
        pod["spec"]["containers"][0]["image"] = json!("depot.exemple.test:5000/appli");
        assert_eq!(version_de(&pod), "", "5000/appli n'est pas un tag");
        pod["spec"]["containers"][0]["image"] = json!("depot.exemple.test:5000/appli:v9");
        assert_eq!(version_de(&pod), "v9");
    }

    #[test]
    fn les_quantites_binaires_et_decimales_ne_se_confondent_pas() {
        assert_eq!(quantite("410Mi"), Some(429_916_160));
        assert_eq!(quantite("410M"), Some(410_000_000));
        assert_eq!(quantite("2Gi"), Some(2_147_483_648));
        assert_eq!(quantite("1024"), Some(1024));
        assert_eq!(quantite(""), None);
        assert_eq!(quantite("beaucoup"), None);
    }

    #[test]
    fn le_cpu_se_lit_toujours_en_millicores() {
        assert_eq!(millicores("250m"), Some(250));
        assert_eq!(millicores("1"), Some(1000), "un coeur entier vaut mille");
        assert_eq!(millicores("1500000n"), Some(2), "les nanocores du serveur de mesures");
        assert_eq!(millicores("0"), Some(0));
    }

    #[test]
    fn les_mesures_se_collent_sur_le_bon_pod_et_s_additionnent() {
        let mut pods = vec![reduire(&pod_qui_tourne())];
        let mesures = json!({ "items": [
            { "metadata": { "name": "web-5cb5677dcc-8ms7z" }, "containers": [
                { "name": "web",  "usage": { "cpu": "40m", "memory": "410Mi" } },
                { "name": "side", "usage": { "cpu": "10m", "memory": "10Mi" } }
            ]},
            { "metadata": { "name": "un-autre" }, "containers": [
                { "name": "c", "usage": { "cpu": "999m", "memory": "9Gi" } }
            ]}
        ]});
        poser_les_mesures(&mut pods, &mesures);
        assert_eq!(pods[0].cpu, Some(50), "les conteneurs d'un pod s'additionnent");
        assert_eq!(pods[0].ram, Some(429_916_160 + 10_485_760));
    }

    #[test]
    fn un_pod_sans_mesure_n_affiche_pas_zero() {
        // Sans serveur de mesures, « 0 m » se lirait « ce pod ne consomme rien », ce qui est faux.
        let mut pods = vec![reduire(&pod_qui_tourne())];
        poser_les_mesures(&mut pods, &json!({ "items": [] }));
        assert_eq!(pods[0].cpu, None);
        assert_eq!(pods[0].ram, None);
    }
}
