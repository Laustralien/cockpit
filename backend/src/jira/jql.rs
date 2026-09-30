//! Les requetes JQL et la validation de ce qui y entre.
//!
//! **UNE CLE N'ENTRE DANS LA JQL OU DANS UN CHEMIN QU'APRES VALIDATION** : c'est ce qui
//! empeche une saisie de devenir une autre requete (`PROJ) OR (1=1`) ou un autre chemin (`../`).

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

/// Majuscules et espaces retires, puis validation : ce qui entre dans un chemin ou un corps.
pub fn normaliser_cle_de_projet(brut: &str) -> Result<String, String> {
    let cle = brut.trim().to_uppercase();
    if cle_de_projet_valide(&cle) {
        Ok(cle)
    } else {
        Err(format!("cle de projet Jira invalide : {cle}"))
    }
}

/// « proj, ABC;abc DEF » → `["PROJ", "ABC", "DEF"]` : majuscules, sans doublon, dans l'ordre saisi.
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
        assert!(cle_de_projet_valide("PROJ"));
        assert!(cle_de_projet_valide("AB_2"));
        assert!(!cle_de_projet_valide("proj"));
        assert!(!cle_de_projet_valide("2AB"));
        assert!(!cle_de_projet_valide("PROJ) OR (1=1"));
        assert!(!cle_de_projet_valide(""));
    }

    #[test]
    fn normalise_puis_valide_une_cle_de_projet() {
        assert_eq!(normaliser_cle_de_projet(" proj ").unwrap(), "PROJ");
        assert!(normaliser_cle_de_projet("PROJ-1").unwrap_err().contains("invalide"));
    }

    #[test]
    fn valide_les_cles_de_ticket() {
        assert!(cle_de_ticket_valide("PROJ-1234"));
        assert!(!cle_de_ticket_valide("PROJ-"));
        assert!(!cle_de_ticket_valide("PROJ-12a"));
        assert!(!cle_de_ticket_valide("../PROJ-1"));
        assert!(verifier_cle_de_ticket("x").is_err());
    }

    #[test]
    fn decoupe_et_normalise_les_cles() {
        assert_eq!(decouper_cles(" proj, ABC;abc  DEF "), vec!["PROJ", "ABC", "DEF"]);
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
        let cles = vec!["PROJ".to_string(), "ABC".to_string()];
        assert_eq!(
            mes_tickets(Some(cles.as_slice())).unwrap().unwrap(),
            "assignee = currentUser() AND statusCategory != Done AND project in (PROJ, ABC) ORDER BY updated DESC"
        );
    }

    #[test]
    fn un_filtre_vide_ne_demande_rien() {
        assert_eq!(mes_tickets(Some(&[][..])).unwrap(), None);
    }

    #[test]
    fn une_cle_invalide_est_refusee() {
        let cles = vec!["PROJ) OR (1=1".to_string()];
        assert!(mes_tickets(Some(cles.as_slice())).is_err());
    }
}
