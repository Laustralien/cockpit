//! Gestion multi-marketplaces des plugins/agents Claude Code.
//!
//! Marketplaces supportes :
//!
//! - **Dossier local, modifiable** : toute marketplace que Claude Code a enregistree comme un
//!   DOSSIER (`claude plugin marketplace add <dossier>`), lue dans
//!   `~/.claude/plugins/known_marketplaces.json`. CRUD complet possible.
//! - **Cache Claude Code** : `~/.claude/plugins/cache/<marketplace-id>/<plugin>/<version>/`.
//!   Lecture seule (ces fichiers sont gere par Claude Code lui-meme).
//!
//! Format d'un plugin :
//! ```text
//! <plugin-root>/
//!   .claude-plugin/plugin.json  (ou plugin.json a la racine)
//!   agents/<agent>.md
//!   commands/<command>.md   (facultatif)
//!   skills/<skill>/SKILL.md  (facultatif)
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------- Types ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketplaceLocation {
    pub id: String,
    pub display_name: String,
    pub path: String,
    pub source_type: String, // "directory" | "cache"
    pub editable: bool,
    pub plugins_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginInfo {
    pub marketplace: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub agents_count: usize,
    pub editable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentInfo {
    pub marketplace: String,
    pub plugin: String,
    pub name: String,
    pub description: String,
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OrchestratorConfig {
    pub experimental_teams_enabled: bool,
    pub teammate_mode: String, // "auto" | "in-process" | "tmux"
    pub default_teammate_model: Option<String>,
    pub marketplaces: Vec<MarketplaceLocation>,
    pub enabled_plugins: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct PluginManifest {
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// **CE FICHIER APPARTIENT A CLAUDE CODE : ON NE REECRIT QUE CE QU'ON CHANGE.** La premiere
/// version ne gardait que les champs qu'elle connaissait : ajouter un plugin effacait `owner`,
/// que Claude Code exige, et posait `"description": null`. Tout ce qui n'est pas decrit ici
/// voyage dans `autres`, et un champ absent n'est pas ecrit (vu au banc le 2026-09-30).
#[derive(Serialize, Deserialize, Debug)]
pub struct MarketplaceManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub plugins: Vec<MarketplacePluginEntry>,
    #[serde(flatten)]
    pub autres: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MarketplacePluginEntry {
    pub name: String,
    /// Un chemin (`"./plugins/x"`) OU un objet (`{"source": "github", ...}`) : les deux formes
    /// existent, et refuser la seconde bloquait toute ecriture dans la marketplace.
    pub source: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub autres: serde_json::Map<String, serde_json::Value>,
}

// ---------- Constantes ----------

/// **LES MARKETPLACES MODIFIABLES SONT CELLES QUE CLAUDE CODE CONNAIT COMME DOSSIERS.**
/// Jusqu'au 2026-09-30, une seule marketplace etait modifiable, celle d'UNE entreprise, a un
/// chemin ecrit en dur, et tout plugin cree devait porter son prefixe : chez n'importe qui
/// d'autre, le bouton « Nouveau plugin » refusait les noms, ou creait un dossier inutile.
/// On lit donc `known_marketplaces.json`, que Claude Code tient lui-meme : une entree dont la
/// source est un dossier (`"source": "directory"`) est modifiable, les autres sont en
/// lecture seule. **L'interface envoie l'IDENTIFIANT, jamais un chemin** : le dossier vient de
/// ce fichier, sinon cet ecran pourrait ecrire n'importe ou sur le disque.
pub(crate) fn marketplaces_locales_depuis(brut: &str) -> Vec<(String, PathBuf)> {
    let Ok(serde_json::Value::Object(tout)) = serde_json::from_str::<serde_json::Value>(brut) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = tout
        .iter()
        .filter_map(|(id, v)| {
            let source = v.get("source")?;
            if source.get("source")?.as_str()? != "directory" {
                return None;
            }
            let chemin = source.get("path")?.as_str()?;
            validate_name(id).ok()?;
            Some((id.clone(), PathBuf::from(chemin)))
        })
        .collect();
    out.sort();
    out
}

fn marketplaces_locales() -> Vec<(String, PathBuf)> {
    let Ok(chemin) = home_path().map(|h| h.join(".claude/plugins/known_marketplaces.json")) else {
        return Vec::new();
    };
    std::fs::read_to_string(chemin)
        .map(|brut| marketplaces_locales_depuis(&brut))
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, p)| p.is_dir())
        .collect()
}

/// Le dossier d'une marketplace MODIFIABLE, ou un refus clair si elle ne l'est pas.
fn racine_modifiable(marketplace_id: &str) -> Result<PathBuf, String> {
    marketplaces_locales()
        .into_iter()
        .find(|(id, _)| id == marketplace_id)
        .map(|(_, p)| p)
        .ok_or_else(|| format!("marketplace '{marketplace_id}' en lecture seule (gérée par Claude Code)"))
}

fn home_path() -> Result<PathBuf, String> {
    crate::chemins::dossier_personnel()
}

fn claude_settings_path() -> Result<PathBuf, String> {
    Ok(home_path()?.join(".claude/settings.json"))
}

fn claude_cache_dir() -> Result<PathBuf, String> {
    Ok(home_path()?.join(".claude/plugins/cache"))
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("nom vide".to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("nom invalide : {}", name));
    }
    Ok(())
}

// ---------- Localisation d'un plugin par marketplace ----------

/// Retourne le chemin du dossier racine d'un plugin (ou None s'il n'existe pas).
fn plugin_root(marketplace_id: &str, plugin: &str) -> Option<PathBuf> {
    validate_name(plugin).ok()?;
    if let Ok(racine) = racine_modifiable(marketplace_id) {
        let p = racine.join("plugins").join(plugin);
        return if p.exists() { Some(p) } else { None };
    }
    // Cache Claude Code
    let plugin_versions_dir = claude_cache_dir().ok()?.join(marketplace_id).join(plugin);
    if !plugin_versions_dir.exists() {
        return None;
    }
    // Trouver le dossier de version le plus recent
    let mut versions: Vec<_> = std::fs::read_dir(&plugin_versions_dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    versions.sort_by_key(|e| e.file_name());
    versions.last().map(|e| e.path())
}

fn plugin_agents_dir(marketplace_id: &str, plugin: &str) -> Option<PathBuf> {
    plugin_root(marketplace_id, plugin).map(|r| r.join("agents"))
}

// ---------- Marketplaces ----------

pub fn list_marketplaces() -> Result<Vec<MarketplaceLocation>, String> {
    let mut out = Vec::new();

    // 1. Les marketplaces en dossier local, modifiables
    let locales = marketplaces_locales();
    for (id, racine) in &locales {
        out.push(MarketplaceLocation {
            id: id.clone(),
            display_name: id.clone(),
            path: racine.to_string_lossy().to_string(),
            source_type: "directory".to_string(),
            editable: true,
            plugins_count: count_plugins_in_dir(&racine.join("plugins")),
        });
    }

    // 2. Cache Claude Code (read-only)
    let cache = claude_cache_dir()?;
    if cache.exists() {
        if let Ok(entries) = std::fs::read_dir(&cache) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().to_string();
                if locales.iter().any(|(l, _)| *l == id) {
                    continue; // deja listee comme dossier local
                }
                let count = count_cached_plugins(&entry.path());
                out.push(MarketplaceLocation {
                    id: id.clone(),
                    display_name: id,
                    path: entry.path().to_string_lossy().to_string(),
                    source_type: "cache".to_string(),
                    editable: false,
                    plugins_count: count,
                });
            }
        }
    }

    out.sort_by(|a, b| {
        // Les modifiables en premier, puis alphabetique
        if a.editable != b.editable {
            b.editable.cmp(&a.editable)
        } else {
            a.display_name.cmp(&b.display_name)
        }
    });
    Ok(out)
}

fn count_plugins_in_dir(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

fn count_cached_plugins(marketplace_cache_dir: &Path) -> usize {
    // dans le cache, on a `<marketplace>/<plugin>/<version>/...`
    std::fs::read_dir(marketplace_cache_dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

// ---------- Plugins / agents listing ----------

pub fn list_plugins_in(marketplace_id: &str) -> Result<Vec<PluginInfo>, String> {
    if let Ok(racine) = racine_modifiable(marketplace_id) {
        return list_local_plugins(marketplace_id, &racine);
    }
    list_cached_plugins(marketplace_id)
}

fn list_local_plugins(marketplace_id: &str, racine: &Path) -> Result<Vec<PluginInfo>, String> {
    let dir = racine.join("plugins");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read_dir: {}", e))?;
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let plugin_path = entry.path();
        let info = read_plugin_metadata(
            marketplace_id,
            &entry.file_name().to_string_lossy(),
            &plugin_path,
            true,
        );
        out.push(info);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn list_cached_plugins(marketplace_id: &str) -> Result<Vec<PluginInfo>, String> {
    validate_name(marketplace_id)?;
    let dir = claude_cache_dir()?.join(marketplace_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read_dir: {}", e))?;
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let plugin_name = entry.file_name().to_string_lossy().to_string();
        // Trouver le sous-dossier de version le plus recent
        let plugin_versions_dir = entry.path();
        let mut versions: Vec<_> = std::fs::read_dir(&plugin_versions_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();
        versions.sort_by_key(|e| e.file_name());
        let Some(latest) = versions.last() else {
            continue;
        };
        let info = read_plugin_metadata(marketplace_id, &plugin_name, &latest.path(), false);
        out.push(info);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn read_plugin_metadata(
    marketplace_id: &str,
    name: &str,
    plugin_path: &Path,
    editable: bool,
) -> PluginInfo {
    let manifest_paths = [
        plugin_path.join("plugin.json"),
        plugin_path.join(".claude-plugin/plugin.json"),
    ];
    for mpath in &manifest_paths {
        if mpath.exists() {
            if let Ok(raw) = std::fs::read_to_string(mpath) {
                if let Ok(m) = serde_json::from_str::<PluginManifest>(&raw) {
                    return PluginInfo {
                        marketplace: marketplace_id.to_string(),
                        name: m.name,
                        version: m.version.unwrap_or_else(|| "0.0.0".to_string()),
                        description: m.description.unwrap_or_default(),
                        agents_count: count_agents(plugin_path),
                        editable,
                    };
                }
            }
        }
    }
    // Fallback : pas de manifest
    PluginInfo {
        marketplace: marketplace_id.to_string(),
        name: name.to_string(),
        version: "0.0.0".to_string(),
        description: String::new(),
        agents_count: count_agents(plugin_path),
        editable,
    }
}

fn count_agents(plugin_path: &Path) -> usize {
    let dir = plugin_path.join("agents");
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(&dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

pub fn list_agents_in(marketplace_id: &str, plugin: &str) -> Result<Vec<AgentInfo>, String> {
    let Some(dir) = plugin_agents_dir(marketplace_id, plugin) else {
        return Ok(Vec::new());
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read_dir agents: {}", e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|x| x != "md").unwrap_or(true) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let (description, model) = parse_frontmatter_lite(&raw);
        out.push(AgentInfo {
            marketplace: marketplace_id.to_string(),
            plugin: plugin.to_string(),
            name: stem,
            description,
            model,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn parse_frontmatter_lite(content: &str) -> (String, Option<String>) {
    let mut description = String::new();
    let mut model: Option<String> = None;
    let mut in_fm = false;
    let mut started = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !started {
                started = true;
                in_fm = true;
                continue;
            } else if in_fm {
                break;
            }
        }
        if !in_fm {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("description:") {
            description = rest.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(rest) = trimmed.strip_prefix("model:") {
            let v = rest.trim().to_string();
            if !v.is_empty() {
                model = Some(v);
            }
        }
    }
    (description, model)
}

// ---------- Lecture / ecriture / rename / delete d'un agent ----------

pub fn read_agent(marketplace_id: &str, plugin: &str, name: &str) -> Result<String, String> {
    validate_name(name)?;
    let Some(dir) = plugin_agents_dir(marketplace_id, plugin) else {
        return Err(format!("plugin introuvable: {}/{}", marketplace_id, plugin));
    };
    let path = dir.join(format!("{}.md", name));
    std::fs::read_to_string(&path).map_err(|e| format!("read {}: {}", path.display(), e))
}


pub fn save_agent(
    marketplace_id: &str,
    plugin: &str,
    name: &str,
    content: &str,
) -> Result<(), String> {
    let racine = racine_modifiable(marketplace_id)?;
    validate_name(plugin)?;
    validate_name(name)?;
    let dir = racine
        .join("plugins")
        .join(plugin)
        .join("agents");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir agents: {}", e))?;
    let path = dir.join(format!("{}.md", name));
    std::fs::write(&path, content).map_err(|e| format!("write {}: {}", path.display(), e))
}

pub fn delete_agent(marketplace_id: &str, plugin: &str, name: &str) -> Result<(), String> {
    let racine = racine_modifiable(marketplace_id)?;
    validate_name(plugin)?;
    validate_name(name)?;
    let path = racine
        .join("plugins")
        .join(plugin)
        .join("agents")
        .join(format!("{}.md", name));
    if !path.exists() {
        return Err(format!("agent inexistant: {}/{}", plugin, name));
    }
    std::fs::remove_file(&path).map_err(|e| format!("remove: {}", e))
}

pub fn rename_agent(
    marketplace_id: &str,
    plugin: &str,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    let racine = racine_modifiable(marketplace_id)?;
    validate_name(plugin)?;
    validate_name(old_name)?;
    validate_name(new_name)?;
    if old_name == new_name {
        return Ok(());
    }
    let dir = racine
        .join("plugins")
        .join(plugin)
        .join("agents");
    let old_path = dir.join(format!("{}.md", old_name));
    let new_path = dir.join(format!("{}.md", new_name));
    if !old_path.exists() {
        return Err(format!("agent inexistant: {}/{}", plugin, old_name));
    }
    if new_path.exists() {
        return Err(format!("destination existe deja: {}/{}", plugin, new_name));
    }
    // Lire, ajuster le `name:` dans le frontmatter, ecrire
    let raw = std::fs::read_to_string(&old_path).map_err(|e| format!("read: {}", e))?;
    let updated = replace_frontmatter_name(&raw, new_name);
    std::fs::write(&new_path, updated).map_err(|e| format!("write: {}", e))?;
    std::fs::remove_file(&old_path).map_err(|e| format!("remove old: {}", e))?;
    Ok(())
}

fn replace_frontmatter_name(content: &str, new_name: &str) -> String {
    let mut out = String::new();
    let mut in_fm = false;
    let mut started = false;
    let mut replaced = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !started {
                started = true;
                in_fm = true;
            } else if in_fm {
                in_fm = false;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fm && !replaced && trimmed.starts_with("name:") {
            out.push_str(&format!("name: {}\n", new_name));
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

// ---------- Plugin scaffold / rename / delete ----------

pub fn create_plugin(marketplace_id: &str, name: &str, description: &str) -> Result<(), String> {
    let racine = racine_modifiable(marketplace_id)?;
    validate_name(name)?;
    let dir = racine.join("plugins").join(name);
    if dir.exists() {
        return Err(format!("le plugin '{}' existe deja", name));
    }
    std::fs::create_dir_all(dir.join("agents")).map_err(|e| format!("mkdir: {}", e))?;
    let manifest = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "description": description,
        // PAS de champ `author` : il est facultatif, et il portait en dur un nom d'entreprise
        // et une adresse e-mail reelle, recopies dans le plugin de chaque utilisateur.
    });
    let manifest_str = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("json: {}", e))?;
    std::fs::write(dir.join("plugin.json"), manifest_str + "\n")
        .map_err(|e| format!("write plugin.json: {}", e))?;
    let readme = format!(
        "# {}\n\n{}\n\n## Agents\n\n(aucun pour l'instant)\n",
        name, description
    );
    std::fs::write(dir.join("README.md"), readme)
        .map_err(|e| format!("write README: {}", e))?;
    add_plugin_to_manifest(&racine, name, description)?;
    Ok(())
}

fn add_plugin_to_manifest(racine: &Path, plugin_name: &str, description: &str) -> Result<(), String> {
    let manifest_path = racine.join(".claude-plugin/marketplace.json");
    if !manifest_path.exists() {
        return Err("marketplace.json introuvable".to_string());
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read marketplace.json: {}", e))?;
    let mut manifest: MarketplaceManifest = serde_json::from_str(&raw)
        .map_err(|e| format!("parse marketplace.json: {}", e))?;
    if manifest.plugins.iter().any(|p| p.name == plugin_name) {
        return Ok(());
    }
    manifest.plugins.push(MarketplacePluginEntry {
        name: plugin_name.to_string(),
        source: serde_json::Value::String(format!("./plugins/{}", plugin_name)),
        description: Some(description.to_string()).filter(|d| !d.is_empty()),
        version: Some("0.1.0".to_string()),
        tags: Vec::new(),
        autres: serde_json::Map::new(),
    });
    let out = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(&manifest_path, out + "\n")
        .map_err(|e| format!("write marketplace.json: {}", e))?;
    Ok(())
}

pub fn delete_plugin(marketplace_id: &str, name: &str) -> Result<(), String> {
    let racine = racine_modifiable(marketplace_id)?;
    validate_name(name)?;
    let dir = racine.join("plugins").join(name);
    if !dir.exists() {
        return Err(format!("plugin inexistant : {}", name));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("remove plugin: {}", e))?;
    remove_plugin_from_manifest(&racine, name)?;
    Ok(())
}

fn remove_plugin_from_manifest(racine: &Path, plugin_name: &str) -> Result<(), String> {
    let manifest_path = racine.join(".claude-plugin/marketplace.json");
    if !manifest_path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read marketplace.json: {}", e))?;
    let mut manifest: MarketplaceManifest = serde_json::from_str(&raw)
        .map_err(|e| format!("parse marketplace.json: {}", e))?;
    let before = manifest.plugins.len();
    manifest.plugins.retain(|p| p.name != plugin_name);
    if manifest.plugins.len() == before {
        return Ok(());
    }
    let out = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(&manifest_path, out + "\n")
        .map_err(|e| format!("write marketplace.json: {}", e))?;
    Ok(())
}

pub fn rename_plugin(
    marketplace_id: &str,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    let racine = racine_modifiable(marketplace_id)?;
    validate_name(old_name)?;
    validate_name(new_name)?;
    if old_name == new_name {
        return Ok(());
    }
    let base = racine.join("plugins");
    let old_dir = base.join(old_name);
    let new_dir = base.join(new_name);
    if !old_dir.exists() {
        return Err(format!("plugin inexistant : {}", old_name));
    }
    if new_dir.exists() {
        return Err(format!("plugin destination existe deja : {}", new_name));
    }
    std::fs::rename(&old_dir, &new_dir).map_err(|e| format!("rename: {}", e))?;

    // Mettre a jour plugin.json
    let manifest_paths = [
        new_dir.join("plugin.json"),
        new_dir.join(".claude-plugin/plugin.json"),
    ];
    for mpath in &manifest_paths {
        if mpath.exists() {
            if let Ok(raw) = std::fs::read_to_string(mpath) {
                if let Ok(mut m) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let serde_json::Value::Object(ref mut obj) = m {
                        obj.insert("name".to_string(), serde_json::Value::String(new_name.to_string()));
                    }
                    let out = serde_json::to_string_pretty(&m).unwrap_or(raw);
                    let _ = std::fs::write(mpath, out + "\n");
                }
            }
        }
    }

    // Mettre a jour marketplace.json (entry name + source path)
    let market_manifest = racine.join(".claude-plugin/marketplace.json");
    if market_manifest.exists() {
        if let Ok(raw) = std::fs::read_to_string(&market_manifest) {
            if let Ok(mut manifest) = serde_json::from_str::<MarketplaceManifest>(&raw) {
                for entry in &mut manifest.plugins {
                    if entry.name == old_name {
                        entry.name = new_name.to_string();
                        entry.source = serde_json::Value::String(format!("./plugins/{}", new_name));
                    }
                }
                if let Ok(out) = serde_json::to_string_pretty(&manifest) {
                    let _ = std::fs::write(&market_manifest, out + "\n");
                }
            }
        }
    }
    Ok(())
}

// ---------- Plugins actives par projet ----------

pub fn get_project_plugins(project_path: &str) -> Result<Vec<String>, String> {
    let settings = PathBuf::from(project_path).join(".claude/settings.json");
    if !settings.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&settings)
        .map_err(|e| format!("read settings: {}", e))?;
    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("parse settings: {}", e))?;
    let plugins = v.get("enabledPlugins");
    match plugins {
        Some(serde_json::Value::Array(arr)) => Ok(arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect()),
        Some(serde_json::Value::Object(obj)) => Ok(obj
            .iter()
            .filter(|(_, v)| v.as_bool().unwrap_or(false))
            .map(|(k, _)| k.split('@').next().unwrap_or(k).to_string())
            .collect()),
        _ => Ok(Vec::new()),
    }
}

pub fn set_project_plugins(project_path: &str, plugins: Vec<String>) -> Result<(), String> {
    let dir = PathBuf::from(project_path).join(".claude");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir .claude: {}", e))?;
    let settings_path = dir.join("settings.json");
    let mut v: serde_json::Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)
            .map_err(|e| format!("read settings: {}", e))?;
        serde_json::from_str(&raw).map_err(|e| format!("parse settings: {}", e))?
    } else {
        serde_json::json!({})
    };
    if let serde_json::Value::Object(ref mut m) = v {
        m.insert(
            "enabledPlugins".to_string(),
            serde_json::Value::Array(
                plugins
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    let out = serde_json::to_string_pretty(&v)
        .map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(&settings_path, out + "\n")
        .map_err(|e| format!("write settings: {}", e))
}

// ---------- Orchestrator / global Claude settings ----------

fn read_claude_settings() -> Result<serde_json::Value, String> {
    let path = claude_settings_path()?;
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read settings: {}", e))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse settings: {}", e))
}

fn write_claude_settings(v: &serde_json::Value) -> Result<(), String> {
    let out = serde_json::to_string_pretty(v)
        .map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(claude_settings_path()?, out + "\n")
        .map_err(|e| format!("write settings: {}", e))
}

pub fn get_orchestrator_config() -> Result<OrchestratorConfig, String> {
    let settings = read_claude_settings()?;

    let experimental_teams_enabled = settings
        .get("env")
        .and_then(|e| e.get("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1" || s == "true")
        .unwrap_or(false);

    let teammate_mode = settings
        .get("teammateMode")
        .and_then(|v| v.as_str())
        .unwrap_or("auto")
        .to_string();

    let default_teammate_model = settings
        .get("defaultTeammateModel")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let enabled_plugins: Vec<String> = match settings.get("enabledPlugins") {
        Some(serde_json::Value::Object(obj)) => obj
            .iter()
            .filter(|(_, v)| v.as_bool().unwrap_or(false))
            .map(|(k, _)| k.clone())
            .collect(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    };

    let marketplaces = list_marketplaces()?;

    Ok(OrchestratorConfig {
        experimental_teams_enabled,
        teammate_mode,
        default_teammate_model,
        marketplaces,
        enabled_plugins,
    })
}

pub fn set_teams_enabled(enabled: bool) -> Result<(), String> {
    let mut settings = read_claude_settings()?;
    let env = settings
        .as_object_mut()
        .ok_or("settings invalide")?
        .entry("env".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let serde_json::Value::Object(env_obj) = env {
        if enabled {
            env_obj.insert(
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env_obj.remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS");
        }
    }
    write_claude_settings(&settings)
}

pub fn set_teammate_mode(mode: &str) -> Result<(), String> {
    if !["auto", "in-process", "tmux"].contains(&mode) {
        return Err(format!("mode invalide: {}", mode));
    }
    let mut settings = read_claude_settings()?;
    if let serde_json::Value::Object(ref mut obj) = settings {
        if mode == "auto" {
            obj.remove("teammateMode");
        } else {
            obj.insert(
                "teammateMode".to_string(),
                serde_json::Value::String(mode.to_string()),
            );
        }
    }
    write_claude_settings(&settings)
}

pub fn toggle_plugin_enabled(plugin_key: &str, enabled: bool) -> Result<(), String> {
    let mut settings = read_claude_settings()?;
    let plugins = settings
        .as_object_mut()
        .ok_or("settings invalide")?
        .entry("enabledPlugins".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let serde_json::Value::Object(p_obj) = plugins {
        if enabled {
            p_obj.insert(plugin_key.to_string(), serde_json::Value::Bool(true));
        } else {
            p_obj.remove(plugin_key);
        }
    }
    write_claude_settings(&settings)
}

#[cfg(test)]
mod tests_marketplaces {
    use super::*;

    #[test]
    fn seules_les_marketplaces_en_dossier_sont_modifiables() {
        let brut = r#"{
          "officielle": {"source": {"source": "github", "repo": "exemple/plugins", "path": "/home/u/.claude/plugins/marketplaces/officielle"}},
          "equipe": {"source": {"source": "directory", "path": "/srv/marketplace-equipe"}},
          "../sortie": {"source": {"source": "directory", "path": "/tmp/x"}}
        }"#;
        let l = marketplaces_locales_depuis(brut);
        assert_eq!(l, vec![("equipe".to_string(), PathBuf::from("/srv/marketplace-equipe"))]);
    }

    #[test]
    fn reecrire_la_marketplace_garde_ce_qu_on_ne_connait_pas() {
        let brut = r#"{"name": "equipe", "owner": {"name": "Equipe"},
          "plugins": [{"name": "a", "source": {"source": "github", "repo": "x/a"}, "category": "outils"}]}"#;
        let mut m: MarketplaceManifest = serde_json::from_str(brut).unwrap();
        m.plugins.push(MarketplacePluginEntry {
            name: "b".into(),
            source: serde_json::Value::String("./plugins/b".into()),
            description: None,
            version: Some("0.1.0".into()),
            tags: Vec::new(),
            autres: serde_json::Map::new(),
        });
        let relu: serde_json::Value = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(relu["owner"]["name"], "Equipe", "owner est exige par Claude Code");
        assert_eq!(relu["plugins"][0]["source"]["repo"], "x/a");
        assert_eq!(relu["plugins"][0]["category"], "outils");
        assert!(relu.get("description").is_none(), "pas de description: null");
        assert!(relu["plugins"][1].get("description").is_none());
    }

    #[test]
    fn un_fichier_illisible_ne_rend_rien() {
        assert!(marketplaces_locales_depuis("pas du json").is_empty());
        assert!(marketplaces_locales_depuis("[]").is_empty());
    }
}
