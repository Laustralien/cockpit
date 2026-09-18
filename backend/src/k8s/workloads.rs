//! Ce qui est DECLARE dans un namespace, meme quand rien ne tourne.
//!
//! **UNE LISTE DE PODS N'EST PAS UNE LISTE DE SERVICES.** Signale par le mainteneur : un travail
//! planifie cree la veille, jamais encore declenche, n'existait nulle part dans Cockpit alors
//! que son interface web le montrait. Normal : il n'a aucun pod. Or c'est justement ce qu'on
//! vient verifier quand on le cherche — « est-ce qu'il est bien la, et pourquoi n'a-t-il rien
//! fait ». Un namespace reel compte 79 travaux planifies pour 264 pods de travaux : la liste des
//! pods en oublie une partie, et en montre surtout le passe.
//!
//! Mesure avant d'ecrire : 12 deploiements en 104 ms, 79 travaux planifies en 383 ms, le reste
//! a zero. C'est une lecture par ouverture d'ecran, pas une lecture sur minuteur.

use serde::Serialize;
use serde_json::Value;

/// Un objet declare, reduit a ce que l'ecran affiche.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Workload {
    pub nom: String,
    /// `Deployment`, `CronJob`, `StatefulSet`, `DaemonSet`.
    pub sorte: String,
    /// Le tag de l'image : la version livree.
    pub version: String,
    /// Ce que la declaration demande, et ce qui repond. Vide de sens pour un travail planifie.
    pub voulus: u32,
    pub prets: u32,
    /// Un travail planifie suspendu ne se declenchera pas : c'est une information, pas une panne.
    pub suspendu: bool,
    /// La planification (`10 3 * * *`), vide pour le reste.
    pub planification: String,
    /// Le dernier declenchement, en ISO. `None` veut dire « jamais » et doit se voir.
    pub dernier: Option<String>,
    /// Quand l'objet a ete cree.
    pub depuis: Option<String>,
}

/// Les chemins d'API a lire, avec la sorte qu'ils rendent.
pub const SOURCES: &[(&str, &str)] = &[
    ("apis/apps/v1", "deployments"),
    ("apis/batch/v1", "cronjobs"),
    ("apis/apps/v1", "statefulsets"),
    ("apis/apps/v1", "daemonsets"),
];

/// La sorte affichee, au singulier, telle que le reste de l'ecran la nomme.
pub fn sorte_de(ressource: &str) -> &'static str {
    match ressource {
        "deployments" => "Deployment",
        "cronjobs" => "CronJob",
        "statefulsets" => "StatefulSet",
        "daemonsets" => "DaemonSet",
        _ => "",
    }
}

/// Reduit un objet declare a sa ligne d'ecran.
pub fn reduire(objet: &Value, sorte: &str) -> Workload {
    let texte = |chemin: &str| {
        objet
            .pointer(chemin)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let nombre = |chemin: &str| objet.pointer(chemin).and_then(Value::as_u64).unwrap_or(0) as u32;

    // L'image vit a un endroit different selon la sorte : un travail planifie enveloppe son
    // gabarit de travail, qui enveloppe son gabarit de pod.
    let image = objet
        .pointer("/spec/template/spec/containers/0/image")
        .or_else(|| objet.pointer("/spec/jobTemplate/spec/template/spec/containers/0/image"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    let (voulus, prets) = match sorte {
        "Deployment" | "StatefulSet" => (nombre("/spec/replicas"), nombre("/status/readyReplicas")),
        "DaemonSet" => (
            nombre("/status/desiredNumberScheduled"),
            nombre("/status/numberReady"),
        ),
        // Un travail planifie n'a pas de « replicas » : ce qui compte est ce qui tourne a
        // l'instant, et le nombre de pods gardes, que la liste des pods dira.
        _ => (0, 0),
    };

    Workload {
        nom: texte("/metadata/name"),
        sorte: sorte.to_string(),
        version: super::modele::version_depuis_image(image),
        voulus,
        prets,
        suspendu: objet
            .pointer("/spec/suspend")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        planification: texte("/spec/schedule"),
        dernier: objet
            .pointer("/status/lastScheduleTime")
            .and_then(Value::as_str)
            .map(str::to_string),
        depuis: objet
            .pointer("/metadata/creationTimestamp")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn un_deploiement_dit_ce_qu_il_veut_et_ce_qui_repond() {
        let objet = json!({
            "metadata": { "name": "web", "creationTimestamp": "2026-01-02T03:04:05Z" },
            "spec": {
                "replicas": 4,
                "template": { "spec": { "containers": [{ "image": "depot/appli:v2-8f31c07" }] } }
            },
            "status": { "readyReplicas": 3 }
        });
        let w = reduire(&objet, "Deployment");
        assert_eq!((w.voulus, w.prets), (4, 3));
        assert_eq!(w.version, "v2-8f31c07");
        assert!(w.planification.is_empty());
        assert!(!w.suspendu);
    }

    #[test]
    fn un_travail_planifie_jamais_declenche_le_dit() {
        // **LE CAS QUI A MOTIVE CE MODULE.** Il n'a aucun pod, donc il etait invisible.
        let objet = json!({
            "metadata": { "name": "game-daily-snapshot", "creationTimestamp": "2026-09-18T06:00:00Z" },
            "spec": {
                "schedule": "10 3 * * *",
                "jobTemplate": { "spec": { "template": { "spec": {
                    "containers": [{ "image": "depot/appli:main-72188cb" }]
                } } } }
            },
            "status": {}
        });
        let w = reduire(&objet, "CronJob");
        assert_eq!(w.nom, "game-daily-snapshot");
        assert_eq!(w.planification, "10 3 * * *");
        assert_eq!(w.dernier, None, "jamais declenche, et ca doit se voir");
        assert_eq!(w.version, "main-72188cb", "l'image vient du gabarit de travail");
    }

    #[test]
    fn un_travail_suspendu_se_distingue_d_un_travail_en_panne() {
        let objet = json!({
            "metadata": { "name": "backfill" },
            "spec": { "schedule": "0 0 31 2 *", "suspend": true },
            "status": {}
        });
        let w = reduire(&objet, "CronJob");
        assert!(w.suspendu);
        assert_eq!(w.dernier, None);
    }

    #[test]
    fn un_daemonset_compte_ses_machines() {
        let objet = json!({
            "metadata": { "name": "collecteur" },
            "spec": { "template": { "spec": { "containers": [{ "image": "depot/agent:1.4" }] } } },
            "status": { "desiredNumberScheduled": 7, "numberReady": 7 }
        });
        let w = reduire(&objet, "DaemonSet");
        assert_eq!((w.voulus, w.prets), (7, 7));
    }

    #[test]
    fn un_objet_sans_rien_ne_fait_pas_tomber_la_lecture() {
        let w = reduire(&json!({}), "Deployment");
        assert_eq!(w.nom, "");
        assert_eq!((w.voulus, w.prets), (0, 0));
        assert_eq!(w.version, "");
    }

    #[test]
    fn chaque_ressource_a_son_nom_affiche() {
        assert_eq!(sorte_de("cronjobs"), "CronJob");
        assert_eq!(sorte_de("deployments"), "Deployment");
        assert_eq!(sorte_de("inconnu"), "");
    }
}
