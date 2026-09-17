//! Les logs qui defilent : un flux ouvert sur le conteneur, arrete des qu'on ferme la vue.
//!
//! **UN SEUL APPEL FAIT LES DEUX.** `follow=true&tailLines=N` envoie d'abord les N dernieres
//! lignes, puis tout ce qui s'ecrit ensuite : on ouvre donc la vue sur la FIN des logs, ce qui
//! est ce qu'on vient y chercher, sans avoir a demander deux fois la meme chose.
//!
//! **UN POD SILENCIEUX NE REND JAMAIS LA MAIN, ET C'EST NORMAL.** Le flux reste ouvert sans un
//! octet pendant des minutes. Une lecture nue y attendrait pour toujours, meme apres la
//! fermeture de la vue : l'attente est donc mise en concurrence avec un signal d'arret.

use crate::evenements::Emetteurs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

/// Ce que l'ecran ecoute.
pub const LIGNES: &str = "k8s_log";
pub const FIN: &str = "k8s_log_fin";

/// Le flux en cours. Un seul : une seule vue de logs est ouverte a la fois.
#[derive(Default)]
pub struct SuiviDesLogs {
    arret: std::sync::Mutex<Option<(Arc<AtomicBool>, Arc<Notify>)>>,
}

impl SuiviDesLogs {
    pub fn arreter(&self) {
        if let Some((drapeau, reveil)) = self.arret.lock().ok().and_then(|mut v| v.take()) {
            drapeau.store(true, Ordering::Relaxed);
            // **SANS CE REVEIL, LA TACHE RESTE BLOQUEE SUR UN POD MUET.** Poser un drapeau ne
            // suffit pas : personne ne le lit tant qu'aucun octet n'arrive.
            reveil.notify_waiters();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn demarrer(
        &self,
        emetteur: Emetteurs,
        contexte: String,
        namespace: String,
        pod: String,
        conteneur: Option<String>,
        lignes: u32,
    ) -> Result<(), String> {
        for nom in [&namespace, &pod] {
            if !super::kubeconfig::nom_valide(nom) {
                return Err(format!("nom refuse : {nom}"));
            }
        }
        self.arreter();
        let drapeau = Arc::new(AtomicBool::new(false));
        let reveil = Arc::new(Notify::new());
        *self.arret.lock().map_err(|_| "suivi des logs inaccessible")? =
            Some((drapeau.clone(), reveil.clone()));

        tokio::spawn(boucle(
            emetteur, contexte, namespace, pod, conteneur, lignes, drapeau, reveil,
        ));
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn boucle(
    emetteur: Emetteurs,
    contexte: String,
    namespace: String,
    pod: String,
    conteneur: Option<String>,
    lignes: u32,
    arret: Arc<AtomicBool>,
    reveil: Arc<Notify>,
) {
    let fin = |raison: Option<String>| {
        emetteur.emettre(
            FIN,
            match raison {
                Some(r) => serde_json::json!({ "raison": r }),
                None => serde_json::json!({ "raison": null }),
            },
        );
    };

    let client = match super::client_de(&contexte) {
        Ok((c, _)) => c,
        Err(e) => return fin(Some(e)),
    };
    let mut chemin = format!(
        "/api/v1/namespaces/{namespace}/pods/{pod}/log?follow=true&timestamps=true&tailLines={}",
        lignes.clamp(1, 5_000)
    );
    if let Some(c) = conteneur.filter(|c| super::kubeconfig::nom_valide(c)) {
        chemin.push_str(&format!("&container={c}"));
    }

    let mut reponse = match client.ouvrir_le_flux(&chemin).await {
        Ok(r) => r,
        Err(e) => return fin(Some(e)),
    };

    loop {
        let morceau = tokio::select! {
            m = reponse.chunk() => m,
            _ = reveil.notified() => return,
        };
        if arret.load(Ordering::Relaxed) {
            // **UN ARRET DEMANDE N'EST PAS UNE FIN.** `FIN` dit « le conteneur a cesse
            // d'ecrire » ; le dire quand c'est NOUS qui fermons ferait afficher « arrete » a
            // une vue qui vient de rouvrir un flux vivant, juste a cote.
            return;
        }
        match morceau {
            Ok(Some(octets)) => {
                let texte = String::from_utf8_lossy(&octets).to_string();
                if !texte.is_empty() {
                    emetteur.emettre(LIGNES, serde_json::json!(texte));
                }
            }
            // Le conteneur s'est arrete : le cluster ferme le flux. Ce n'est pas une panne.
            Ok(None) => return fin(None),
            Err(e) => return fin(Some(format!("flux des logs interrompu : {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
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

    #[test]
    fn un_nom_douteux_n_ouvre_aucun_flux() {
        let suivi = SuiviDesLogs::default();
        let emetteur: Emetteurs = Arc::new(Espion::default());
        let erreur = suivi
            .demarrer(emetteur, "ctx".into(), "ns".into(), "../autre".into(), None, 100)
            .unwrap_err();
        assert!(erreur.contains("refuse"), "{erreur}");
    }

    #[tokio::test]
    async fn arreter_reveille_une_lecture_qui_attend() {
        // **C'EST LA GARDE QUI COMPTE ICI.** Un pod muet laisse la tache bloquee sur sa
        // lecture : sans reveil, elle ne verrait jamais le drapeau et survivrait a la
        // fermeture de la vue, connexion comprise.
        let suivi = SuiviDesLogs::default();
        let drapeau = Arc::new(AtomicBool::new(false));
        let reveil = Arc::new(Notify::new());
        *suivi.arret.lock().unwrap() = Some((drapeau.clone(), reveil.clone()));

        let attente = {
            let reveil = reveil.clone();
            tokio::spawn(async move { reveil.notified().await })
        };
        // Laisse l'attente s'installer avant de la reveiller.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        suivi.arreter();

        let fini = tokio::time::timeout(std::time::Duration::from_secs(2), attente).await;
        assert!(fini.is_ok(), "la lecture qui attendait doit etre reveillee");
        assert!(drapeau.load(Ordering::Relaxed), "et le drapeau pose");
    }

    #[test]
    fn un_second_suivi_arrete_le_premier() {
        let suivi = SuiviDesLogs::default();
        let premier = Arc::new(AtomicBool::new(false));
        *suivi.arret.lock().unwrap() = Some((premier.clone(), Arc::new(Notify::new())));
        suivi.arreter();
        assert!(premier.load(Ordering::Relaxed));
    }
}
