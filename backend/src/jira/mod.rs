//! Jira Server / Data Center : lister ses tickets et les faire avancer depuis Cockpit.
//!
//! **LE JETON NE QUITTE JAMAIS LE BACKEND.** L'interface recoit `jeton_pose`, pas la valeur :
//! meme regle que les cles d'API des fournisseurs d'IA.
//!
//! **TOUTES LES COMMANDES SONT `async`** : elles attendent le reseau ou git.

use marqueur_commande::commande;

pub mod branche;
pub mod client;
pub mod config;
pub mod jql;
pub mod modele;

use branche::Correspondance;
use client::Jira;
use config::{ConfigJira, LiaisonJira};
use modele::{DetailTicket, Ticket, Transition, TypeTicket};
use reqwest::Method;

const CHAMPS_LISTE: &str = "summary,status,issuetype,priority,project,updated";
const CHAMPS_DETAIL: &str = "summary,description,status,issuetype,priority,project,updated,comment";

#[commande]
pub async fn jira_config(state: &crate::AppState) -> Result<ConfigJira, String> {
    Ok(config::lire(&state.db))
}

#[commande]
pub async fn jira_poser_config(
    state: &crate::AppState,
    url: String,
    jeton: Option<String>,
    types_branche: Option<Correspondance>,
) -> Result<ConfigJira, String> {
    config::poser(&state.db, &url, jeton.as_deref(), types_branche)
}

/// Rend le nom affiche du compte : de quoi confirmer que c'est le bon.
#[commande]
pub async fn jira_tester(state: &crate::AppState) -> Result<String, String> {
    let moi = moi(&Jira::depuis(&state.db)?).await?;
    Ok(if moi.nom_affiche.is_empty() { moi.name } else { moi.nom_affiche })
}

/// `cles_projets` absent : tous mes tickets. Liste vide : aucun (voir `jql::mes_tickets`).
#[commande]
pub async fn jira_mes_tickets(
    state: &crate::AppState,
    cles_projets: Option<Vec<String>>,
) -> Result<Vec<Ticket>, String> {
    let Some(jql) = jql::mes_tickets(cles_projets.as_deref())? else {
        return Ok(vec![]);
    };
    let jira = Jira::depuis(&state.db)?;
    let corps = jira
        .envoyer(
            Method::GET,
            "/rest/api/2/search",
            &[("jql", jql.as_str()), ("fields", CHAMPS_LISTE), ("maxResults", "200")],
            None,
        )
        .await?;
    modele::lire_recherche(&corps, jira.base())
}

#[commande]
pub async fn jira_ticket(state: &crate::AppState, cle: String) -> Result<DetailTicket, String> {
    lire_le_ticket(&Jira::depuis(&state.db)?, &cle).await
}

#[commande]
pub async fn jira_transitions(state: &crate::AppState, cle: String) -> Result<Vec<Transition>, String> {
    lister_transitions(&Jira::depuis(&state.db)?, &cle).await
}

#[commande]
pub async fn jira_liaisons(state: &crate::AppState) -> Result<Vec<LiaisonJira>, String> {
    config::liaisons(&state.db)
}

#[commande]
pub async fn jira_liaison(state: &crate::AppState, projet: String) -> Result<LiaisonJira, String> {
    let nom = crate::resolve_db_project_name(state, &projet).await;
    config::liaison(&state.db, &nom)
}

#[commande]
pub async fn jira_poser_liaison(
    state: &crate::AppState,
    projet: String,
    cles: String,
    gabarit: String,
) -> Result<LiaisonJira, String> {
    let nom = crate::resolve_db_project_name(state, &projet).await;
    config::poser_liaison(&state.db, &nom, &cles, &gabarit)
}

/// L'apercu calcule ici plutot que dans l'interface : une seule regle de nommage.
#[commande]
pub async fn jira_apercu_branche(
    state: &crate::AppState,
    gabarit: String,
    cle: String,
    type_ticket: String,
    resume: String,
) -> Result<String, String> {
    Ok(branche::nom_de_branche(&gabarit, &cle, &type_ticket, &resume, &config::types_branche(&state.db)))
}

#[commande]
pub async fn jira_transitionner(state: &crate::AppState, cle: String, transition_id: String) -> Result<(), String> {
    transitionner(&Jira::depuis(&state.db)?, &cle, &transition_id).await
}

#[commande]
pub async fn jira_commenter(state: &crate::AppState, cle: String, texte: String) -> Result<(), String> {
    jql::verifier_cle_de_ticket(&cle)?;
    let texte = texte.trim();
    if texte.is_empty() {
        return Err("commentaire vide".to_string());
    }
    Jira::depuis(&state.db)?
        .envoyer(
            Method::POST,
            &format!("/rest/api/2/issue/{cle}/comment"),
            &[],
            Some(serde_json::json!({ "body": texte })),
        )
        .await
        .map(|_| ())
}

#[commande]
pub async fn jira_saisir_temps(
    state: &crate::AppState,
    cle: String,
    duree: String,
    commentaire: Option<String>,
) -> Result<(), String> {
    jql::verifier_cle_de_ticket(&cle)?;
    let corps = modele::corps_de_saisie(&duree, commentaire.as_deref())?;
    Jira::depuis(&state.db)?
        .envoyer(Method::POST, &format!("/rest/api/2/issue/{cle}/worklog"), &[], Some(corps))
        .await
        .map(|_| ())
}

#[commande]
pub async fn jira_types_ticket(state: &crate::AppState, cle_projet: String) -> Result<Vec<TypeTicket>, String> {
    let cle_projet = cle_projet.trim().to_uppercase();
    if !jql::cle_de_projet_valide(&cle_projet) {
        return Err(format!("cle de projet Jira invalide : {cle_projet}"));
    }
    let corps = Jira::depuis(&state.db)?
        .envoyer(Method::GET, &format!("/rest/api/2/issue/createmeta/{cle_projet}/issuetypes"), &[], None)
        .await?;
    modele::lire_types(&corps)
}

/// Cree le ticket en me l'assignant, et rend sa cle.
#[commande]
pub async fn jira_creer_ticket(
    state: &crate::AppState,
    cle_projet: String,
    type_id: String,
    resume: String,
    description: Option<String>,
) -> Result<String, String> {
    let cle_projet = cle_projet.trim().to_uppercase();
    if !jql::cle_de_projet_valide(&cle_projet) {
        return Err(format!("cle de projet Jira invalide : {cle_projet}"));
    }
    let jira = Jira::depuis(&state.db)?;
    let moi = moi(&jira).await?;
    let corps = modele::corps_de_creation(&cle_projet, &type_id, &resume, description.as_deref(), &moi.name)?;
    let reponse = jira.envoyer(Method::POST, "/rest/api/2/issue", &[], Some(corps)).await?;
    modele::lire_ticket_cree(&reponse)
}

async fn transitionner(jira: &Jira, cle: &str, transition_id: &str) -> Result<(), String> {
    jql::verifier_cle_de_ticket(cle)?;
    jira.envoyer(
        Method::POST,
        &format!("/rest/api/2/issue/{cle}/transitions"),
        &[],
        Some(serde_json::json!({ "transition": { "id": transition_id } })),
    )
    .await
    .map(|_| ())
}

async fn moi(jira: &Jira) -> Result<modele::Moi, String> {
    let corps = jira.envoyer(Method::GET, "/rest/api/2/myself", &[], None).await?;
    modele::lire_moi(&corps)
}

async fn lire_le_ticket(jira: &Jira, cle: &str) -> Result<DetailTicket, String> {
    jql::verifier_cle_de_ticket(cle)?;
    let corps = jira
        .envoyer(Method::GET, &format!("/rest/api/2/issue/{cle}"), &[("fields", CHAMPS_DETAIL)], None)
        .await?;
    modele::lire_ticket(&corps, jira.base())
}

async fn lister_transitions(jira: &Jira, cle: &str) -> Result<Vec<Transition>, String> {
    jql::verifier_cle_de_ticket(cle)?;
    let corps = jira
        .envoyer(Method::GET, &format!("/rest/api/2/issue/{cle}/transitions"), &[], None)
        .await?;
    modele::lire_transitions(&corps)
}
