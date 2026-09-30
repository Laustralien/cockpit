//! Le nom de branche d'un ticket, tire d'un gabarit propre a chaque projet.
//!
//! **UN GABARIT, PAS UN FORMAT EN DUR** : chaque equipe a sa convention
//! (`{type}/tl/{cle}/{slug}` chez PROJ), et Cockpit est un outil public.

use std::collections::HashMap;

pub const GABARIT_PAR_DEFAUT: &str = "{type}/{cle}/{slug}";

/// Au-dela, le nom de branche devient illisible dans un terminal.
const LONGUEUR_SLUG: usize = 50;

/// Type de ticket Jira → type de branche. La cle `*` sert de repli.
pub type Correspondance = HashMap<String, String>;

pub fn correspondance_par_defaut() -> Correspondance {
    HashMap::from([("Bug".to_string(), "fix".to_string()), ("*".to_string(), "feature".to_string())])
}

/// Sans tenir compte de la casse : « Bug » et « bug » sont le meme type pour un humain.
pub fn type_de_branche(type_ticket: &str, c: &Correspondance) -> String {
    c.iter()
        .find(|(k, _)| k.as_str() != "*" && k.eq_ignore_ascii_case(type_ticket))
        .map(|(_, v)| v.clone())
        .or_else(|| c.get("*").cloned())
        .unwrap_or_else(|| "feature".to_string())
}

fn translitterer(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'ç' => "c",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => "o",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'ý' | 'ÿ' => "y",
        'œ' => "oe",
        'æ' => "ae",
        'ß' => "ss",
        _ => return None,
    })
}

pub fn slug(texte: &str) -> String {
    let mut brut = String::new();
    for c in texte.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            brut.push(c);
        } else if let Some(t) = translitterer(c) {
            brut.push_str(t);
        } else {
            brut.push('-');
        }
    }
    let mut slug = brut.split('-').filter(|m| !m.is_empty()).collect::<Vec<_>>().join("-");
    if slug.len() > LONGUEUR_SLUG {
        // Que de l'ASCII a ce stade : couper a un octet ne tombe jamais dans un caractere.
        slug.truncate(LONGUEUR_SLUG);
        while slug.ends_with('-') {
            slug.pop();
        }
    }
    slug
}

/// **UN SLUG VIDE DONNERAIT UNE BRANCHE EN `/`**, que git refuse : on retombe sur la cle.
pub fn nom_de_branche(gabarit: &str, cle: &str, type_ticket: &str, resume: &str, c: &Correspondance) -> String {
    let gabarit = if gabarit.trim().is_empty() { GABARIT_PAR_DEFAUT } else { gabarit.trim() };
    let mut s = slug(resume);
    if s.is_empty() {
        s = cle.to_lowercase();
    }
    gabarit
        .replace("{type}", &type_de_branche(type_ticket, c))
        .replace("{cle}", cle)
        .replace("{slug}", &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_bug_devient_une_branche_fix() {
        let c = correspondance_par_defaut();
        assert_eq!(
            nom_de_branche("{type}/tl/{cle}/{slug}", "PROJ-1234", "Bug", "Correction login", &c),
            "fix/tl/PROJ-1234/correction-login"
        );
    }

    #[test]
    fn un_type_inconnu_prend_le_repli() {
        let c = correspondance_par_defaut();
        assert_eq!(type_de_branche("Story", &c), "feature");
    }

    #[test]
    fn la_correspondance_ignore_la_casse() {
        let c = correspondance_par_defaut();
        assert_eq!(type_de_branche("bug", &c), "fix");
    }

    #[test]
    fn le_slug_retire_accents_et_ponctuation() {
        assert_eq!(slug("Écran d'accueil : « Déjà vu » !"), "ecran-d-accueil-deja-vu");
        assert_eq!(slug("Cœur   du  système"), "coeur-du-systeme");
    }

    #[test]
    fn le_slug_est_borne_sans_tiret_final() {
        let s = slug(&"mot ".repeat(30));
        assert!(s.len() <= 50, "{s}");
        assert!(!s.ends_with('-'), "{s}");
        assert!(!s.starts_with('-'), "{s}");
    }

    #[test]
    fn un_resume_sans_lettre_retombe_sur_la_cle() {
        let c = correspondance_par_defaut();
        assert_eq!(nom_de_branche("{type}/{cle}/{slug}", "PROJ-9", "Bug", "???", &c), "fix/PROJ-9/proj-9");
    }

    #[test]
    fn un_gabarit_vide_prend_le_gabarit_par_defaut() {
        let c = correspondance_par_defaut();
        assert_eq!(nom_de_branche("  ", "ABC-1", "Task", "Truc", &c), "feature/ABC-1/truc");
    }
}
