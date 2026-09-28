//! Partir de la branche principale a jour pour travailler sur un ticket.
//!
//! **TOUJOURS DEPUIS `main` A JOUR** (ou `master` a defaut) : partir de la branche courante
//! embarquerait le travail d'un autre ticket dans la MR de celui-ci.

use super::{run_git_reseau, run_git_strict};

#[derive(Debug)]
pub struct Depart {
    /// La base dont on est parti ; `None` quand la branche existait deja.
    pub base: Option<String>,
    pub creee: bool,
}

async fn existe(depot: &str, reference: &str) -> bool {
    run_git_strict(depot, &["rev-parse", "--verify", "--quiet", reference]).await.is_ok()
}

async fn choisir_la_base(depot: &str) -> Result<String, String> {
    for base in ["main", "master"] {
        if existe(depot, &format!("refs/heads/{base}")).await
            || existe(depot, &format!("refs/remotes/origin/{base}")).await
        {
            return Ok(base.to_string());
        }
    }
    Err("ni main ni master dans ce depot : impossible de choisir une base".to_string())
}

/// Controles d'abord, puis seulement les gestes : un refus ne laisse jamais le depot a moitie
/// deplace. Une branche deja la est reprise telle quelle, sans repartir de la base.
pub async fn partir_de_la_base(depot: &str, branche: &str) -> Result<Depart, String> {
    if run_git_strict(depot, &["check-ref-format", "--branch", branche]).await.is_err() {
        return Err(format!("nom de branche invalide : {branche}"));
    }
    // Les fichiers non suivis ne genent pas un checkout : seuls les suivis modifies bloquent.
    let modifies = run_git_strict(depot, &["status", "--porcelain", "--untracked-files=no"]).await?;
    if !modifies.trim().is_empty() {
        return Err("la copie de travail a des modifications non commitees : commit ou stash d'abord".to_string());
    }
    if existe(depot, &format!("refs/heads/{branche}")).await {
        run_git_strict(depot, &["checkout", branche]).await?;
        return Ok(Depart { base: None, creee: false });
    }

    let a_un_origin = run_git_strict(depot, &["remote", "get-url", "origin"]).await.is_ok();
    if a_un_origin {
        run_git_reseau(depot, &["fetch", "origin"]).await?;
    }
    let base = choisir_la_base(depot).await?;
    run_git_strict(depot, &["checkout", &base]).await?;
    // Passe ce point, un echec laisse le depot sur `base` : on le dit, plutot que de rendre
    // seulement l'erreur brute de git qui ne dit pas ou on en est reste.
    if a_un_origin && existe(depot, &format!("refs/remotes/origin/{base}")).await {
        run_git_reseau(depot, &["pull", "--ff-only", "origin", &base])
            .await
            .map_err(|e| format!("{e} (le depot est maintenant sur {base} ; la branche {branche} n'a pas ete creee)"))?;
    }
    run_git_strict(depot, &["checkout", "-b", branche])
        .await
        .map_err(|e| format!("{e} (le depot est maintenant sur {base} ; la branche {branche} n'a pas ete creee)"))?;
    Ok(Depart { base: Some(base), creee: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn git(dossier: &Path, args: &[&str]) -> String {
        let s = Command::new("git")
            .args(["-c", "user.name=essai", "-c", "user.email=essai@exemple.org", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dossier)
            .output()
            .unwrap();
        assert!(s.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&s.stderr));
        String::from_utf8_lossy(&s.stdout).trim().to_string()
    }

    fn dossier_temporaire() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cockpit-depart-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Un depot « origin » nu et son clone, avec un commit pousse sur `base`.
    fn depot_avec_origin(base: &str) -> (PathBuf, PathBuf) {
        let racine = dossier_temporaire();
        let origin = racine.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", base]);
        git(&racine, &["clone", origin.to_str().unwrap(), "clone"]);
        let clone = racine.join("clone");
        // Un clone de depot vide ne sait pas toujours sur quelle branche il est : on le dit.
        git(&clone, &["symbolic-ref", "HEAD", &format!("refs/heads/{base}")]);
        std::fs::write(clone.join("a.txt"), "un").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "-m", "depart"]);
        git(&clone, &["push", "origin", base]);
        (origin, clone)
    }

    fn branche_courante(depot: &Path) -> String {
        git(depot, &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    #[tokio::test]
    async fn part_de_main_a_jour() {
        let (origin, clone) = depot_avec_origin("main");
        // Quelqu'un d'autre pousse sur main.
        let autre = origin.parent().unwrap().join("autre");
        git(origin.parent().unwrap(), &["clone", origin.to_str().unwrap(), "autre"]);
        std::fs::write(autre.join("b.txt"), "deux").unwrap();
        git(&autre, &["add", "."]);
        git(&autre, &["commit", "-m", "nouveau"]);
        git(&autre, &["push", "origin", "main"]);
        // Le clone est sur une autre branche.
        git(&clone, &["checkout", "-b", "ailleurs"]);

        let d = partir_de_la_base(clone.to_str().unwrap(), "feature/CCM-1/essai").await.unwrap();
        assert!(d.creee);
        assert_eq!(d.base.as_deref(), Some("main"));
        assert_eq!(branche_courante(&clone), "feature/CCM-1/essai");
        assert!(clone.join("b.txt").exists(), "la branche part de main A JOUR");
    }

    #[tokio::test]
    async fn prend_master_sans_main() {
        let (_, clone) = depot_avec_origin("master");
        let d = partir_de_la_base(clone.to_str().unwrap(), "fix/CCM-2/x").await.unwrap();
        assert_eq!(d.base.as_deref(), Some("master"));
    }

    #[tokio::test]
    async fn refuse_une_copie_modifiee() {
        let (_, clone) = depot_avec_origin("main");
        git(&clone, &["checkout", "-b", "ailleurs"]);
        std::fs::write(clone.join("a.txt"), "modifie").unwrap();
        let e = partir_de_la_base(clone.to_str().unwrap(), "fix/CCM-3/x").await.unwrap_err();
        assert!(e.contains("commit ou stash"), "{e}");
        assert_eq!(branche_courante(&clone), "ailleurs", "rien n'a bouge, pas meme un checkout avant le refus");
    }

    #[tokio::test]
    async fn un_fichier_non_suivi_ne_bloque_pas() {
        let (_, clone) = depot_avec_origin("main");
        std::fs::write(clone.join("brouillon.txt"), "x").unwrap();
        assert!(partir_de_la_base(clone.to_str().unwrap(), "fix/CCM-4/x").await.is_ok());
    }

    #[tokio::test]
    async fn bascule_sur_une_branche_existante() {
        let (_, clone) = depot_avec_origin("main");
        git(&clone, &["checkout", "-b", "fix/CCM-5/x"]);
        git(&clone, &["checkout", "main"]);
        let d = partir_de_la_base(clone.to_str().unwrap(), "fix/CCM-5/x").await.unwrap();
        assert!(!d.creee);
        assert_eq!(d.base, None);
        assert_eq!(branche_courante(&clone), "fix/CCM-5/x");
    }

    #[tokio::test]
    async fn refuse_un_nom_invalide_avant_tout() {
        let (_, clone) = depot_avec_origin("main");
        git(&clone, &["checkout", "-b", "ailleurs"]);
        let e = partir_de_la_base(clone.to_str().unwrap(), "fix/CCM-6/avec espace").await.unwrap_err();
        assert!(e.contains("nom de branche invalide"), "{e}");
        assert_eq!(branche_courante(&clone), "ailleurs");
    }

    #[tokio::test]
    async fn un_pull_impossible_dit_ou_est_reste_le_depot() {
        let (origin, clone) = depot_avec_origin("main");
        git(&clone, &["checkout", "-b", "ailleurs"]);
        // Le main local avance de son cote...
        git(&clone, &["checkout", "main"]);
        std::fs::write(clone.join("a.txt"), "local").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "-m", "divergence locale"]);
        git(&clone, &["checkout", "ailleurs"]);
        // ...pendant qu'un autre commit, different, est pousse sur origin/main : ff-only impossible.
        let autre = origin.parent().unwrap().join("autre");
        git(origin.parent().unwrap(), &["clone", origin.to_str().unwrap(), "autre"]);
        std::fs::write(autre.join("b.txt"), "distant").unwrap();
        git(&autre, &["add", "."]);
        git(&autre, &["commit", "-m", "divergence distante"]);
        git(&autre, &["push", "origin", "main"]);

        let e = partir_de_la_base(clone.to_str().unwrap(), "fix/CCM-8/x").await.unwrap_err();
        assert!(e.contains("maintenant sur main"), "{e}");
        assert_eq!(branche_courante(&clone), "main", "le checkout a reussi, seul le pull a echoue");
    }

    #[tokio::test]
    async fn sans_main_ni_master_c_est_une_erreur() {
        let depot = dossier_temporaire();
        git(&depot, &["init", "-b", "trunk"]);
        std::fs::write(depot.join("a.txt"), "un").unwrap();
        git(&depot, &["add", "."]);
        git(&depot, &["commit", "-m", "depart"]);
        let e = partir_de_la_base(depot.to_str().unwrap(), "fix/CCM-7/x").await.unwrap_err();
        assert!(e.contains("ni main ni master"), "{e}");
    }
}
