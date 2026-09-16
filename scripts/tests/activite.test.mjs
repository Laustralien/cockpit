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
