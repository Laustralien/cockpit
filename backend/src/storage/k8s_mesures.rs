//! L'historique des mesures d'un namespace, garde en local.
//!
//! **POURQUOI CETTE TABLE EXISTE.** Le cluster ne rend qu'un INSTANTANE : personne ne garde
//! l'historique a notre place, et la source qui le ferait (un Prometheus) n'est pas joignable
//! avec les droits d'un developpeur. Sans ces lignes, une courbe repart de zero a chaque
//! ouverture d'ecran et une periode d'une heure ne montre rien.
//!
//! **ELLE NE VOYAGE PAS.** La synchronisation a une liste explicite (`storage::synchro::TYPES`)
//! et cette table n'y est pas : des dizaines de milliers de points par jour n'ont rien a faire
//! sur le compte de quelqu'un, et ils ne decrivent que le cluster vu depuis CETTE machine.
//!
//! **ET ELLE NE GROSSIT PAS SANS FIN.** Trois bornes : on n'ecrit que pour les namespaces que
//! l'utilisateur a declares, on ramene les points de plus d'une heure a UN par minute, et on
//! jette au-dela de la duree qu'il a choisie. Sans ca, un namespace de 25 pods mesure toutes
//! les cinq secondes ecrirait 432 000 lignes par jour.
//!
//! **ET LE NOM DU POD N'EST ECRIT QU'UNE FOIS** (`k8s_cibles`). Mesure : 160 octets par point
//! en repetant cluster + namespace + pod a chaque ligne, 22 octets avec un identifiant. Sur une
//! journee et 25 pods : 5,5 Mo contre 0,7 Mo.

use rusqlite::{params, Result};

/// Au-dela, un point par minute suffit : personne ne lit la seconde pres sur une vieille heure.
const PLEINE_RESOLUTION_MS: i64 = 3_600_000;
/// Au-dela, on jette. C'est la « fraicheur » que l'ecran promet.
pub const RETENTION_MS: i64 = 24 * 3_600_000;

/// Une mesure telle qu'on la range et la relit.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub pod: String,
    pub t: i64,
    pub cpu: i64,
    pub ram: i64,
}

impl super::Database {
    /// Ecrit un tour de mesures. Une seule transaction : 25 insertions isolees couteraient 25
    /// ecritures disque la ou une seule suffit.
    pub fn k8s_noter(
        &self,
        contexte: &str,
        namespace: &str,
        t: i64,
        mesures: &[(String, i64, i64)],
    ) -> Result<()> {
        if mesures.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut nommer = tx.prepare(
                "INSERT OR IGNORE INTO k8s_cibles (contexte, namespace, pod) VALUES (?1, ?2, ?3)",
            )?;
            let mut retrouver = tx.prepare(
                "SELECT id FROM k8s_cibles WHERE contexte = ?1 AND namespace = ?2 AND pod = ?3",
            )?;
            let mut poser = tx.prepare(
                "INSERT OR REPLACE INTO k8s_mesures (cible, t, cpu, ram) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (pod, cpu, ram) in mesures {
                nommer.execute(params![contexte, namespace, pod])?;
                let id: i64 = retrouver.query_row(params![contexte, namespace, pod], |l| l.get(0))?;
                poser.execute(params![id, t, cpu, ram])?;
            }
        }
        tx.commit()
    }

    /// Les points d'un namespace depuis un instant, du plus ancien au plus recent.
    pub fn k8s_historique(&self, contexte: &str, namespace: &str, depuis: i64) -> Result<Vec<Point>> {
        let conn = self.conn();
        let mut req = conn.prepare(
            "SELECT c.pod, m.t, m.cpu, m.ram
               FROM k8s_mesures AS m
               JOIN k8s_cibles  AS c ON c.id = m.cible
              WHERE c.contexte = ?1 AND c.namespace = ?2 AND m.t >= ?3
              ORDER BY m.t",
        )?;
        let lignes = req.query_map(params![contexte, namespace, depuis], |l| {
            Ok(Point {
                pod: l.get(0)?,
                t: l.get(1)?,
                cpu: l.get(2)?,
                ram: l.get(3)?,
            })
        })?;
        lignes.collect()
    }

    /// Ramene le passe a sa juste place : un point par minute au-dela d'une heure, rien au-dela
    /// d'une journee. Rend le nombre de lignes supprimees, ce qui permet de le mesurer.
    pub fn k8s_elaguer(&self, maintenant: i64) -> Result<usize> {
        self.k8s_elaguer_avec(maintenant, RETENTION_MS)
    }

    /// La meme chose, avec la duree de garde choisie par l'utilisateur.
    pub fn k8s_elaguer_avec(&self, maintenant: i64, retention_ms: i64) -> Result<usize> {
        let conn = self.conn();
        let vieux = conn.execute(
            "DELETE FROM k8s_mesures WHERE t < ?1",
            params![maintenant - retention_ms],
        )?;
        // **ON GARDE LE PREMIER POINT DE CHAQUE MINUTE**, pas le dernier : c'est celui qui
        // existe deja dans les courbes affichees, donc la ligne ne se deplace pas sous les yeux
        // au moment ou l'elagage passe.
        let denses = conn.execute(
            "DELETE FROM k8s_mesures AS m
              WHERE m.t < ?1
                AND EXISTS (
                    SELECT 1 FROM k8s_mesures AS autre
                     WHERE autre.cible = m.cible
                       AND autre.t / 60000 = m.t / 60000
                       AND autre.t < m.t
                )",
            params![maintenant - PLEINE_RESOLUTION_MS],
        )?;
        // Un pod qui n'a plus aucun point n'a plus de nom a garder : sinon la table des
        // cibles grossit a chaque redemarrage de pod, pour rien.
        conn.execute(
            "DELETE FROM k8s_cibles WHERE id NOT IN (SELECT DISTINCT cible FROM k8s_mesures)",
            [],
        )?;
        Ok(vieux + denses)
    }

    /// Combien de points sont gardes, toutes cibles confondues. Sert aux essais et au
    /// diagnostic : une table qui grossit sans fin doit pouvoir se constater.
    pub fn k8s_compter(&self) -> Result<i64> {
        self.conn()
            .query_row("SELECT COUNT(*) FROM k8s_mesures", [], |l| l.get(0))
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::Database;

    fn base() -> Database {
        Database::new(":memory:").expect("base en memoire")
    }

    const H: i64 = 3_600_000;

    #[test]
    fn un_tour_de_mesures_se_relit_tel_quel() {
        let db = base();
        db.k8s_noter(
            "prod",
            "equipe",
            1_000,
            &[("web-1".into(), 40, 100), ("web-2".into(), 38, 90)],
        )
        .unwrap();
        let points = db.k8s_historique("prod", "equipe", 0).unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].cpu + points[1].cpu, 78);
    }

    #[test]
    fn deux_namespaces_ne_se_melangent_pas() {
        let db = base();
        db.k8s_noter("prod", "a", 1_000, &[("p".into(), 1, 1)]).unwrap();
        db.k8s_noter("prod", "b", 1_000, &[("p".into(), 2, 2)]).unwrap();
        assert_eq!(db.k8s_historique("prod", "a", 0).unwrap()[0].cpu, 1);
        assert_eq!(db.k8s_historique("prod", "b", 0).unwrap()[0].cpu, 2);
        // Ni deux clusters portant le meme nom de namespace.
        db.k8s_noter("qlf", "a", 1_000, &[("p".into(), 9, 9)]).unwrap();
        assert_eq!(db.k8s_historique("prod", "a", 0).unwrap().len(), 1);
    }

    #[test]
    fn le_meme_instant_ne_s_ecrit_pas_deux_fois() {
        // Une relance de l'ecran peut redemander la meme mesure : elle remplace, elle
        // n'empile pas.
        let db = base();
        db.k8s_noter("prod", "a", 1_000, &[("p".into(), 1, 1)]).unwrap();
        db.k8s_noter("prod", "a", 1_000, &[("p".into(), 5, 5)]).unwrap();
        let points = db.k8s_historique("prod", "a", 0).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].cpu, 5);
    }

    #[test]
    fn au_dela_d_une_journee_on_jette() {
        let db = base();
        let maintenant = 100 * H;
        db.k8s_noter("prod", "a", maintenant - 25 * H, &[("p".into(), 1, 1)]).unwrap();
        db.k8s_noter("prod", "a", maintenant - 2 * H, &[("p".into(), 2, 2)]).unwrap();
        db.k8s_elaguer(maintenant).unwrap();
        let points = db.k8s_historique("prod", "a", 0).unwrap();
        assert_eq!(points.len(), 1, "seule la mesure de moins de 24 h survit");
        assert_eq!(points[0].cpu, 2);
    }

    #[test]
    fn au_dela_d_une_heure_il_reste_un_point_par_minute() {
        // **C'EST CE QUI REND LA TABLE TENABLE.** Sans cette reduction, 25 pods mesures toutes
        // les cinq secondes ecrivent 432 000 lignes par jour et par namespace.
        let db = base();
        let maintenant = 100 * H;
        let vieux = maintenant - 2 * H;
        for i in 0..12 {
            // Douze points dans la meme minute, puis douze dans la suivante.
            db.k8s_noter("prod", "a", vieux + i * 5_000, &[("p".into(), i, i)]).unwrap();
            db.k8s_noter("prod", "a", vieux + 60_000 + i * 5_000, &[("p".into(), i, i)]).unwrap();
        }
        // Et une mesure recente, en pleine resolution : elle ne doit PAS etre touchee.
        for i in 0..5 {
            db.k8s_noter("prod", "a", maintenant - 60_000 + i * 5_000, &[("p".into(), 7, 7)])
                .unwrap();
        }
        assert_eq!(db.k8s_compter().unwrap(), 29);

        db.k8s_elaguer(maintenant).unwrap();

        let points = db.k8s_historique("prod", "a", 0).unwrap();
        let anciens: Vec<_> = points.iter().filter(|p| p.t < maintenant - H).collect();
        assert_eq!(anciens.len(), 2, "une minute, un point");
        assert_eq!(anciens[0].t, vieux, "et c'est le PREMIER de la minute");
        let recents = points.len() - anciens.len();
        assert_eq!(recents, 5, "l'heure ecoulee garde sa pleine resolution");
    }

    #[test]
    fn elaguer_une_base_vide_ne_fait_rien() {
        let db = base();
        assert_eq!(db.k8s_elaguer(0).unwrap(), 0);
        assert_eq!(db.k8s_compter().unwrap(), 0);
    }
}

#[cfg(test)]
mod tests_place {
    use crate::storage::Database;

    #[test]
    fn un_pod_oublie_ne_laisse_pas_son_nom_derriere() {
        // Les pods changent de nom a chaque livraison : sans ce menage, la table des noms
        // grossit indefiniment alors que leurs mesures ont ete jetees depuis longtemps.
        let db = Database::new(":memory:").unwrap();
        let h = 3_600_000i64;
        db.k8s_noter("prod", "a", 10 * h, &[("ancien".into(), 1, 1)]).unwrap();
        db.k8s_noter("prod", "a", 100 * h, &[("recent".into(), 2, 2)]).unwrap();
        db.k8s_elaguer(100 * h).unwrap();
        let noms: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM k8s_cibles", [], |l| l.get(0))
            .unwrap();
        assert_eq!(noms, 1, "seul le pod encore mesure garde son nom");
    }
}
