//! Ce que Cockpit montre d'un ticket, et la lecture des reponses de l'API REST v2.
//!
//! Les structures `*Jira` collent au format de Jira et restent privees : l'interface ne voit
//! que les notres, plates et deja traduites en champs simples.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Ticket {
    pub cle: String,
    pub resume: String,
    pub description: String,
    pub statut: String,
    /// `new`, `indeterminate` ou `done` : la categorie que Jira donne au statut, stable
    /// d'un workflow a l'autre la ou les noms de statut ne le sont pas.
    pub categorie_statut: String,
    pub type_ticket: String,
    pub priorite: String,
    pub projet: String,
    pub maj_le: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Transition {
    pub id: String,
    pub nom: String,
    pub vers: String,
    pub categorie_vers: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Commentaire {
    pub auteur: String,
    pub cree_le: String,
    pub corps: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TypeTicket {
    pub id: String,
    pub nom: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetailTicket {
    pub ticket: Ticket,
    pub commentaires: Vec<Commentaire>,
}

#[derive(Debug, Deserialize)]
pub struct Moi {
    pub name: String,
    #[serde(default, rename = "displayName")]
    pub nom_affiche: String,
}

// --- Ce que Jira envoie ---

#[derive(Deserialize)]
struct Nomme {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct Categorie {
    #[serde(default)]
    key: String,
}

#[derive(Deserialize)]
struct StatutJira {
    #[serde(default)]
    name: String,
    #[serde(rename = "statusCategory")]
    categorie: Option<Categorie>,
}

#[derive(Deserialize)]
struct ProjetJira {
    #[serde(default)]
    key: String,
}

#[derive(Deserialize)]
struct Auteur {
    #[serde(default, rename = "displayName")]
    nom: String,
}

#[derive(Deserialize)]
struct CommentaireJira {
    author: Option<Auteur>,
    #[serde(default)]
    created: String,
    #[serde(default)]
    body: String,
}

#[derive(Deserialize)]
struct CommentairesJira {
    #[serde(default)]
    comments: Vec<CommentaireJira>,
}

#[derive(Deserialize)]
struct Champs {
    #[serde(default)]
    summary: String,
    description: Option<String>,
    status: Option<StatutJira>,
    issuetype: Option<Nomme>,
    priority: Option<Nomme>,
    project: Option<ProjetJira>,
    #[serde(default)]
    updated: String,
    comment: Option<CommentairesJira>,
}

#[derive(Deserialize)]
struct IssueJira {
    key: String,
    fields: Champs,
}

#[derive(Deserialize)]
struct RechercheJira {
    #[serde(default)]
    issues: Vec<IssueJira>,
}

#[derive(Deserialize)]
struct CibleJira {
    #[serde(default)]
    name: String,
    #[serde(rename = "statusCategory")]
    categorie: Option<Categorie>,
}

#[derive(Deserialize)]
struct TransitionJira {
    id: String,
    #[serde(default)]
    name: String,
    to: Option<CibleJira>,
}

#[derive(Deserialize)]
struct TransitionsJira {
    #[serde(default)]
    transitions: Vec<TransitionJira>,
}

#[derive(Deserialize)]
struct TypeJira {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    subtask: bool,
}

#[derive(Deserialize)]
struct TypesJira {
    #[serde(default)]
    values: Vec<TypeJira>,
}

#[derive(Deserialize)]
struct TicketCreeJira {
    key: String,
}

fn lire<T: serde::de::DeserializeOwned>(corps: &str) -> Result<T, String> {
    serde_json::from_str(corps).map_err(|e| {
        log::warn!("jira : reponse illisible — {e}");
        format!("reponse de Jira illisible : {e}")
    })
}

fn vers_ticket(i: IssueJira, base: &str) -> (Ticket, Vec<Commentaire>) {
    let f = i.fields;
    let (statut, categorie_statut) = match f.status {
        Some(s) => (s.name, s.categorie.map(|c| c.key).unwrap_or_default()),
        None => (String::new(), String::new()),
    };
    let commentaires = f
        .comment
        .map(|c| c.comments)
        .unwrap_or_default()
        .into_iter()
        .map(|c| Commentaire {
            auteur: c.author.map(|a| a.nom).unwrap_or_default(),
            cree_le: c.created,
            corps: c.body,
        })
        .collect();
    let ticket = Ticket {
        url: format!("{base}/browse/{}", i.key),
        cle: i.key,
        resume: f.summary,
        description: f.description.unwrap_or_default(),
        statut,
        categorie_statut,
        type_ticket: f.issuetype.map(|t| t.name).unwrap_or_default(),
        priorite: f.priority.map(|p| p.name).unwrap_or_default(),
        projet: f.project.map(|p| p.key).unwrap_or_default(),
        maj_le: f.updated,
    };
    (ticket, commentaires)
}

pub fn lire_recherche(corps: &str, base: &str) -> Result<Vec<Ticket>, String> {
    let r: RechercheJira = lire(corps)?;
    Ok(r.issues.into_iter().map(|i| vers_ticket(i, base).0).collect())
}

pub fn lire_ticket(corps: &str, base: &str) -> Result<DetailTicket, String> {
    let (ticket, commentaires) = vers_ticket(lire(corps)?, base);
    Ok(DetailTicket { ticket, commentaires })
}

pub fn lire_transitions(corps: &str) -> Result<Vec<Transition>, String> {
    let t: TransitionsJira = lire(corps)?;
    Ok(t.transitions
        .into_iter()
        .map(|t| {
            let (vers, categorie_vers) = match t.to {
                Some(c) => (c.name, c.categorie.map(|k| k.key).unwrap_or_default()),
                None => (String::new(), String::new()),
            };
            Transition { id: t.id, nom: t.name, vers, categorie_vers }
        })
        .collect())
}

/// Les sous-taches demandent un parent : on ne les propose pas a la creation.
pub fn lire_types(corps: &str) -> Result<Vec<TypeTicket>, String> {
    let t: TypesJira = lire(corps)?;
    Ok(t.values
        .into_iter()
        .filter(|t| !t.subtask)
        .map(|t| TypeTicket { id: t.id, nom: t.name })
        .collect())
}

pub fn lire_moi(corps: &str) -> Result<Moi, String> {
    lire(corps)
}

pub fn lire_ticket_cree(corps: &str) -> Result<String, String> {
    Ok(lire::<TicketCreeJira>(corps)?.key)
}

/// La premiere transition qui mene a un statut « en cours », quel que soit son nom.
pub fn transition_en_cours(t: &[Transition]) -> Option<&Transition> {
    t.iter().find(|t| t.categorie_vers == "indeterminate")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://jira.exemple.org";

    const RECHERCHE: &str = r#"{"issues":[
      {"key":"CCM-1234","fields":{"summary":"Correction login",
        "status":{"name":"En cours","statusCategory":{"key":"indeterminate"}},
        "issuetype":{"name":"Bug"},"priority":{"name":"Major"},"project":{"key":"CCM"},
        "updated":"2026-09-25T10:12:00.000+0200"}},
      {"key":"ABC-7","fields":{"summary":"Sans priorite",
        "status":{"name":"A faire","statusCategory":{"key":"new"}},
        "issuetype":{"name":"Story"},"priority":null,"project":{"key":"ABC"},
        "updated":"2026-09-20T08:00:00.000+0200"}}]}"#;

    const TICKET: &str = r#"{"key":"CCM-1234","fields":{"summary":"Correction login",
      "description":"Le bouton ne repond pas",
      "status":{"name":"En cours","statusCategory":{"key":"indeterminate"}},
      "issuetype":{"name":"Bug"},"priority":{"name":"Major"},"project":{"key":"CCM"},
      "updated":"2026-09-25T10:12:00.000+0200",
      "comment":{"comments":[{"author":{"displayName":"Ada"},"created":"2026-09-24T09:00:00.000+0200","body":"Vu en QLF"}]}}}"#;

    const TRANSITIONS: &str = r#"{"transitions":[
      {"id":"11","name":"A faire","to":{"name":"A faire","statusCategory":{"key":"new"}}},
      {"id":"21","name":"Commencer","to":{"name":"En cours","statusCategory":{"key":"indeterminate"}}},
      {"id":"31","name":"Terminer","to":{"name":"Termine","statusCategory":{"key":"done"}}}]}"#;

    const TYPES: &str = r#"{"values":[
      {"id":"1","name":"Bug","subtask":false},
      {"id":"5","name":"Sous-tache","subtask":true},
      {"id":"10","name":"Story","subtask":false}]}"#;

    #[test]
    fn lit_une_recherche() {
        let t = lire_recherche(RECHERCHE, BASE).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].cle, "CCM-1234");
        assert_eq!(t[0].categorie_statut, "indeterminate");
        assert_eq!(t[0].type_ticket, "Bug");
        assert_eq!(t[0].projet, "CCM");
        assert_eq!(t[0].url, "https://jira.exemple.org/browse/CCM-1234");
        assert_eq!(t[1].priorite, "", "une priorite nulle devient vide");
    }

    #[test]
    fn lit_un_ticket_et_ses_commentaires() {
        let d = lire_ticket(TICKET, BASE).unwrap();
        assert_eq!(d.ticket.description, "Le bouton ne repond pas");
        assert_eq!(d.commentaires.len(), 1);
        assert_eq!(d.commentaires[0].auteur, "Ada");
        assert_eq!(d.commentaires[0].corps, "Vu en QLF");
    }

    #[test]
    fn lit_les_transitions_et_choisit_en_cours() {
        let t = lire_transitions(TRANSITIONS).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t[1].vers, "En cours");
        assert_eq!(transition_en_cours(&t).unwrap().id, "21");
        assert!(transition_en_cours(&t[2..]).is_none());
    }

    #[test]
    fn ignore_les_sous_taches() {
        let t = lire_types(TYPES).unwrap();
        assert_eq!(t.iter().map(|x| x.nom.as_str()).collect::<Vec<_>>(), vec!["Bug", "Story"]);
    }

    #[test]
    fn lit_moi_et_le_ticket_cree() {
        let m = lire_moi(r#"{"name":"tlegendre","displayName":"T. Legendre"}"#).unwrap();
        assert_eq!(m.name, "tlegendre");
        assert_eq!(m.nom_affiche, "T. Legendre");
        assert_eq!(lire_ticket_cree(r#"{"id":"1","key":"CCM-42","self":"x"}"#).unwrap(), "CCM-42");
    }

    #[test]
    fn une_reponse_illisible_est_une_erreur_lisible() {
        let e = lire_recherche("<html>proxy</html>", BASE).unwrap_err();
        assert!(e.contains("illisible"), "{e}");
    }
}
