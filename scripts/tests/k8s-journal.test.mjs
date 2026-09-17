/**
 * Essais de la vue des logs (src/lib/k8s/journal.ts).
 *
 * Ce qu'ils gardent : une console qui ramene en bas quelqu'un en train de lire, et une page qui
 * grossit sans fin sur un pod bavard. Les deux rendent les logs inutilisables au moment ou on
 * en a le plus besoin.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  doitSuivre, decouper, compterLesLignes, ajouter, separerLHeure, MARGE_DU_BAS,
} from "../../src/lib/k8s/journal.ts";

test("on suit le bas tant qu'on y est", () => {
  // Pile en bas, et a quelques pixels : c'est « en bas ».
  assert.equal(doitSuivre(900, 1000, 100), true);
  assert.equal(doitSuivre(900 - MARGE_DU_BAS, 1000, 100), true);
});

test("remonter dans les logs coupe le suivi, revenir en bas le reprend", () => {
  // Le defaut classique des consoles : on remonte pour lire, une ligne arrive, on est ramene
  // en bas et on perd sa place.
  assert.equal(doitSuivre(300, 1000, 100), false, "on lit plus haut : on ne bouge plus");
  assert.equal(doitSuivre(0, 1000, 100), false, "tout en haut non plus");
  assert.equal(doitSuivre(900, 1000, 100), true, "de retour en bas : on suit de nouveau");
});

test("une vue plus courte que son contenu d'un cheveu suit quand meme", () => {
  assert.equal(doitSuivre(0, 100, 100), true, "rien a defiler, donc on est en bas");
});

test("la recherche decoupe la ligne pour surligner, sans rien perdre", () => {
  const morceaux = decouper("ERROR: connexion refusee, error 42", "error");
  assert.equal(morceaux.map((m) => m.texte).join(""), "ERROR: connexion refusee, error 42");
  assert.deepEqual(
    morceaux.filter((m) => m.trouve).map((m) => m.texte),
    ["ERROR", "error"],
    "les deux casses correspondent, et la ligne garde son texte d'origine",
  );
});

test("sans recherche, la ligne reste entiere et non surlignee", () => {
  const morceaux = decouper("une ligne", "  ");
  assert.deepEqual(morceaux, [{ texte: "une ligne", trouve: false }]);
});

test("le compteur dit combien de LIGNES contiennent la recherche", () => {
  const lignes = ["erreur ici", "tout va bien", "encore une Erreur"];
  assert.equal(compterLesLignes(lignes, "erreur"), 2);
  assert.equal(compterLesLignes(lignes, ""), 0);
  assert.equal(compterLesLignes(lignes, "introuvable"), 0);
});

test("ce qui arrive du flux s'ajoute a la suite", () => {
  const apres = ajouter(["une"], "deux\ntrois\n");
  assert.deepEqual(apres, ["une", "deux", "trois"]);
  assert.deepEqual(ajouter(["une"], ""), ["une"], "un flux muet ne change rien");
});

test("un pod bavard ne fait pas grossir la page sans fin", () => {
  const debut = Array.from({ length: 10 }, (_, i) => `ligne ${i}`);
  const apres = ajouter(debut, "neuve-1\nneuve-2\n", 5);
  assert.equal(apres.length, 5);
  assert.equal(apres.at(-1), "neuve-2", "ce sont les DERNIERES qu'on garde");
  assert.equal(apres.at(0), "ligne 7");
});

test("l'horodatage du cluster se detache du message", () => {
  const { heure, texte } = separerLHeure("2026-09-17T15:00:45.406134289Z demarrage termine");
  assert.equal(heure, "15:00:45");
  assert.equal(texte, "demarrage termine");
});

test("une ligne sans horodatage reste intacte", () => {
  // Les logs demandes sans horodatage, ou une ligne de trace : on ne coupe pas au hasard.
  assert.deepEqual(separerLHeure("at main.rs:42 panic"), { heure: "", texte: "at main.rs:42 panic" });
  assert.deepEqual(separerLHeure("2026-09-17 pas une date ISO"), {
    heure: "",
    texte: "2026-09-17 pas une date ISO",
  });
});
