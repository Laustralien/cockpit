//! Ce que l'utilisateur a ecrit pour SON agent : les consignes globales et les skills.
//!
//! **UNE CAPACITE COMME LES AUTRES, DONC `None` PAR DEFAUT.** Chaque CLI d'agent range ces
//! choses a sa facon — Claude Code lit `~/.claude/CLAUDE.md` et `~/.claude/skills/` — et
//! plusieurs n'ont aucun equivalent. Un fournisseur qui ne sait pas le DIT, et l'interface
//! n'affiche rien plutot que d'inventer un fichier qui ne sera jamais lu.
//!
//! **LE CLIENT NE CHOISIT JAMAIS UN CHEMIN.** Les commandes prennent l'IDENTIFIANT du
//! fournisseur, et c'est LUI qui dit ou ecrire. Accepter un chemin venu de l'interface
//! donnerait a n'importe quel appel le droit d'ecrire ou il veut sur le disque : c'est la
//! regle « ne jamais croire une valeur venue du client », appliquee au cas ou elle coute le
//! plus cher.

use std::path::PathBuf;

/// Un fournisseur dont l'utilisateur peut lire et regler les consignes.
pub trait Consignes: Send + Sync {
    /// Le fichier d'instructions globales, tel que le CLI le lit.
    fn fichier(&self) -> PathBuf;

    /// Le dossier des skills, un par sous-dossier. `None` quand le CLI n'a pas ce
    /// concept : on n'affiche alors pas une liste vide, qui se lirait « aucun skill ».
    fn dossier_skills(&self) -> Option<PathBuf> {
        None
    }
}

/// Les consignes globales, telles qu'on les montre.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct EtatConsignes {
    /// Le fournisseur qui les porte.
    pub fournisseur: String,
    pub nom: String,
    /// Chemin complet. **TOUJOURS MONTRE** : on edite un fichier qui vit hors de Cockpit,
    /// dans la configuration d'un autre logiciel, et personne ne doit le decouvrir apres coup.
    pub chemin: String,
    /// Absent tant que l'utilisateur n'en a pas ecrit : ce n'est pas une erreur, et le
    /// premier enregistrement le cree.
    pub existe: bool,
    pub contenu: String,
}

/// Un skill : un sous-dossier qui porte un `SKILL.md`.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct Skill {
    pub nom: String,
    /// Ce que le skill dit de lui-meme. Vide quand le fichier n'a pas d'en-tete.
    pub description: String,
    pub chemin: String,
}

/// Lit le fichier de consignes.
///
/// Un fichier absent rend un contenu VIDE et `existe: false`, jamais une erreur : c'est
/// l'etat normal de quelqu'un qui n'a encore rien ecrit.
pub fn lire(fichier: &std::path::Path) -> Result<(bool, String), String> {
    match std::fs::read_to_string(fichier) {
        Ok(texte) => Ok((true, texte)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((false, String::new())),
        Err(e) => Err(format!("lecture de {} : {e}", fichier.display())),
    }
}

/// Ecrit le fichier de consignes, en creant son dossier au besoin.
///
/// **ECRITURE A COTE PUIS RENOMMAGE** : ce fichier est lu par un AUTRE logiciel, qui peut le
/// relire a tout moment. Une ecriture directe le laisserait a moitie ecrit le temps d'un
/// battement, et l'agent lirait des consignes tronquees.
pub fn ecrire(fichier: &std::path::Path, contenu: &str) -> Result<(), String> {
    if let Some(parent) = fichier.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creation de {} : {e}", parent.display()))?;
    }
    let temporaire = fichier.with_extension(format!("cockpit-{}", std::process::id()));
    std::fs::write(&temporaire, contenu)
        .map_err(|e| format!("ecriture de {} : {e}", temporaire.display()))?;
    std::fs::rename(&temporaire, fichier).map_err(|e| {
        let _ = std::fs::remove_file(&temporaire);
        format!("remplacement de {} : {e}", fichier.display())
    })
}

/// Les skills d'un dossier, tries par nom.
///
/// Un dossier absent rend une liste vide : personne n'a encore ecrit de skill, et ce
/// n'est pas une panne.
pub fn lister_les_skills(dossier: &std::path::Path) -> Vec<Skill> {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return Vec::new();
    };
    let mut skills: Vec<Skill> = entrees
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let chemin = e.path();
            let fiche = chemin.join("SKILL.md");
            // **UN SOUS-DOSSIER SANS `SKILL.md` N'EST PAS UN SKILL.** Le dossier peut contenir
            // autre chose (un `.git`, des ressources) : l'afficher ferait croire a un skill
            // que le CLI ne chargera jamais.
            if !fiche.is_file() {
                return None;
            }
            let nom = chemin.file_name()?.to_string_lossy().to_string();
            let description = std::fs::read_to_string(&fiche)
                .map(|texte| description_de(&texte))
                .unwrap_or_default();
            Some(Skill { nom, description, chemin: chemin.to_string_lossy().to_string() })
        })
        .collect();
    skills.sort_by(|a, b| a.nom.cmp(&b.nom));
    skills
}

/// La description annoncee par l'en-tete d'un `SKILL.md`.
///
/// **PAS DE BIBLIOTHEQUE YAML POUR DEUX CHAMPS.** L'en-tete est delimite par deux lignes
/// `---` et la description tient sur UNE ligne, meme tres longue. On lit donc ce qui suit
/// `description:` dans ce bloc, et rien d'autre : en demander plus voudrait dire porter un
/// analyseur complet pour afficher un sous-titre.
pub fn description_de(texte: &str) -> String {
    let mut lignes = texte.lines();
    // L'en-tete doit COMMENCER le fichier. Un `---` trouve au milieu est une separation
    // ordinaire de markdown, pas un en-tete.
    if lignes.next().map(str::trim) != Some("---") {
        return String::new();
    }
    for ligne in lignes {
        if ligne.trim() == "---" {
            break;
        }
        if let Some(reste) = ligne.trim_start().strip_prefix("description:") {
            return reste.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_description_se_lit_dans_l_en_tete() {
        let texte = "---\nname: cache\ndescription: Tout ce qui decide qu'une page est servie\n---\n\n# Titre\n";
        assert_eq!(description_de(texte), "Tout ce qui decide qu'une page est servie");
    }

    #[test]
    fn un_fichier_sans_en_tete_n_a_pas_de_description() {
        assert_eq!(description_de("# Titre\n\ndescription: pas dans l'en-tete\n"), "");
        assert_eq!(description_de(""), "");
    }

    /// Un `---` au milieu du texte est une separation markdown, pas un en-tete.
    #[test]
    fn un_tiret_au_milieu_n_ouvre_pas_un_en_tete() {
        assert_eq!(description_de("# Titre\n\n---\ndescription: piege\n---\n"), "");
    }

    #[test]
    fn les_guillemets_de_l_en_tete_sont_retires() {
        assert_eq!(description_de("---\ndescription: \"avec guillemets\"\n---\n"), "avec guillemets");
        assert_eq!(description_de("---\ndescription: 'simples'\n---\n"), "simples");
    }

    /// La description s'arrete a la fin de l'en-tete : ce qui suit appartient au corps.
    #[test]
    fn ce_qui_suit_l_en_tete_n_est_pas_lu() {
        let texte = "---\nname: x\n---\ndescription: du corps\n";
        assert_eq!(description_de(texte), "");
    }

    #[test]
    fn un_fichier_absent_se_lit_comme_vide_et_pas_comme_une_panne() {
        let manquant = std::env::temp_dir().join("cockpit-consignes-absentes-42.md");
        let _ = std::fs::remove_file(&manquant);
        assert_eq!(lire(&manquant).unwrap(), (false, String::new()));
    }

    #[test]
    fn ce_qu_on_ecrit_se_relit() {
        let bac = std::env::temp_dir().join(format!("cockpit-consignes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bac);
        let fichier = bac.join("sous").join("CLAUDE.md");
        // Le dossier n'existe pas encore : l'ecriture le cree.
        ecrire(&fichier, "mes consignes").unwrap();
        assert_eq!(lire(&fichier).unwrap(), (true, "mes consignes".to_string()));
        ecrire(&fichier, "").unwrap();
        assert_eq!(lire(&fichier).unwrap(), (true, String::new()), "on peut tout effacer");
        // Rien ne traine a cote du fichier.
        let restes: Vec<String> = std::fs::read_dir(fichier.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(restes, vec!["CLAUDE.md".to_string()]);
        let _ = std::fs::remove_dir_all(&bac);
    }

    #[test]
    fn une_skill_est_un_dossier_qui_porte_un_skill_md() {
        let bac = std::env::temp_dir().join(format!("cockpit-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bac);
        std::fs::create_dir_all(bac.join("cache")).unwrap();
        std::fs::write(
            bac.join("cache").join("SKILL.md"),
            "---\nname: cache\ndescription: Le cache HTTP\n---\n",
        )
        .unwrap();
        // Un dossier SANS fiche : le CLI ne le chargera jamais, on ne le montre pas.
        std::fs::create_dir_all(bac.join("brouillon")).unwrap();
        // Un fichier a la racine n'est pas un skill non plus.
        std::fs::write(bac.join("notes.md"), "x").unwrap();

        let liste = lister_les_skills(&bac);
        assert_eq!(liste.len(), 1, "seul le dossier avec SKILL.md compte : {liste:?}");
        assert_eq!(liste[0].nom, "cache");
        assert_eq!(liste[0].description, "Le cache HTTP");
        let _ = std::fs::remove_dir_all(&bac);
    }

    #[test]
    fn un_dossier_de_skills_absent_rend_une_liste_vide() {
        let manquant = std::env::temp_dir().join("cockpit-skills-qui-n-existent-pas-42");
        let _ = std::fs::remove_dir_all(&manquant);
        assert!(lister_les_skills(&manquant).is_empty());
    }
}
