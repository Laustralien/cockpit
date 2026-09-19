/**
 * Essais de l'etat d'un agent dans un terminal (src/lib/terminaux/activite.ts).
 *
 * Ce que ces essais gardent : un terminal jamais ouvert annonce a tort « il t'attend », et un
 * « fini » qui s'efface tout seul avant qu'on l'ait vu. Les deux font perdre confiance au
 * repere, et un repere auquel on ne croit plus ne sert a rien.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { prochainEtat, meriteUnRepere, SILENCE_MS } from "../../src/lib/terminaux/activite.ts";

const T = 1_000_000;

test("un agent qui ecrit travaille", () => {
  const s = prochainEtat(undefined, true, T - 500, T);
  assert.equal(s.etat, "en-cours");
  assert.equal(s.agentAvant, true);
});

test("un agent silencieux attend une reponse", () => {
  assert.equal(prochainEtat(undefined, true, T - SILENCE_MS, T).etat, "attend");
  assert.equal(prochainEtat(undefined, true, T - SILENCE_MS - 1, T).etat, "attend");
  // Juste en dessous du seuil : il travaille encore.
  assert.equal(prochainEtat(undefined, true, T - SILENCE_MS + 1, T).etat, "en-cours");
});

test("un terminal jamais ouvert ne dit JAMAIS qu'il attend", () => {
  // Aucune sortie observee : on ne sait pas. Annoncer « il t'attend » enverrait traverser
  // l'ecran pour un agent qui travaille.
  assert.equal(prochainEtat(undefined, true, undefined, T).etat, "en-cours");
});

test("un agent qui rend la main passe a « fini »", () => {
  const pendant = prochainEtat(undefined, true, T - 100, T);
  const apres = prochainEtat(pendant, false, T - 100, T + 1000);
  assert.equal(apres.etat, "fini");
  assert.equal(apres.agentAvant, false);
});

test("« fini » reste tant qu'on n'est pas retourne voir", () => {
  let s = prochainEtat(undefined, true, T, T);
  s = prochainEtat(s, false, T, T + 1000);
  assert.equal(s.etat, "fini");
  // Plusieurs passages plus tard, toujours la : c'est l'information qu'on a demandee.
  for (let i = 0; i < 20; i++) s = prochainEtat(s, false, T, T + 2000 + i * 1000);
  assert.equal(s.etat, "fini");
});

test("un terminal sans agent ne dit rien", () => {
  const s = prochainEtat(undefined, false, undefined, T);
  assert.equal(s.etat, "aucun");
  // Et il le reste, meme apres plusieurs passages.
  assert.equal(prochainEtat(s, false, undefined, T + 9999).etat, "aucun");
});

test("repartir efface l'attente", () => {
  let s = prochainEtat(undefined, true, T - SILENCE_MS - 1, T);
  assert.equal(s.etat, "attend");
  // La sortie reprend : l'agent travaille de nouveau.
  s = prochainEtat(s, true, T + 100, T + 200);
  assert.equal(s.etat, "en-cours");
});

test("un agent relance apres avoir fini repart proprement", () => {
  let s = prochainEtat(undefined, true, T, T);
  s = prochainEtat(s, false, T, T + 1000);
  assert.equal(s.etat, "fini");
  s = prochainEtat(s, true, T + 2000, T + 2100);
  assert.equal(s.etat, "en-cours", "« fini » ne colle pas au terminal une fois relance");
});

test("seuls l'attente et la fin meritent un repere", () => {
  assert.equal(meriteUnRepere("attend"), true);
  assert.equal(meriteUnRepere("fini"), true);
  // « En cours » est l'ordinaire : le signaler mettrait un repere sur la moitie de la liste,
  // et plus rien ne ressortirait.
  assert.equal(meriteUnRepere("en-cours"), false);
  assert.equal(meriteUnRepere("aucun"), false);
});

// --- Le terminal qu'on REGARDE ---------------------------------------------------------

test("un terminal sous les yeux ne porte aucun repere", () => {
  // Le defaut livre en 0.74.0 : le repere s'effacait au clic, puis le calcul suivant le
  // remettait une seconde plus tard, puisque l'agent attendait toujours. Le cadre
  // disparaissait et revenait sous les yeux de l'utilisateur.
  const attend = prochainEtat(undefined, true, T - SILENCE_MS - 1, T);
  assert.equal(attend.etat, "attend");
  const regarde = prochainEtat(attend, true, T - SILENCE_MS - 1, T + 1000, SILENCE_MS, true);
  assert.equal(regarde.etat, "aucun", "on le voit, il n'a rien a signaler");
});

test("le repere revient quand on regarde ailleurs", () => {
  let s = prochainEtat(undefined, true, T - SILENCE_MS - 1, T, SILENCE_MS, true);
  assert.equal(s.etat, "aucun");
  s = prochainEtat(s, true, T - SILENCE_MS - 1, T + 1000, SILENCE_MS, false);
  assert.equal(s.etat, "attend", "l'agent attend toujours : on le redit");
});

test("regarder un terminal qui vient de finir consomme l'information", () => {
  let s = prochainEtat(undefined, true, T, T);
  // Il rend la main pendant qu'on le regarde : rien a signaler, on l'a vu.
  s = prochainEtat(s, false, T, T + 1000, SILENCE_MS, true);
  assert.equal(s.etat, "aucun");
  // Et « fini » ne reapparait pas quand on part.
  s = prochainEtat(s, false, T, T + 2000, SILENCE_MS, false);
  assert.equal(s.etat, "aucun", "l'information a ete vue, elle ne revient pas");
});

test("« fini » survit a un passage ailleurs, puis se consomme en regardant", () => {
  let s = prochainEtat(undefined, true, T, T);
  s = prochainEtat(s, false, T, T + 1000);
  assert.equal(s.etat, "fini");
  s = prochainEtat(s, false, T, T + 2000, SILENCE_MS, true);
  assert.equal(s.etat, "aucun", "vu");
  s = prochainEtat(s, false, T, T + 3000, SILENCE_MS, false);
  assert.equal(s.etat, "aucun", "et il ne revient pas");
});

// ── Se brancher pour observer ────────────────────────────────────────────────────────────────

test("on se branche sur un terminal d'agent qu'on n'a jamais ouvert", async () => {
  const { doitObserver } = await import("../../src/lib/terminaux/activite.ts");
  const agent = { llm: true, alive: true, cols: 120, rows: 30 };
  assert.equal(doitObserver(agent, false, false), true);
  // Deja branche, ou deja vu ecrire : il n'y a plus rien a demander.
  assert.equal(doitObserver(agent, true, false), false);
  assert.equal(doitObserver(agent, false, true), false);
});

test("on ne se branche jamais sur ce qui ferait du degat", async () => {
  const { doitObserver } = await import("../../src/lib/terminaux/activite.ts");
  // Pas d'agent : l'ecran et l'historique de chaque terminal de chaque projet, pour rien.
  assert.equal(doitObserver({ llm: false, alive: true, cols: 120, rows: 30 }, false, false), false);
  // Session morte : se brancher ROUVRIRAIT un shell, ce qu'un demarrage ne doit jamais faire.
  assert.equal(doitObserver({ llm: true, alive: false, cols: 120, rows: 30 }, false, false), false);
  // Taille inconnue : on redimensionnerait un terminal qu'on n'affiche meme pas, et une
  // application plein ecran ne s'en remet pas.
  assert.equal(doitObserver({ llm: true, alive: true, cols: 0, rows: 30 }, false, false), false);
  assert.equal(doitObserver({ llm: true, alive: true, cols: 120, rows: 0 }, false, false), false);
});
