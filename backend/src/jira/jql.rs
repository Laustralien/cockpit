//! Les requetes JQL et la validation de ce qui y entre.
//!
//! **UNE CLE N'ENTRE DANS LA JQL OU DANS UN CHEMIN QU'APRES VALIDATION** : c'est ce qui
//! empeche une saisie de devenir une autre requete (`CCM) OR (1=1`) ou un autre chemin (`../`).

pub fn cle_de_projet_valide(cle: &str) -> bool {
    let mut c = cle.chars();
    matches!(c.next(), Some(x) if x.is_ascii_uppercase())
        && c.all(|x| x.is_ascii_uppercase() || x.is_ascii_digit() || x == '_')
}

pub fn cle_de_ticket_valide(cle: &str) -> bool {
    match cle.rsplit_once('-') {
        Some((projet, numero)) => {
            cle_de_projet_valide(projet) && !numero.is_empty() && numero.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

pub fn verifier_cle_de_ticket(cle: &str) -> Result<(), String> {
    if cle_de_ticket_valide(cle) {
        Ok(())
    } else {
        Err(format!("cle de ticket Jira invalide : {cle}"))
    }
}

/// « ccm, ABC;abc DEF » → `["CCM", "ABC", "DEF"]` : majuscules, sans doublon, dans l'ordre saisi.
pub fn decouper_cles(brut: &str) -> Vec<String> {
    let mut cles: Vec<String> = Vec::new();
    for morceau in brut.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let cle = morceau.trim().to_uppercase();
        if !cle.is_empty() && !cles.contains(&cle) {
            cles.push(cle);
        }
    }
    cles
}

/// `None` en entree : tous mes tickets. **UNE LISTE VIDE NE DEMANDE RIEN** (`Ok(None)`) : un
/// projet sans cle liee ne doit pas afficher les tickets de tous les autres.
pub fn mes_tickets(cles: Option<&[String]>) -> Result<Option<String>, String> {
    const DEBUT: &str = "assignee = currentUser() AND statusCategory != Done";
    const FIN: &str = "ORDER BY updated DESC";
    let Some(cles) = cles else {
        return Ok(Some(format!("{DEBUT} {FIN}")));
    };
    if cles.is_empty() {
        return Ok(None);
    }
    if let Some(mauvaise) = cles.iter().find(|c| !cle_de_projet_valide(c)) {
        return Err(format!("cle de projet Jira invalide : {mauvaise}"));
    }
    Ok(Some(format!("{DEBUT} AND project in ({}) {FIN}", cles.join(", "))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valide_les_cles_de_projet() {
        assert!(cle_de_projet_valide("CCM"));
        assert!(cle_de_projet_valide("AB_2"));
        assert!(!cle_de_projet_valide("ccm"));
        assert!(!cle_de_projet_valide("2AB"));
        assert!(!cle_de_projet_valide("CCM) OR (1=1"));
        assert!(!cle_de_projet_valide(""));
    }

    #[test]
    fn valide_les_cles_de_ticket() {
        assert!(cle_de_ticket_valide("CCM-1234"));
        assert!(!cle_de_ticket_valide("CCM-"));
        assert!(!cle_de_ticket_valide("CCM-12a"));
        assert!(!cle_de_ticket_valide("../CCM-1"));
        assert!(verifier_cle_de_ticket("x").is_err());
    }

    #[test]
    fn decoupe_et_normalise_les_cles() {
        assert_eq!(decouper_cles(" ccm, ABC;abc  DEF "), vec!["CCM", "ABC", "DEF"]);
        assert!(decouper_cles("  ").is_empty());
    }

    #[test]
    fn sans_filtre_on_demande_tous_mes_tickets() {
        assert_eq!(
            mes_tickets(None).unwrap().unwrap(),
            "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
        );
    }

    #[test]
    fn le_filtre_restreint_aux_projets() {
        let cles = vec!["CCM".to_string(), "ABC".to_string()];
        assert_eq!(
            mes_tickets(Some(cles.as_slice())).unwrap().unwrap(),
            "assignee = currentUser() AND statusCategory != Done AND project in (CCM, ABC) ORDER BY updated DESC"
        );
    }

    #[test]
    fn un_filtre_vide_ne_demande_rien() {
        assert_eq!(mes_tickets(Some(&[][..])).unwrap(), None);
    }

    #[test]
    fn une_cle_invalide_est_refusee() {
        let cles = vec!["CCM) OR (1=1".to_string()];
        assert!(mes_tickets(Some(cles.as_slice())).is_err());
    }
}
