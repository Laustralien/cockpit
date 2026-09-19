/**
 * Aucun composant ne demande une couleur qui n'existe pas.
 *
 * **UN TOKEN INCONNU NE LEVE RIEN : LA DECLARATION EST SIMPLEMENT IGNOREE.** Constate le
 * 2026-09-19 : vingt-huit endroits demandaient `var(--border)` alors que le token s'appelle
 * `--border-color`. Les cadres des graphiques n'avaient donc ni bordure ni grille, et personne
 * ne pouvait le voir autrement qu'en comparant au dessin attendu. C'est exactement le genre de
 * faute qu'une relecture ne rattrape pas et qu'un essai attrape en une seconde.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

function fichiers(racine, suffixes) {
  const trouves = [];
  for (const nom of readdirSync(racine)) {
    const chemin = join(racine, nom);
    if (statSync(chemin).isDirectory()) trouves.push(...fichiers(chemin, suffixes));
    else if (suffixes.some((s) => nom.endsWith(s))) trouves.push(chemin);
  }
  return trouves;
}

/** Les tokens DEFINIS : feuilles globales, styles de composants, et ceux poses depuis le code. */
function tokensDefinis() {
  const definis = new Set();
  for (const f of fichiers("src", [".css", ".svelte", ".ts"])) {
    const texte = readFileSync(f, "utf8");
    // Les accents comptent : un token peut s'appeler `--liseré`, et le couper au premier
    // caractere non latin le rendrait introuvable.
    for (const m of texte.matchAll(/(--[\w\u00c0-\u024f-]+)\s*[:=]/g)) definis.add(m[1]);
    // `setProperty("--x", …)` : pose un token depuis le code, il compte aussi.
    for (const m of texte.matchAll(/setProperty\(\s*["'`](--[a-z0-9-]+)/g)) definis.add(m[1]);
  }
  return definis;
}

test("chaque var(--token) demande un token qui existe", () => {
  const definis = tokensDefinis();
  const manquants = new Map();
  for (const f of fichiers("src", [".css", ".svelte", ".ts"])) {
    const texte = readFileSync(f, "utf8");
    for (const m of texte.matchAll(/var\(\s*(--[\w\u00c0-\u024f-]+)(\$\{)?/g)) {
      // `var(--serie-${i})` designe un token construit : son nom n'existe qu'a l'execution.
      if (!m[2] && !definis.has(m[1])) {
        manquants.set(m[1], [...(manquants.get(m[1]) ?? []), f]);
      }
    }
  }
  assert.deepEqual(
    [...manquants].map(([t, ou]) => `${t} (${ou.length} endroit(s), ex. ${ou[0]})`),
    [],
    "ces tokens sont demandes mais definis nulle part : la declaration est ignoree en silence",
  );
});

test("l'essai sait reconnaitre un token inconnu", () => {
  // **UN ESSAI QUI NE PEUT PAS ECHOUER NE PROUVE RIEN.** On lui donne de quoi tomber.
  const definis = tokensDefinis();
  assert.ok(definis.has("--border-color"), "le token de bordure existe bien");
  assert.ok(!definis.has("--border"), "et l'ancien nom, lui, n'existe pas");
});
