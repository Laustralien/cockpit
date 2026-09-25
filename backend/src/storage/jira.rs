//! La liaison d'un projet a Jira, telle qu'elle est rangee. Brut : l'interpretation (decoupage
//! des cles, gabarit par defaut) appartient a `crate::jira::config`.

use super::db::Database;

impl Database {
    pub fn get_project_jira(&self, project: &str) -> Result<(Option<String>, Option<String>), String> {
        self.conn()
            .query_row(
                "SELECT jira_cles, jira_gabarit_branche FROM projects WHERE name=?1",
                [project],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| e.to_string())
    }

    pub fn set_project_jira(&self, project: &str, cles: Option<&str>, gabarit: Option<&str>) -> Result<(), String> {
        let n = self
            .conn()
            .execute(
                "UPDATE projects SET jira_cles=?1, jira_gabarit_branche=?2 WHERE name=?3",
                rusqlite::params![cles, gabarit, project],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("projet inconnu : {project}"));
        }
        Ok(())
    }

    /// Les projets lies a au moins une cle : (nom, cles brutes, gabarit).
    pub fn list_project_jira(&self) -> Result<Vec<(String, String, Option<String>)>, String> {
        let conn = self.conn();
        let mut requete = conn
            .prepare(
                "SELECT name, jira_cles, jira_gabarit_branche FROM projects
                 WHERE jira_cles IS NOT NULL AND jira_cles != '' ORDER BY position, name",
            )
            .map_err(|e| e.to_string())?;
        let lignes = requete
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        let liste = lignes.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        Ok(liste)
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::db::Database;

    #[test]
    fn range_et_relit_la_liaison_d_un_projet() {
        let db = Database::new(":memory:").unwrap();
        db.create_project("site", "/tmp/site", "", "", &[]).unwrap();
        db.create_project("autre", "/tmp/autre", "", "", &[]).unwrap();
        assert_eq!(db.get_project_jira("site").unwrap(), (None, None));

        db.set_project_jira("site", Some("CCM,ABC"), Some("{type}/tl/{cle}/{slug}")).unwrap();
        assert_eq!(
            db.get_project_jira("site").unwrap(),
            (Some("CCM,ABC".into()), Some("{type}/tl/{cle}/{slug}".into()))
        );

        let liste = db.list_project_jira().unwrap();
        assert_eq!(liste, vec![("site".into(), "CCM,ABC".into(), Some("{type}/tl/{cle}/{slug}".into()))]);
    }

    #[test]
    fn un_projet_inconnu_est_une_erreur() {
        let db = Database::new(":memory:").unwrap();
        assert!(db.set_project_jira("fantome", Some("CCM"), None).is_err());
    }
}
