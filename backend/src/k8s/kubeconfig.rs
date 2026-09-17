//! Lire le kubeconfig et en tirer de quoi appeler un cluster.
//!
//! **COCKPIT NE DETIENT AUCUN SECRET DE CLUSTER, ET C'EST UN CHOIX.** Les jetons restent dans
//! les fichiers que kubectl, Rancher et le reste ecrivent deja ; on les relit a chaque appel.
//! Rien n'est recopie en base, donc rien ne peut partir dans la synchronisation entre machines.
//! Cote interface, un projet ne retient qu'un NOM de contexte et un NOM de namespace.
//!
//! **CE MODULE NE LIT NI L'ENVIRONNEMENT NI LE DISQUE DANS SA PARTIE DECISIVE.** Les fonctions
//! qui decident prennent le texte des fichiers et rendent des structures : c'est ce qui les rend
//! eprouvables, meme decoupe que `terminal::environnement::modifications` et
//! `chemins::resoudre_dossier_personnel`.

use base64::Engine;
use serde::Deserialize;
use std::path::PathBuf;

/// Un contexte tel qu'il s'affiche dans le selecteur : un nom, et ou il pointe.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Contexte {
    pub nom: String,
    pub cluster: String,
    /// L'adresse du serveur, AFFICHEE dans l'interface : on parle a la production de
    /// quelqu'un, et se tromper de cluster doit etre impossible a ne pas voir.
    pub serveur: String,
    /// Le namespace inscrit dans le contexte, s'il y en a un. Sert de premier choix.
    pub namespace: Option<String>,
    pub courant: bool,
    /// Vide quand Cockpit sait parler a ce cluster. Sinon, la RAISON, affichee telle quelle :
    /// un contexte qu'on ne sait pas servir se dit, il ne se cache pas.
    pub obstacle: Option<String>,
}

/// De quoi construire un client HTTP pour un contexte donne.
#[derive(Debug, Clone, PartialEq)]
pub struct Acces {
    pub serveur: String,
    pub identite: Identite,
    /// L'autorite de certification du cluster, en PEM, quand il en declare une.
    pub autorite: Option<Vec<u8>>,
    /// `insecure-skip-tls-verify` du kubeconfig. On l'obeit, et l'interface le DIT.
    pub sans_verification: bool,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Identite {
    Jeton(String),
    /// Certificat client (PEM), tel que le posent kubeadm et minikube.
    Certificat { pem: Vec<u8> },
}

// --- La forme du fichier -----------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Fichier {
    #[serde(default, rename = "current-context")]
    current_context: Option<String>,
    #[serde(default)]
    contexts: Vec<EntreeContexte>,
    #[serde(default)]
    clusters: Vec<EntreeCluster>,
    #[serde(default)]
    users: Vec<EntreeUtilisateur>,
}

#[derive(Debug, Deserialize)]
struct EntreeContexte {
    name: String,
    context: DetailContexte,
}

#[derive(Debug, Deserialize)]
struct DetailContexte {
    cluster: String,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EntreeCluster {
    name: String,
    cluster: DetailCluster,
}

#[derive(Debug, Deserialize)]
struct DetailCluster {
    #[serde(default)]
    server: Option<String>,
    #[serde(default, rename = "certificate-authority-data")]
    autorite_encodee: Option<String>,
    #[serde(default, rename = "certificate-authority")]
    autorite_fichier: Option<String>,
    #[serde(default, rename = "insecure-skip-tls-verify")]
    sans_verification: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct EntreeUtilisateur {
    name: String,
    user: DetailUtilisateur,
}

#[derive(Debug, Default, Deserialize)]
struct DetailUtilisateur {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, rename = "tokenFile")]
    fichier_jeton: Option<String>,
    #[serde(default, rename = "client-certificate-data")]
    certificat_encode: Option<String>,
    #[serde(default, rename = "client-certificate")]
    certificat_fichier: Option<String>,
    #[serde(default, rename = "client-key-data")]
    cle_encodee: Option<String>,
    #[serde(default, rename = "client-key")]
    cle_fichier: Option<String>,
    /// Un programme externe qui fabrique le jeton (`aws eks get-token`, `gcloud`, oidc-login).
    #[serde(default)]
    exec: Option<serde_yaml_ng::Value>,
    /// L'ancetre de `exec`, encore servi par de vieux fichiers.
    #[serde(default, rename = "auth-provider")]
    fournisseur: Option<serde_yaml_ng::Value>,
}

pub fn lire(texte: &str) -> Result<Fichier, String> {
    serde_yaml_ng::from_str(texte).map_err(|e| format!("kubeconfig illisible : {e}"))
}

// --- Ce que l'interface voit -------------------------------------------------------------

/// Les contextes de TOUS les fichiers, dans l'ordre des fichiers puis de leur declaration.
///
/// **UN CONTEXTE DEJA VU NE SE REMPLACE PAS.** Quand `KUBECONFIG` en enchaine plusieurs, la
/// regle de kubectl est que le PREMIER l'emporte ; le contraire ferait parler au mauvais
/// cluster un utilisateur qui a mis son fichier personnel en tete.
pub fn contextes(fichiers: &[Fichier]) -> Vec<Contexte> {
    let mut vus: Vec<Contexte> = Vec::new();
    for fichier in fichiers {
        for entree in &fichier.contexts {
            if vus.iter().any(|c| c.nom == entree.name) {
                continue;
            }
            let cluster = fichier.clusters.iter().find(|c| c.name == entree.context.cluster);
            let utilisateur = entree
                .context
                .user
                .as_ref()
                .and_then(|nom| fichier.users.iter().find(|u| &u.name == nom));
            let serveur = cluster.and_then(|c| c.cluster.server.clone()).unwrap_or_default();
            let obstacle = obstacle_de(cluster.is_some(), utilisateur.map(|u| &u.user), &serveur);
            vus.push(Contexte {
                nom: entree.name.clone(),
                cluster: entree.context.cluster.clone(),
                serveur,
                namespace: entree.context.namespace.clone(),
                courant: fichiers
                    .iter()
                    .find_map(|f| f.current_context.clone())
                    .is_some_and(|c| c == entree.name),
                obstacle,
            });
        }
    }
    vus
}

/// Pourquoi Cockpit ne saura pas parler a ce contexte, le cas echeant.
///
/// **ON REFUSE EN LE DISANT, ON NE DEVINE PAS.** Un contexte dont l'identite vient d'un
/// programme externe demanderait de lancer ce programme et d'en rejouer le protocole ; tant que
/// ce n'est pas fait, l'annoncer vaut mieux qu'un echec au premier clic.
fn obstacle_de(cluster_connu: bool, utilisateur: Option<&DetailUtilisateur>, serveur: &str) -> Option<String> {
    if !cluster_connu {
        return Some("cluster introuvable dans le kubeconfig".into());
    }
    if serveur.is_empty() {
        return Some("ce cluster n'indique aucune adresse de serveur".into());
    }
    let Some(u) = utilisateur else {
        return Some("aucune identite associee a ce contexte".into());
    };
    if u.exec.is_some() {
        return Some("identite fournie par un programme externe (exec), pas encore geree".into());
    }
    if u.fournisseur.is_some() {
        return Some("identite fournie par un auth-provider, pas encore geree".into());
    }
    let a_un_jeton = u.token.is_some() || u.fichier_jeton.is_some();
    let a_un_certificat = (u.certificat_encode.is_some() || u.certificat_fichier.is_some())
        && (u.cle_encodee.is_some() || u.cle_fichier.is_some());
    if !a_un_jeton && !a_un_certificat {
        return Some("ni jeton ni certificat client dans cette identite".into());
    }
    None
}

// --- Ce qu'il faut pour appeler ----------------------------------------------------------

/// Resout un contexte NOMME en de quoi appeler.
///
/// `lire_fichier` est fourni par l'appelant : le kubeconfig peut pointer des fichiers a cote
/// (certificats, jeton), et ce module ne touche pas au disque lui-meme.
pub fn acces(
    fichiers: &[Fichier],
    nom_demande: &str,
    lire_fichier: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Acces, String> {
    let (fichier, entree) = fichiers
        .iter()
        .find_map(|f| f.contexts.iter().find(|c| c.name == nom_demande).map(|c| (f, c)))
        .ok_or_else(|| format!("contexte inconnu : {nom_demande}"))?;

    let cluster = fichier
        .clusters
        .iter()
        .find(|c| c.name == entree.context.cluster)
        .ok_or_else(|| format!("cluster inconnu : {}", entree.context.cluster))?;
    let serveur = cluster
        .cluster
        .server
        .clone()
        .ok_or_else(|| format!("le cluster {} n'indique aucun serveur", cluster.name))?;

    let utilisateur = entree
        .context
        .user
        .as_ref()
        .and_then(|nom| fichier.users.iter().find(|u| &u.name == nom))
        .map(|u| &u.user);
    if let Some(raison) = obstacle_de(true, utilisateur, &serveur) {
        return Err(raison);
    }
    let u = utilisateur.ok_or("aucune identite associee a ce contexte")?;

    let autorite = match (&cluster.cluster.autorite_encodee, &cluster.cluster.autorite_fichier) {
        (Some(encodee), _) => Some(decoder(encodee)?),
        (None, Some(chemin)) => Some(lire_fichier(chemin)?),
        _ => None,
    };

    let identite = if let Some(jeton) = &u.token {
        Identite::Jeton(jeton.trim().to_string())
    } else if let Some(chemin) = &u.fichier_jeton {
        Identite::Jeton(String::from_utf8_lossy(&lire_fichier(chemin)?).trim().to_string())
    } else {
        // reqwest veut le certificat et sa cle dans un MEME bloc PEM.
        let mut pem = match (&u.certificat_encode, &u.certificat_fichier) {
            (Some(encode), _) => decoder(encode)?,
            (None, Some(chemin)) => lire_fichier(chemin)?,
            _ => return Err("certificat client absent".into()),
        };
        let mut cle = match (&u.cle_encodee, &u.cle_fichier) {
            (Some(encodee), _) => decoder(encodee)?,
            (None, Some(chemin)) => lire_fichier(chemin)?,
            _ => return Err("cle du certificat client absente".into()),
        };
        if !pem.ends_with(b"\n") {
            pem.push(b'\n');
        }
        pem.append(&mut cle);
        Identite::Certificat { pem }
    };

    Ok(Acces {
        serveur: serveur.trim_end_matches('/').to_string(),
        identite,
        autorite,
        sans_verification: cluster.cluster.sans_verification.unwrap_or(false),
        namespace: entree.context.namespace.clone(),
    })
}

fn decoder(valeur: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(valeur.trim())
        .map_err(|e| format!("valeur encodee illisible dans le kubeconfig : {e}"))
}

// --- Ou vivent les fichiers --------------------------------------------------------------

/// Les fichiers a lire, dans l'ordre : `KUBECONFIG` s'il est pose, sinon `~/.kube/config`.
///
/// `KUBECONFIG` enchaine plusieurs chemins, separes par `:` sous Unix et `;` sous Windows.
pub fn chemins(variable: Option<&str>, dossier_personnel: &std::path::Path) -> Vec<PathBuf> {
    let separateur = if cfg!(windows) { ';' } else { ':' };
    match variable.map(str::trim).filter(|v| !v.is_empty()) {
        Some(valeur) => valeur
            .split(separateur)
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(PathBuf::from)
            .collect(),
        None => vec![dossier_personnel.join(".kube").join("config")],
    }
}

/// Un nom de ressource Kubernetes, tel qu'on accepte de le recoller dans une URL.
///
/// **LE CLIENT ENVOIE UN NOM, ET CE NOM FINIT DANS UN CHEMIN D'URL.** Sans ce controle, un
/// `../` venu de l'interface sortirait du namespace demande et irait interroger autre chose sur
/// le cluster. La regle est celle de Kubernetes lui-meme (RFC 1123), donc rien de legitime n'est
/// refuse. Meme famille que « ne jamais croire une valeur venue du client ».
pub fn nom_valide(nom: &str) -> bool {
    !nom.is_empty()
        && nom.len() <= 253
        && nom
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && !nom.starts_with(['-', '.'])
        && !nom.ends_with(['-', '.'])
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEUX_CONTEXTES: &str = r#"
apiVersion: v1
current-context: prod
contexts:
  - name: prod
    context: { cluster: prod, user: prod, namespace: ccm-main }
  - name: qlf
    context: { cluster: qlf, user: qlf }
clusters:
  - name: prod
    cluster: { server: "https://prod.exemple.test:6443" }
  - name: qlf
    cluster:
      server: "https://qlf.exemple.test"
      insecure-skip-tls-verify: true
users:
  - name: prod
    user: { token: jeton-de-prod }
  - name: qlf
    user: { token: jeton-de-qlf }
"#;

    fn sans_disque() -> impl Fn(&str) -> Result<Vec<u8>, String> {
        |chemin: &str| Err(format!("aucun fichier ne devait etre lu, demande : {chemin}"))
    }

    #[test]
    fn les_contextes_se_lisent_avec_leur_serveur_et_leur_namespace() {
        let f = lire(DEUX_CONTEXTES).unwrap();
        let liste = contextes(&[f]);
        assert_eq!(liste.len(), 2);
        assert_eq!(liste[0].nom, "prod");
        assert_eq!(liste[0].serveur, "https://prod.exemple.test:6443");
        assert_eq!(liste[0].namespace.as_deref(), Some("ccm-main"));
        assert!(liste[0].courant, "current-context designe prod");
        assert!(!liste[1].courant);
        assert!(liste.iter().all(|c| c.obstacle.is_none()), "ces deux-la sont servables");
    }

    #[test]
    fn le_jeton_et_le_namespace_du_contexte_arrivent_dans_l_acces() {
        let f = lire(DEUX_CONTEXTES).unwrap();
        let a = acces(&[f], "prod", &sans_disque()).unwrap();
        assert_eq!(a.serveur, "https://prod.exemple.test:6443");
        assert_eq!(a.identite, Identite::Jeton("jeton-de-prod".into()));
        assert_eq!(a.namespace.as_deref(), Some("ccm-main"));
        assert!(!a.sans_verification);
    }

    #[test]
    fn un_cluster_qui_refuse_la_verification_le_dit_dans_l_acces() {
        let f = lire(DEUX_CONTEXTES).unwrap();
        assert!(acces(&[f], "qlf", &sans_disque()).unwrap().sans_verification);
    }

    #[test]
    fn une_identite_fournie_par_un_programme_externe_est_refusee_avec_sa_raison() {
        // Le cas d'AWS EKS, de gcloud et d'oidc-login. Mieux vaut le dire dans le selecteur
        // que d'echouer au premier clic sur une erreur de reseau incomprehensible.
        let texte = r#"
contexts: [{ name: eks, context: { cluster: eks, user: eks } }]
clusters: [{ name: eks, cluster: { server: "https://eks.exemple.test" } }]
users:
  - name: eks
    user:
      exec: { apiVersion: client.authentication.k8s.io/v1beta1, command: aws }
"#;
        let f = lire(texte).unwrap();
        let liste = contextes(&[f]);
        let obstacle = liste[0].obstacle.as_deref().unwrap_or_default();
        assert!(obstacle.contains("exec"), "la raison doit nommer le mecanisme : {obstacle}");

        let f = lire(texte).unwrap();
        let erreur = acces(&[f], "eks", &sans_disque()).unwrap_err();
        assert!(erreur.contains("exec"), "et l'appel doit refuser pareil : {erreur}");
    }

    #[test]
    fn une_identite_sans_jeton_ni_certificat_est_refusee() {
        let texte = r#"
contexts: [{ name: vide, context: { cluster: c, user: u } }]
clusters: [{ name: c, cluster: { server: "https://c.exemple.test" } }]
users: [{ name: u, user: {} }]
"#;
        let f = lire(texte).unwrap();
        assert!(contextes(&[f]).unwrap_premier().obstacle.is_some());
    }

    #[test]
    fn le_certificat_client_est_recolle_avec_sa_cle() {
        // reqwest veut un seul bloc PEM. Les deux moities arrivent encodees dans le fichier.
        let cert = base64::engine::general_purpose::STANDARD.encode(b"-----CERT-----");
        let cle = base64::engine::general_purpose::STANDARD.encode(b"-----CLE-----");
        let texte = format!(
            r#"
contexts: [{{ name: local, context: {{ cluster: c, user: u }} }}]
clusters: [{{ name: c, cluster: {{ server: "https://c.exemple.test" }} }}]
users: [{{ name: u, user: {{ client-certificate-data: {cert}, client-key-data: {cle} }} }}]
"#
        );
        let f = lire(&texte).unwrap();
        let a = acces(&[f], "local", &sans_disque()).unwrap();
        match a.identite {
            Identite::Certificat { pem } => {
                let texte = String::from_utf8(pem).unwrap();
                assert!(texte.contains("-----CERT-----") && texte.contains("-----CLE-----"));
                assert!(texte.contains("-----CERT-----\n-----CLE-----"), "un saut separe les deux");
            }
            autre => panic!("certificat attendu, obtenu {autre:?}"),
        }
    }

    #[test]
    fn un_jeton_range_dans_un_fichier_est_lu_par_l_appelant() {
        let texte = r#"
contexts: [{ name: c, context: { cluster: c, user: u } }]
clusters: [{ name: c, cluster: { server: "https://c.exemple.test" } }]
users: [{ name: u, user: { tokenFile: /var/run/secrets/token } }]
"#;
        let f = lire(texte).unwrap();
        let a = acces(&[f], "c", &|chemin| {
            assert_eq!(chemin, "/var/run/secrets/token");
            Ok(b"  jeton-du-fichier\n".to_vec())
        })
        .unwrap();
        assert_eq!(a.identite, Identite::Jeton("jeton-du-fichier".into()), "les blancs sautent");
    }

    #[test]
    fn le_premier_fichier_l_emporte_quand_un_nom_revient() {
        // La regle de kubectl. L'inverse ferait parler au mauvais cluster.
        let perso = lire(
            r#"
contexts: [{ name: prod, context: { cluster: a, user: a } }]
clusters: [{ name: a, cluster: { server: "https://le-bon.exemple.test" } }]
users: [{ name: a, user: { token: x } }]
"#,
        )
        .unwrap();
        let autre = lire(
            r#"
contexts: [{ name: prod, context: { cluster: b, user: b } }]
clusters: [{ name: b, cluster: { server: "https://l-autre.exemple.test" } }]
users: [{ name: b, user: { token: y } }]
"#,
        )
        .unwrap();
        let liste = contextes(&[perso, autre]);
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].serveur, "https://le-bon.exemple.test");
    }

    #[test]
    fn les_chemins_suivent_kubeconfig_puis_le_dossier_personnel() {
        let maison = std::path::Path::new("/maison");
        assert_eq!(chemins(None, maison), vec![PathBuf::from("/maison/.kube/config")]);
        assert_eq!(chemins(Some("   "), maison), vec![PathBuf::from("/maison/.kube/config")]);
        let separateur = if cfg!(windows) { ";" } else { ":" };
        let valeur = format!("/a/config{separateur}/b/config");
        assert_eq!(
            chemins(Some(&valeur), maison),
            vec![PathBuf::from("/a/config"), PathBuf::from("/b/config")]
        );
    }

    #[test]
    fn un_nom_de_ressource_ne_peut_pas_sortir_de_son_chemin() {
        assert!(nom_valide("ccm-ccmadmin-main"));
        assert!(nom_valide("web-5cb5677dcc-8ms7z"));
        assert!(!nom_valide("../secrets"));
        assert!(!nom_valide("ccm/main"));
        assert!(!nom_valide("MAJUSCULES"));
        assert!(!nom_valide(""));
        assert!(!nom_valide("-tiret-au-debut"));
        assert!(!nom_valide(&"a".repeat(254)));
    }

    /// Petit confort de lecture pour les essais.
    trait Premier {
        fn unwrap_premier(self) -> Contexte;
    }
    impl Premier for Vec<Contexte> {
        fn unwrap_premier(self) -> Contexte {
            self.into_iter().next().expect("au moins un contexte")
        }
    }
}
