/**
 * Essais de la charge confiee au pont (src/lib/coquille.ts).
 *
 * Ce qu'ils gardent : « Error: An object could not be cloned. » Le pont d'Electron clone ce
 * qu'on lui donne et refuse un Proxy, or Svelte 5 represente tout `$state` par un Proxy.
 * Enregistrer un second namespace echouait donc des qu'un premier existait.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { aplatir } from "../../src/lib/coquille.ts";

/** Ce que Svelte pose autour d'une valeur reactive : un Proxy, que le clonage refuse. */
function commeUnEtat(valeur) {
  return new Proxy(valeur, {});
}

test("une charge venue de l'etat traverse le clonage", () => {
  const cibles = commeUnEtat([
    commeUnEtat({ contexte: "prod", namespace: "equipe-a", periode: 300, actif: true }),
    { contexte: "prod", namespace: "equipe-b", periode: 60, actif: true },
  ]);
  const plat = aplatir({ reglages: { cibles, retention_heures: 24 } });
  // Le vrai juge : le clonage structure, celui-la meme que le pont applique.
  const clone = structuredClone(plat);
  assert.equal(clone.reglages.cibles.length, 2);
  assert.equal(clone.reglages.cibles[0].namespace, "equipe-a");
  assert.equal(clone.reglages.retention_heures, 24);
});

test("sans la remise a plat, le clonage echoue", () => {
  // **L'ESSAI DOIT POUVOIR TOMBER.** Si le clonage acceptait les Proxy, le precedent ne
  // prouverait rien.
  const charge = { reglages: { cibles: [commeUnEtat({ a: 1 })] } };
  assert.throws(() => structuredClone(charge), /clone/i);
});

test("une charge de valeurs simples n'est pas recopiee", () => {
  // Le chemin de frappe passe par la : rien ne doit s'y ajouter, pas meme une allocation.
  const frappe = { id: 12, data: "a", vrai: true, rien: undefined };
  assert.equal(aplatir(frappe), frappe, "c'est le MEME objet, pas une copie");
});

test("une charge vide passe sans bruit", () => {
  const vide = {};
  assert.equal(aplatir(vide), vide);
});

test("null ne compte pas comme un objet a recopier", () => {
  // `typeof null === "object"` : sans la garde, toute commande portant un champ absent
  // paierait une recopie.
  const charge = { id: 3, precedent: null };
  assert.equal(aplatir(charge), charge);
});

test("un tableau de valeurs simples est quand meme remis a plat", () => {
  // Il peut lui-meme etre un Proxy d'etat : c'est le cas d'une liste reactive.
  const charge = { noms: commeUnEtat(["a", "b"]) };
  const plat = aplatir(charge);
  assert.notEqual(plat, charge);
  assert.deepEqual(structuredClone(plat).noms, ["a", "b"]);
});
