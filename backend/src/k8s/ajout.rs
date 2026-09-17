//! Ajouter un cluster : coller ce que Rancher donne, Cockpit s'occupe de la fusion.
//!
//! **POURQUOI CETTE FONCTION EXISTE.** Recuperer l'acces a un cluster, c'est aujourd'hui :
//! ouvrir son interface web, telecharger un fichier, puis le recoller a la main dans
//! `~/.kube/config` sans casser ce qu'il contient deja. Chacun le refait dans son coin, et le
//! premier accident est silencieux : on ecrase l'acces a un AUTRE cluster et on ne s'en rend
//! compte que le jour ou on en a besoin.
//!
//! **ON ECRIT DANS LE FICHIER STANDARD, PAS DANS UN COIN A NOUS.** `kubectl`, `k9s` et tout le
//! reste lisent le meme fichier : un stockage parallele obligerait a tout refaire deux fois, et
//! ferait de Cockpit le detenteur de jetons d'acces a des productions. Ici il ne detient rien de
//! plus qu'avant, il range.
//!
//! Trois regles qui ne se discutent pas :
//! **(1)** on n'ECRASE jamais un acces existant : un nom deja pris est renomme ;
//! **(2)** on ne change PAS le contexte courant : un `kubectl delete` tape dans un terminal
//! partirait alors sur un cluster qu'on n'a pas choisi ;
//! **(3)** aucun message d'erreur ne recopie le fichier : il contient un jeton.

use serde_yaml_ng::{Mapping, Value};

/// Ce qui a ete ajoute, pour le dire a l'utilisateur.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Ajout {
    /// Les contextes desormais disponibles, tels qu'ils s'appellent APRES fusion.
    pub ajoutes: Vec<String>,
    /// Ceux qui existaient deja a l'identique : rien n'a bouge pour eux.
    pub deja_la: Vec<String>,
    /// Ceux dont l'identite a ete remplacee : un jeton renouvele, le meme cluster.
    pub renouveles: Vec<String>,
    /// Ceux qu'on a du renommer, parce que le nom etait pris par un AUTRE cluster.
    pub renommes: Vec<String>,
}

/// Lit un kubeconfig colle, sans jamais recopier son contenu dans l'erreur.
pub fn lire_sans_fuite(texte: &str) -> Result<Mapping, String> {
    let valeur: Value = serde_yaml_ng::from_str(texte).map_err(|e| {
        // **LE MESSAGE DE LA BIBLIOTHEQUE CITE LA LIGNE FAUTIVE, DONC PARFOIS LE JETON.**
        // On ne garde que l'endroit.
        match e.location() {
            Some(ou) => format!("fichier illisible a la ligne {}, colonne {}", ou.line(), ou.column()),
            None => "fichier illisible : ce n'est pas un kubeconfig".to_string(),
        }
    })?;
    match valeur {
        Value::Mapping(m) => Ok(m),
        _ => Err("ce n'est pas un kubeconfig : on attend une configuration, pas une liste".into()),
    }
}

fn liste<'a>(config: &'a Mapping, nom: &str) -> Vec<&'a Value> {
    config
        .get(Value::from(nom))
        .and_then(Value::as_sequence)
        .map(|s| s.iter().collect())
        .unwrap_or_default()
}

fn nom_de(entree: &Value) -> Option<String> {
    entree.get("name").and_then(Value::as_str).map(str::to_string)
}

fn serveur_de(cluster: &Value) -> Option<String> {
    cluster
        .get("cluster")
        .and_then(|c| c.get("server"))
        .and_then(Value::as_str)
        .map(|s| s.trim_end_matches('/').to_string())
}

/// Fusionne `ajout` dans `existant` et rend le fichier complet, plus ce qui s'est passe.
pub fn fusionner(existant: Option<&str>, ajout: &str) -> Result<(String, Ajout), String> {
    let mut base = match existant.map(str::trim).filter(|t| !t.is_empty()) {
        Some(texte) => lire_sans_fuite(texte)?,
        None => Mapping::new(),
    };
    let neuf = lire_sans_fuite(ajout)?;

    let contextes_neufs = liste(&neuf, "contexts");
    if contextes_neufs.is_empty() {
        return Err("ce fichier ne contient aucun contexte : ce n'est pas un kubeconfig".into());
    }

    let mut resultat = Ajout::default();
    let mut clusters: Vec<Value> = liste(&base, "clusters").into_iter().cloned().collect();
    let mut users: Vec<Value> = liste(&base, "users").into_iter().cloned().collect();
    let mut contextes: Vec<Value> = liste(&base, "contexts").into_iter().cloned().collect();

    for contexte_neuf in &contextes_neufs {
        let Some(nom_contexte) = nom_de(contexte_neuf) else { continue };
        let detail = contexte_neuf.get("context");
        let nom_cluster = detail
            .and_then(|d| d.get("cluster"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let nom_user = detail
            .and_then(|d| d.get("user"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let Some(cluster_neuf) = liste(&neuf, "clusters")
            .into_iter()
            .find(|c| nom_de(c).as_deref() == Some(nom_cluster.as_str()))
            .cloned()
        else {
            return Err(format!("le contexte « {nom_contexte} » designe un cluster absent du fichier"));
        };
        let user_neuf = liste(&neuf, "users")
            .into_iter()
            .find(|u| nom_de(u).as_deref() == Some(nom_user.as_str()))
            .cloned();

        let serveur_neuf = serveur_de(&cluster_neuf);
        let existe = clusters
            .iter()
            .find(|c| nom_de(c).as_deref() == Some(nom_cluster.as_str()));

        // Le meme nom pointe-t-il deja sur le MEME serveur ? Alors c'est le meme cluster.
        let meme_cluster = existe.is_some_and(|c| serveur_de(c) == serveur_neuf);
        let (nom_cluster, nom_user, nom_contexte_final) = if existe.is_none() {
            (nom_cluster.clone(), nom_user.clone(), nom_contexte.clone())
        } else if meme_cluster {
            (nom_cluster.clone(), nom_user.clone(), nom_contexte.clone())
        } else {
            // Un AUTRE cluster porte deja ce nom : on renomme plutot que d'ecraser un acces.
            let suffixe = suffixe_libre(&clusters, &contextes, &nom_cluster, &nom_contexte);
            resultat.renommes.push(format!("{nom_contexte}{suffixe}"));
            (
                format!("{nom_cluster}{suffixe}"),
                format!("{nom_user}{suffixe}"),
                format!("{nom_contexte}{suffixe}"),
            )
        };

        let identique = meme_cluster
            && users
                .iter()
                .find(|u| nom_de(u).as_deref() == Some(nom_user.as_str()))
                .map(|u| u.get("user").cloned())
                == user_neuf.as_ref().map(|u| u.get("user").cloned());

        poser(&mut clusters, renommer(cluster_neuf, &nom_cluster));
        if let Some(u) = user_neuf {
            poser(&mut users, renommer(u, &nom_user));
        }
        let contexte_pose = renommer_contexte(contexte_neuf, &nom_contexte_final, &nom_cluster, &nom_user);
        poser(&mut contextes, contexte_pose);

        if identique {
            resultat.deja_la.push(nom_contexte_final.clone());
        } else if meme_cluster {
            // Le meme cluster, une identite differente : un jeton vient d'etre renouvele.
            resultat.renouveles.push(nom_contexte_final.clone());
        }
        resultat.ajoutes.push(nom_contexte_final);
    }

    base.insert(Value::from("apiVersion"), Value::from("v1"));
    base.insert(Value::from("kind"), Value::from("Config"));
    base.insert(Value::from("clusters"), Value::Sequence(clusters));
    base.insert(Value::from("users"), Value::Sequence(users));
    base.insert(Value::from("contexts"), Value::Sequence(contextes));
    // **LE CONTEXTE COURANT NE BOUGE PAS.** Ajouter un acces ne doit jamais rediriger les
    // commandes que l'utilisateur tape dans son terminal.
    if !base.contains_key(Value::from("current-context")) {
        if let Some(premier) = resultat.ajoutes.first() {
            base.insert(Value::from("current-context"), Value::from(premier.as_str()));
        }
    }

    let texte = serde_yaml_ng::to_string(&Value::Mapping(base))
        .map_err(|_| "impossible d'ecrire le kubeconfig fusionne".to_string())?;
    Ok((texte, resultat))
}

/// Remplace l'entree de meme nom, ou l'ajoute.
fn poser(liste: &mut Vec<Value>, entree: Value) {
    let nom = nom_de(&entree);
    match liste.iter().position(|e| nom_de(e) == nom) {
        Some(i) => liste[i] = entree,
        None => liste.push(entree),
    }
}

fn renommer(mut entree: Value, nom: &str) -> Value {
    if let Value::Mapping(m) = &mut entree {
        m.insert(Value::from("name"), Value::from(nom));
    }
    entree
}

fn renommer_contexte(entree: &Value, nom: &str, cluster: &str, user: &str) -> Value {
    let mut copie = entree.clone();
    if let Value::Mapping(m) = &mut copie {
        m.insert(Value::from("name"), Value::from(nom));
        if let Some(Value::Mapping(detail)) = m.get_mut(Value::from("context")) {
            detail.insert(Value::from("cluster"), Value::from(cluster));
            if !user.is_empty() {
                detail.insert(Value::from("user"), Value::from(user));
            }
        }
    }
    copie
}

/// Le premier suffixe qui ne heurte ni un cluster ni un contexte existant.
fn suffixe_libre(clusters: &[Value], contextes: &[Value], cluster: &str, contexte: &str) -> String {
    for n in 2..100 {
        let suffixe = format!("-{n}");
        let pris = clusters
            .iter()
            .any(|c| nom_de(c) == Some(format!("{cluster}{suffixe}")))
            || contextes
                .iter()
                .any(|c| nom_de(c) == Some(format!("{contexte}{suffixe}")));
        if !pris {
            return suffixe;
        }
    }
    "-neuf".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXISTANT: &str = r#"
apiVersion: v1
kind: Config
current-context: prod
clusters:
  - name: prod
    cluster: { server: "https://prod.exemple.test" }
contexts:
  - name: prod
    context: { cluster: prod, user: prod, namespace: equipe-a }
users:
  - name: prod
    user: { token: jeton-de-prod }
"#;

    const QLF: &str = r#"
apiVersion: v1
kind: Config
current-context: "qlf"
clusters:
  - name: "qlf"
    cluster: { server: "https://qlf.exemple.test" }
contexts:
  - name: "qlf"
    context: { user: "qlf", cluster: "qlf" }
users:
  - name: "qlf"
    user: { token: "jeton-de-qlf" }
"#;

    fn contextes_de(texte: &str) -> Vec<String> {
        let m = lire_sans_fuite(texte).unwrap();
        liste(&m, "contexts").iter().filter_map(|c| nom_de(c)).collect()
    }

    #[test]
    fn un_second_cluster_s_ajoute_sans_toucher_au_premier() {
        let (texte, quoi) = fusionner(Some(EXISTANT), QLF).unwrap();
        assert_eq!(contextes_de(&texte), vec!["prod", "qlf"]);
        assert_eq!(quoi.ajoutes, vec!["qlf"]);
        assert!(quoi.renommes.is_empty());
        // L'acces d'avant est intact, jusqu'a son namespace.
        assert!(texte.contains("jeton-de-prod"));
        assert!(texte.contains("equipe-a"));
    }

    #[test]
    fn le_contexte_courant_ne_change_jamais() {
        // Sinon un `kubectl delete` tape dans un terminal partirait sur le cluster qu'on
        // vient d'ajouter, sans que personne l'ait demande.
        let (texte, _) = fusionner(Some(EXISTANT), QLF).unwrap();
        let m = lire_sans_fuite(&texte).unwrap();
        assert_eq!(
            m.get(Value::from("current-context")).and_then(Value::as_str),
            Some("prod"),
        );
    }

    #[test]
    fn un_nom_deja_pris_par_un_autre_cluster_est_renomme_au_lieu_d_ecraser() {
        // Deux entreprises, deux Rancher, et le meme nom de contexte par hasard. Ecraser
        // ferait perdre l'acces au premier sans un mot, et on ne s'en apercevrait qu'en
        // cherchant a l'utiliser.
        let autre_prod = QLF
            .replace("qlf", "prod")
            .replace("https://prod.exemple.test", "https://ailleurs.exemple.test")
            .replace("jeton-de-prod", "jeton-de-l-autre-boite");
        let (texte, quoi) = fusionner(Some(EXISTANT), &autre_prod).unwrap();
        assert_eq!(quoi.renommes, vec!["prod-2"]);
        assert_eq!(contextes_de(&texte), vec!["prod", "prod-2"]);
        // L'acces d'origine survit, avec son serveur et son jeton.
        assert!(texte.contains("https://prod.exemple.test"));
        assert!(texte.contains("jeton-de-prod"));
        // Et le nouveau est bien la, sous son nom de secours.
        assert!(texte.contains("https://ailleurs.exemple.test"));
        assert!(texte.contains("jeton-de-l-autre-boite"));
    }

    #[test]
    fn recoller_deux_fois_le_meme_fichier_ne_cree_pas_de_doublon() {
        let (une_fois, _) = fusionner(Some(EXISTANT), QLF).unwrap();
        let (deux_fois, quoi) = fusionner(Some(&une_fois), QLF).unwrap();
        assert_eq!(contextes_de(&deux_fois), vec!["prod", "qlf"]);
        assert_eq!(quoi.deja_la, vec!["qlf"], "rien de neuf, et on le dit");
        assert!(quoi.renommes.is_empty());
    }

    #[test]
    fn un_jeton_renouvele_remplace_l_ancien_sans_creer_un_second_contexte() {
        // Le cas courant : le jeton expire, on retelecharge le meme fichier.
        let (avant, _) = fusionner(Some(EXISTANT), QLF).unwrap();
        let renouvele = QLF.replace("jeton-de-qlf", "jeton-tout-neuf");
        let (apres, quoi) = fusionner(Some(&avant), &renouvele).unwrap();
        assert_eq!(contextes_de(&apres), vec!["prod", "qlf"], "toujours un seul contexte qlf");
        assert_eq!(quoi.renouveles, vec!["qlf"]);
        assert!(apres.contains("jeton-tout-neuf"));
        assert!(!apres.contains("jeton-de-qlf"), "l'ancien jeton ne traine pas");
    }

    #[test]
    fn sans_kubeconfig_existant_on_en_fabrique_un() {
        let (texte, quoi) = fusionner(None, QLF).unwrap();
        assert_eq!(quoi.ajoutes, vec!["qlf"]);
        let m = lire_sans_fuite(&texte).unwrap();
        assert_eq!(m.get(Value::from("kind")).and_then(Value::as_str), Some("Config"));
        assert_eq!(
            m.get(Value::from("current-context")).and_then(Value::as_str),
            Some("qlf"),
            "le premier acces devient le courant, puisqu'il n'y en avait aucun",
        );
    }

    #[test]
    fn un_fichier_qui_n_est_pas_un_kubeconfig_est_refuse_clairement() {
        let erreur = fusionner(Some(EXISTANT), "bonjour").unwrap_err();
        assert!(erreur.contains("kubeconfig"), "{erreur}");
        let erreur = fusionner(Some(EXISTANT), "clusters: []").unwrap_err();
        assert!(erreur.contains("aucun contexte"), "{erreur}");
    }

    #[test]
    fn une_erreur_de_lecture_ne_recopie_jamais_le_jeton() {
        // **UN MESSAGE D'ERREUR PART DANS UNE INFOBULLE, UN JOURNAL, UNE CAPTURE D'ECRAN.**
        // La bibliotheque cite volontiers la ligne fautive : ici elle contient un jeton.
        let casse = format!("{QLF}\n  ceci n'est pas: du: yaml: valide:");
        let erreur = fusionner(None, &casse).unwrap_err();
        assert!(!erreur.contains("jeton-de-qlf"), "le message fuite le jeton : {erreur}");
        assert!(erreur.contains("ligne"), "et il dit tout de meme ou regarder : {erreur}");
    }

    #[test]
    fn un_contexte_qui_designe_un_cluster_absent_est_refuse() {
        let bancal = r#"
contexts: [{ name: c, context: { cluster: introuvable, user: u } }]
clusters: []
users: [{ name: u, user: { token: x } }]
"#;
        let erreur = fusionner(None, bancal).unwrap_err();
        assert!(erreur.contains("absent"), "{erreur}");
    }
}
