/**
 * Essais de l'historique et du trace (src/lib/k8s/mesures.ts).
 *
 * Ce qu'ils gardent : une courbe qui comprime les trous (et ment sur le temps), une echelle qui
 * saute a chaque point, et un ecran laisse ouvert qui grossit sans fin.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  noter, fenetre, lesPlusGourmands, borneHaute, graduations, points, ligne, aire,
  leplusProche, dureeCourte, heureDe, periodesOffertes, couverture, depuisEnregistre, fusionner, TOTAL,
} from "../../src/lib/k8s/mesures.ts";

const T = 1_700_000_000_000;
const CADRE = { largeur: 100, hauteur: 50 };

test("chaque mesure s'ajoute a l'historique du pod, et au total du namespace", () => {
  let h = noter(new Map(), [{ nom: "a", cpu: 10, ram: 100 }, { nom: "b", cpu: 5, ram: 50 }], T);
  h = noter(h, [{ nom: "a", cpu: 20, ram: 200 }, { nom: "b", cpu: 5, ram: 50 }], T + 5000);
  assert.equal(h.get("a").length, 2);
  assert.deepEqual(h.get("a").at(-1), { t: T + 5000, cpu: 20, ram: 200 });
  assert.deepEqual(h.get(TOTAL).at(-1), { t: T + 5000, cpu: 25, ram: 250 });
});

test("un pod qui cesse d'etre mesure garde sa courbe", () => {
  // Il s'est arrete : on veut voir OU elle s'arrete, pas la voir disparaitre.
  let h = noter(new Map(), [{ nom: "a", cpu: 10, ram: 100 }], T);
  h = noter(h, [], T + 5000);
  assert.equal(h.get("a").length, 1);
  assert.equal(h.get(TOTAL).at(-1).cpu, 0, "le total, lui, retombe a zero");
});

test("un ecran laisse ouvert ne grossit pas sans fin", () => {
  let h = new Map();
  for (let i = 0; i < 20; i++) h = noter(h, [{ nom: "a", cpu: i, ram: i }], T + i * 1000, 5);
  assert.equal(h.get("a").length, 5);
  assert.equal(h.get("a").at(-1).cpu, 19, "ce sont les DERNIERS points qu'on garde");
});

test("la fenetre ne garde que ce qui est dans la periode demandee", () => {
  let h = new Map();
  for (let i = 0; i < 10; i++) h = noter(h, [{ nom: "a", cpu: i, ram: i }], T + i * 60_000);
  const recents = fenetre(h.get("a"), T + 7 * 60_000);
  assert.equal(recents.length, 3);
  assert.equal(recents[0].cpu, 7);
  assert.deepEqual(fenetre(undefined, T), [], "une serie inconnue ne casse rien");
});

test("les plus gourmands sont tries, et bornes", () => {
  const m = [
    { nom: "petit", cpu: 1, ram: 900 },
    { nom: "gros", cpu: 50, ram: 10 },
    { nom: "moyen", cpu: 20, ram: 100 },
  ];
  assert.deepEqual(lesPlusGourmands(m, "cpu").map((x) => x.nom), ["gros", "moyen", "petit"]);
  assert.deepEqual(lesPlusGourmands(m, "ram").map((x) => x.nom), ["petit", "moyen", "gros"]);
  assert.equal(lesPlusGourmands(m, "cpu", 2).length, 2);
});

test("l'echelle prend de la marge et reste sur des paliers ronds", () => {
  // Une courbe collee au haut du cadre ne dit plus si elle plafonne ou si elle deborde.
  assert.equal(borneHaute([42]), 50);
  assert.equal(borneHaute([180]), 200);
  assert.equal(borneHaute([0]), 1, "sans mesure, une echelle vide plutot qu'une division par zero");
  assert.equal(borneHaute([]), 1);
  assert.deepEqual(graduations(50), [0, 25, 50]);
});

test("l'echelle ne saute pas a chaque point", () => {
  // Deux mesures voisines doivent donner la MEME echelle, sinon la courbe se deforme sous
  // les yeux a chaque tour.
  assert.equal(borneHaute([41]), borneHaute([43]));
  assert.equal(borneHaute([101]), borneHaute([149]));
});

test("l'abscisse suit le temps, pas le rang du point", () => {
  // Un echantillon manquant doit laisser un TROU. Le placer au rang suivant comprimerait la
  // courbe et decalerait tout ce qui precede.
  const serie = [
    { t: T, cpu: 0, ram: 0 },
    { t: T + 30_000, cpu: 100, ram: 0 },
  ];
  const coords = points(serie, "cpu", CADRE, T, T + 60_000, 100);
  assert.deepEqual(coords[0], { x: 0, y: 50 });
  assert.deepEqual(coords[1], { x: 50, y: 0 }, "a la moitie du temps, pas a la moitie des points");
});

test("une valeur au-dela de l'echelle reste dans le cadre", () => {
  const coords = points([{ t: T, cpu: 500, ram: 0 }], "cpu", CADRE, T, T + 1000, 100);
  assert.equal(coords[0].y, 0, "elle touche le haut, elle n'en sort pas");
});

test("le trace d'une serie vide ne dessine rien", () => {
  assert.equal(ligne([]), "");
  assert.equal(aire([], CADRE), "");
});

test("un seul point se voit quand meme", () => {
  // Sinon, la premiere mesure apres l'ouverture de l'ecran n'affiche rien du tout.
  const trace = ligne(points([{ t: T, cpu: 50, ram: 0 }], "cpu", CADRE, T, T + 1000, 100));
  assert.match(trace, /^M/);
  assert.ok(trace.includes("L"), `un trait, pas un point invisible : ${trace}`);
});

test("l'aire se referme sur le bas du cadre", () => {
  const coords = points(
    [{ t: T, cpu: 50, ram: 0 }, { t: T + 1000, cpu: 100, ram: 0 }],
    "cpu", CADRE, T, T + 1000, 100,
  );
  const chemin = aire(coords, CADRE);
  assert.ok(chemin.endsWith("Z"), chemin);
  assert.ok(chemin.includes(`,${CADRE.hauteur}`), "elle descend jusqu'en bas");
});

test("l'infobulle montre le point le plus proche du curseur", () => {
  const serie = [
    { t: T, cpu: 1, ram: 0 },
    { t: T + 30_000, cpu: 2, ram: 0 },
    { t: T + 60_000, cpu: 3, ram: 0 },
  ];
  assert.equal(leplusProche(serie, 0, CADRE, T, T + 60_000).cpu, 1);
  assert.equal(leplusProche(serie, 50, CADRE, T, T + 60_000).cpu, 2);
  assert.equal(leplusProche(serie, 100, CADRE, T, T + 60_000).cpu, 3);
  assert.equal(leplusProche([], 10, CADRE, T, T + 1000), null);
});

test("les durees se lisent en un mot", () => {
  assert.equal(dureeCourte(5), "5 s");
  assert.equal(dureeCourte(300), "5 min");
  assert.equal(dureeCourte(3600), "1 h");
  assert.equal(dureeCourte(86400), "1 j");
});

test("l'heure d'un point se lit a la seconde", () => {
  assert.match(heureDe(T), /^\d{2}:\d{2}:\d{2}$/);
});

test("l'ecran ne propose pas une periode qu'il ne peut pas remplir", () => {
  // Choisir « 1 h » sur quatre minutes d'historique donnait un cadre vide et une courbe
  // collee au bord : on croit a une panne.
  assert.deepEqual(periodesOffertes(60), [300], "rien d'atteint : on offre le premier palier");
  assert.deepEqual(periodesOffertes(400), [300, 900], "le palier atteint, et celui qui vient");
  assert.deepEqual(periodesOffertes(4000), [300, 900, 3600, 10800]);
});

test("la couverture dit depuis quand l'ecran mesure", () => {
  const T2 = 1_700_000_000_000;
  assert.equal(couverture([], T2), 0);
  assert.equal(couverture([{ t: T2 - 120_000, cpu: 0, ram: 0 }], T2), 120);
});

test("l'historique enregistre se relit, total recalcule", () => {
  const h = depuisEnregistre([
    { pod: "a", t: 1000, cpu: 10, ram: 100 },
    { pod: "b", t: 1000, cpu: 5, ram: 50 },
    { pod: "a", t: 2000, cpu: 20, ram: 200 },
  ]);
  assert.equal(h.get("a").length, 2);
  assert.deepEqual(h.get(TOTAL), [
    { t: 1000, cpu: 15, ram: 150 },
    { t: 2000, cpu: 20, ram: 200 },
  ]);
});

test("la base et le direct se fusionnent sans doublon", () => {
  // Les deux peuvent porter le meme instant : un point par instant, sinon la courbe dessine
  // des dents de scie.
  const base = depuisEnregistre([{ pod: "a", t: 1000, cpu: 10, ram: 100 }]);
  const direct = new Map([["a", [{ t: 1000, cpu: 11, ram: 110 }, { t: 2000, cpu: 12, ram: 120 }]]]);
  const tout = fusionner(base, direct);
  assert.equal(tout.get("a").length, 2);
  assert.equal(tout.get("a")[0].cpu, 11, "le direct l'emporte sur le meme instant");
  assert.equal(tout.get("a")[1].t, 2000);
});

test("fusionner garde ce que la base avait et que le direct n'a pas", () => {
  const base = depuisEnregistre([{ pod: "vieux", t: 500, cpu: 1, ram: 1 }]);
  const tout = fusionner(base, new Map([["neuf", [{ t: 900, cpu: 2, ram: 2 }]]]));
  assert.equal(tout.get("vieux").length, 1);
  assert.equal(tout.get("neuf").length, 1);
});
