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
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "displayName")]
    pub nom_affiche: String,
    /// Rempli par Cloud (`/myself`), absent sur Server/DC.
    #[serde(default, rename = "accountId")]
    pub account_id: String,
}

/// La forme d'assignation a envoyer : Cloud identifie le compte par `accountId`, Server/DC
/// par `name`. `assigne_de` choisit d'apres `Jira::cloud()`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Assigne<'a> {
    Nom(&'a str),
    Compte(&'a str),
}

impl Assigne<'_> {
    pub fn valeur(&self) -> serde_json::Value {
        match self {
            Assigne::Nom(n) => serde_json::json!({ "name": n }),
            Assigne::Compte(c) => serde_json::json!({ "accountId": c }),
        }
    }
}

/// `None` si l'identifiant attendu (accountId sur Cloud, name sur Server/DC) est vide : on
/// n'envoie jamais `{"name": ""}` ou `{"accountId": ""}`, que Jira pourrait prendre pour une
/// desassignation ou refuser autrement.
pub fn assigne_de(moi: &Moi, cloud: bool) -> Option<Assigne<'_>> {
    if cloud {
        (!moi.account_id.is_empty()).then_some(Assigne::Compte(&moi.account_id))
    } else {
        (!moi.name.is_empty()).then_some(Assigne::Nom(&moi.name))
    }
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
    /// Cloud seulement (pagination de `/rest/api/3/search/jql`) : absent sur Server/DC.
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
    /// Absent sur Server/DC (`#[serde(default)]` vaut alors `false`, sans consequence : seule
    /// `lire_page_recherche`, utilisee cote Cloud, regarde ce champ).
    #[serde(default, rename = "isLast")]
    is_last: bool,
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
    /// Server/DC : `values`. Cloud : `issueTypes` — meme `createmeta/{projet}/issuetypes`,
    /// forme differente ; sans l'alias, Cloud ne proposait plus aucun type a la creation.
    #[serde(default, alias = "issueTypes")]
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

/// Une page de `/rest/api/3/search/jql` (Cloud) : les tickets, et le jeton de la page
/// suivante s'il en reste une (`None` des que `isLast` vaut vrai, meme si Jira laissait
/// trainer un `nextPageToken` perime).
pub fn lire_page_recherche(corps: &str, base: &str) -> Result<(Vec<Ticket>, Option<String>), String> {
    let r: RechercheJira = lire(corps)?;
    let suite = if r.is_last { None } else { r.next_page_token };
    let tickets = r.issues.into_iter().map(|i| vers_ticket(i, base).0).collect();
    Ok((tickets, suite))
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

// --- Ce que Cockpit envoie ---

/// `duree` au format de Jira : `1h30m`, `2d`, `45m`.
pub fn corps_de_saisie(duree: &str, commentaire: Option<&str>) -> Result<serde_json::Value, String> {
    let duree = duree.trim();
    if duree.is_empty() {
        return Err("duree vide".to_string());
    }
    let mut corps = serde_json::json!({ "timeSpent": duree });
    if let Some(c) = commentaire.map(str::trim).filter(|c| !c.is_empty()) {
        corps["comment"] = serde_json::Value::String(c.to_string());
    }
    Ok(corps)
}

/// `assigne` a `None` : pas de champ `assignee` du tout, pour les instances Jira ou il n'est
/// pas sur l'ecran de creation (voir `refus_du_champ_assignee`).
pub fn corps_de_creation(
    cle_projet: &str,
    type_id: &str,
    resume: &str,
    description: Option<&str>,
    assigne: Option<Assigne>,
) -> Result<serde_json::Value, String> {
    let resume = resume.trim();
    if resume.is_empty() {
        return Err("resume vide".to_string());
    }
    if type_id.trim().is_empty() {
        return Err("type de ticket non choisi".to_string());
    }
    let mut champs = serde_json::json!({
        "project": { "key": cle_projet },
        "issuetype": { "id": type_id.trim() },
        "summary": resume,
    });
    if let Some(a) = assigne {
        champs["assignee"] = a.valeur();
    }
    if let Some(d) = description.map(str::trim).filter(|d| !d.is_empty()) {
        champs["description"] = serde_json::Value::String(d.to_string());
    }
    Ok(serde_json::json!({ "fields": champs }))
}

/// Le POST de creation a echoue a cause du champ `assignee` (absent de l'ecran de creation
/// sur certaines instances Jira) : on peut retenter sans lui, puis assigner a part.
pub fn refus_du_champ_assignee(erreur: &str) -> bool {
    erreur.to_lowercase().contains("assignee")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://jira.exemple.org";

    const RECHERCHE: &str = r#"{"issues":[
      {"key":"PROJ-1234","fields":{"summary":"Correction login",
        "status":{"name":"En cours","statusCategory":{"key":"indeterminate"}},
        "issuetype":{"name":"Bug"},"priority":{"name":"Major"},"project":{"key":"PROJ"},
        "updated":"2026-09-25T10:12:00.000+0200"}},
      {"key":"ABC-7","fields":{"summary":"Sans priorite",
        "status":{"name":"A faire","statusCategory":{"key":"new"}},
        "issuetype":{"name":"Story"},"priority":null,"project":{"key":"ABC"},
        "updated":"2026-09-20T08:00:00.000+0200"}}]}"#;

    const TICKET: &str = r#"{"key":"PROJ-1234","fields":{"summary":"Correction login",
      "description":"Le bouton ne repond pas",
      "status":{"name":"En cours","statusCategory":{"key":"indeterminate"}},
      "issuetype":{"name":"Bug"},"priority":{"name":"Major"},"project":{"key":"PROJ"},
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
        assert_eq!(t[0].cle, "PROJ-1234");
        assert_eq!(t[0].categorie_statut, "indeterminate");
        assert_eq!(t[0].type_ticket, "Bug");
        assert_eq!(t[0].projet, "PROJ");
        assert_eq!(t[0].url, "https://jira.exemple.org/browse/PROJ-1234");
        assert_eq!(t[1].priorite, "", "une priorite nulle devient vide");
    }

    /// Cloud (`/rest/api/3/search/jql`) : meme forme `{"issues":[...]}`, plus la pagination
    /// (`nextPageToken`, `isLast`) et sans `description` demandee dans la liste.
    const RECHERCHE_CLOUD: &str = r#"{"issues":[
      {"key":"PROJ-1234","fields":{"summary":"Correction login",
        "status":{"name":"En cours","statusCategory":{"key":"indeterminate"}},
        "issuetype":{"name":"Bug"},"priority":{"name":"Major"},"project":{"key":"PROJ"},
        "updated":"2026-09-25T10:12:00.000+0200"}}],
      "nextPageToken":"CAEaAggD","isLast":true}"#;

    #[test]
    fn lit_une_recherche_cloud_avec_pagination() {
        let t = lire_recherche(RECHERCHE_CLOUD, BASE).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].cle, "PROJ-1234");
        assert_eq!(t[0].description, "", "pas demandee dans la liste");
    }

    const RECHERCHE_CLOUD_PAGE_SUIVANTE: &str = r#"{"issues":[
      {"key":"PROJ-1","fields":{"summary":"Un","status":{"name":"A faire","statusCategory":{"key":"new"}},
        "issuetype":{"name":"Bug"},"priority":null,"project":{"key":"PROJ"},
        "updated":"2026-09-20T08:00:00.000+0200"}}],
      "nextPageToken":"CAEaAggD","isLast":false}"#;

    #[test]
    fn lire_page_recherche_annonce_le_jeton_de_la_page_suivante_si_pas_derniere() {
        let (tickets, suite) = lire_page_recherche(RECHERCHE_CLOUD_PAGE_SUIVANTE, BASE).unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(suite.as_deref(), Some("CAEaAggD"));
    }

    #[test]
    fn lire_page_recherche_ne_rend_rien_a_suivre_sur_la_derniere_page() {
        let (tickets, suite) = lire_page_recherche(RECHERCHE_CLOUD, BASE).unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(suite, None, "isLast:true arrete la pagination meme si nextPageToken trainait");
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

    /// Cloud (`/rest/api/2/issue/createmeta/{projet}/issuetypes`) rend la liste sous
    /// `issueTypes`, pas `values` (Server/DC) : sans alias, la creation de ticket ne
    /// proposait plus aucun type sur Cloud.
    const TYPES_CLOUD: &str = r#"{"issueTypes":[
      {"id":"1","name":"Bug","subtask":false},
      {"id":"5","name":"Sous-tache","subtask":true}],"maxResults":50,"startAt":0,"total":2}"#;

    #[test]
    fn lit_les_types_cote_cloud_sous_issue_types() {
        let t = lire_types(TYPES_CLOUD).unwrap();
        assert_eq!(t.iter().map(|x| x.nom.as_str()).collect::<Vec<_>>(), vec!["Bug"]);
    }

    #[test]
    fn lit_moi_et_le_ticket_cree() {
        let m = lire_moi(r#"{"name":"jdupont","displayName":"J. Dupont"}"#).unwrap();
        assert_eq!(m.name, "jdupont");
        assert_eq!(m.nom_affiche, "J. Dupont");
        assert_eq!(m.account_id, "", "absent sur Server/DC");
        assert_eq!(lire_ticket_cree(r#"{"id":"1","key":"PROJ-42","self":"x"}"#).unwrap(), "PROJ-42");
    }

    #[test]
    fn lit_moi_cote_cloud_sans_name() {
        let m = lire_moi(r#"{"accountId":"5b10ac8d82e05b22cc7d4ef5","displayName":"J. Dupont"}"#).unwrap();
        assert_eq!(m.name, "", "Cloud n'envoie pas name");
        assert_eq!(m.account_id, "5b10ac8d82e05b22cc7d4ef5");
        assert_eq!(m.nom_affiche, "J. Dupont");
    }

    #[test]
    fn choisit_la_forme_d_assignation_selon_cloud() {
        let moi = Moi { name: "jdupont".to_string(), nom_affiche: "J. Dupont".to_string(), account_id: "acc-123".to_string() };
        assert_eq!(assigne_de(&moi, false), Some(Assigne::Nom("jdupont")));
        assert_eq!(assigne_de(&moi, true), Some(Assigne::Compte("acc-123")));
    }

    #[test]
    fn n_assigne_personne_si_l_identifiant_attendu_est_vide() {
        // Un compte sans accountId (Cloud) ou sans name (Server/DC, improbable mais pas
        // impossible) ne doit jamais envoyer `{"name": ""}` / `{"accountId": ""}` a Jira.
        let moi = Moi { name: String::new(), nom_affiche: "J. Dupont".to_string(), account_id: String::new() };
        assert_eq!(assigne_de(&moi, false), None, "name vide sur Server/DC");
        assert_eq!(assigne_de(&moi, true), None, "accountId vide sur Cloud");
    }

    #[test]
    fn une_reponse_illisible_est_une_erreur_lisible() {
        let e = lire_recherche("<html>proxy</html>", BASE).unwrap_err();
        assert!(e.contains("illisible"), "{e}");
    }

    #[test]
    fn le_corps_de_saisie_porte_la_duree_et_le_commentaire() {
        let c = corps_de_saisie(" 1h30m ", Some("revue")).unwrap();
        assert_eq!(c, serde_json::json!({"timeSpent": "1h30m", "comment": "revue"}));
        let sans = corps_de_saisie("2h", Some("  ")).unwrap();
        assert_eq!(sans, serde_json::json!({"timeSpent": "2h"}));
        assert!(corps_de_saisie("  ", None).is_err());
    }

    #[test]
    fn le_corps_de_creation_assigne_le_ticket_par_name() {
        let c = corps_de_creation("PROJ", "1", " Corriger ", Some("desc"), Some(Assigne::Nom("jdupont"))).unwrap();
        assert_eq!(
            c,
            serde_json::json!({"fields": {
                "project": {"key": "PROJ"}, "issuetype": {"id": "1"}, "summary": "Corriger",
                "assignee": {"name": "jdupont"}, "description": "desc"}})
        );
        assert!(corps_de_creation("PROJ", "1", "  ", None, Some(Assigne::Nom("x"))).is_err());
        assert!(corps_de_creation("PROJ", "", "Titre", None, Some(Assigne::Nom("x"))).is_err());
    }

    #[test]
    fn le_corps_de_creation_assigne_le_ticket_par_account_id_sur_cloud() {
        let c = corps_de_creation("PROJ", "1", "Corriger", None, Some(Assigne::Compte("acc-123"))).unwrap();
        assert_eq!(
            c,
            serde_json::json!({"fields": {
                "project": {"key": "PROJ"}, "issuetype": {"id": "1"}, "summary": "Corriger",
                "assignee": {"accountId": "acc-123"}}})
        );
    }

    #[test]
    fn le_corps_de_creation_sans_assignation_omet_le_champ() {
        let c = corps_de_creation("PROJ", "1", "Corriger", None, None).unwrap();
        assert_eq!(
            c,
            serde_json::json!({"fields": {
                "project": {"key": "PROJ"}, "issuetype": {"id": "1"}, "summary": "Corriger"}})
        );
    }

    #[test]
    fn detecte_un_refus_qui_porte_sur_l_assignation() {
        assert!(refus_du_champ_assignee(
            "requete refusee par Jira : assignee : le champ assignee ne peut pas etre defini"
        ));
        assert!(!refus_du_champ_assignee("resume vide"));
        assert!(!refus_du_champ_assignee("jeton Jira invalide ou expire"));
    }
}
