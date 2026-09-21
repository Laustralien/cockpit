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

test("la periode choisie figure toujours parmi celles proposees", () => {
  // Un `select` dont la valeur ne correspond a aucune option s'affiche vide : c'est ce qui
  // arrivait avec une periode retenue de 15 min sur vingt secondes de mesure.
  const offertes = periodesOffertes(20);
  assert.ok(!offertes.includes(900), "900 n'est pas encore atteignable");
  const avecLaChoisie = [...new Set([...offertes, 900])].sort((a, b) => a - b);
  assert.ok(avecLaChoisie.includes(900), "mais elle doit rester dans la liste affichee");
});

// ── Les heures sous la courbe ────────────────────────────────────────────────────────────────

test("les reperes tombent sur des instants ronds", async () => {
  const { graduationsDeTemps } = await import("../../src/lib/k8s/mesures.ts");
  // Une fenetre d'une heure qui commence a 14:07:23 : on ne veut pas lire « 14:07 », « 14:16 »,
  // « 14:25 ». Le pas choisi doit diviser l'heure, et chaque repere tomber dessus.
  const debut = new Date(2026, 8, 18, 14, 7, 23).getTime();
  const reperes = graduationsDeTemps(debut, debut + 3_600_000, 640);
  assert.ok(reperes.length >= 3, `trop peu de reperes : ${reperes.length}`);
  const pasMinutes = (reperes[1].t - reperes[0].t) / 60_000;
  assert.equal(60 % pasMinutes, 0, `le pas (${pasMinutes} min) ne divise pas l'heure`);
  for (const r of reperes) {
    const d = new Date(r.t);
    assert.equal(d.getSeconds(), 0, `${r.libelle} ne tombe pas sur une minute ronde`);
    assert.equal(d.getMinutes() % pasMinutes, 0, `${r.libelle} ne tombe pas sur le pas`);
  }
  assert.match(reperes[0].libelle, /^\d{2}:\d{2}$/, "pas de secondes toujours nulles sur une heure");
});

test("le pas s'adapte a la largeur, jamais de libelles qui se touchent", async () => {
  const { graduationsDeTemps } = await import("../../src/lib/k8s/mesures.ts");
  const debut = new Date(2026, 8, 18, 14, 0, 0).getTime();
  const large = graduationsDeTemps(debut, debut + 3_600_000, 1200);
  const etroit = graduationsDeTemps(debut, debut + 3_600_000, 240);
  assert.ok(large.length > etroit.length, "un cadre large porte plus de reperes");
  for (const liste of [large, etroit]) {
    for (let i = 1; i < liste.length; i++) {
      assert.ok(liste[i].x - liste[i - 1].x >= 80, `reperes trop serres : ${liste[i].libelle}`);
    }
  }
});

test("une fenetre courte montre les secondes", async () => {
  const { graduationsDeTemps } = await import("../../src/lib/k8s/mesures.ts");
  const debut = new Date(2026, 8, 18, 14, 0, 0).getTime();
  const reperes = graduationsDeTemps(debut, debut + 120_000, 900);
  assert.match(reperes[0].libelle, /^\d{2}:\d{2}:\d{2}$/);
});

test("chaque repere est dans le cadre, a sa place", async () => {
  const { graduationsDeTemps } = await import("../../src/lib/k8s/mesures.ts");
  const debut = new Date(2026, 8, 18, 14, 0, 0).getTime();
  const largeur = 600;
  const reperes = graduationsDeTemps(debut, debut + 3_600_000, largeur);
  for (const r of reperes) {
    assert.ok(r.x >= 0 && r.x <= largeur, `${r.libelle} sort du cadre : ${r.x}`);
  }
  // Le repere de 14:30 est a la moitie du cadre, parce que l'abscisse suit le TEMPS.
  const demi = reperes.find((r) => r.libelle.endsWith(":30"));
  assert.ok(demi);
  assert.equal(Math.round(demi.x), largeur / 2);
});

test("un cadre pas encore mesure ne rend rien", async () => {
  const { graduationsDeTemps } = await import("../../src/lib/k8s/mesures.ts");
  // Au premier rendu, la largeur vaut zero : sans cette garde, la boucle tourne sans fin.
  assert.deepEqual(graduationsDeTemps(1000, 2000, 0), []);
  assert.deepEqual(graduationsDeTemps(2000, 2000, 500), []);
});

// ── Les bandes empilees ──────────────────────────────────────────────────────────────────────

const { empiler, sommets, bandeSousLeCurseur, couleurDeSerie, BANDES_MAX } = await import(
  "../../src/lib/k8s/mesures.ts"
);

/** Fabrique un historique : { pod: [[t, cpu], …] }. */
function historique(table) {
  const h = new Map();
  for (const [nom, points] of Object.entries(table)) {
    h.set(nom, points.map(([t, cpu]) => ({ t, cpu, ram: cpu * 10 })));
  }
  return h;
}

test("les bandes s'empilent, et leur sommet est le total", () => {
  const h = historique({ a: [[1000, 10], [2000, 20]], b: [[1000, 5], [2000, 5]] });
  const bandes = empiler(h, "cpu", 0);
  assert.equal(bandes.length, 2);
  assert.deepEqual(bandes[0].points[0], { t: 1000, bas: 0, haut: 10 });
  assert.deepEqual(bandes[1].points[0], { t: 1000, bas: 10, haut: 15 });
  assert.deepEqual(sommets(bandes), [15, 25], "le haut de la pile, c'est la courbe du total");
});

test("un pod non mesure a cet instant vaut zero, jamais sa derniere valeur", () => {
  // Prolonger inventerait de la consommation pour un pod qui n'existait plus.
  const h = historique({ a: [[1000, 10]], b: [[1000, 5], [2000, 5]] });
  const bandes = empiler(h, "cpu", 0);
  const aDeux = bandes.find((x) => x.nom === "a").points.find((p) => p.t === 2000);
  assert.equal(aDeux.haut - aDeux.bas, 0);
  assert.deepEqual(sommets(bandes), [15, 5]);
});

test("tous les pods mesures ont leur bande, sans regroupement", () => {
  // Demande du mainteneur : « je veux pas de autres, je veux tous les voir ». Vingt-trois pods,
  // c'est son namespace reel.
  const table = {};
  for (let i = 0; i < 23; i++) table[`pod-${String(i).padStart(2, "0")}`] = [[1000, 10]];
  const bandes = empiler(historique(table), "cpu", 0);
  assert.equal(bandes.length, 23);
  assert.ok(!bandes.some((b) => b.teinte < 0), "aucune bande « autres »");
  assert.equal(sommets(bandes)[0], 230);
});

test("chaque rang a sa couleur, et deux voisins ne se ressemblent pas", () => {
  const teinte = (c) => Number(c.match(/hsl\(([\d.]+)/)[1]);
  const vues = new Set();
  for (let i = 0; i < 40; i++) {
    const c = couleurDeSerie(i);
    assert.match(c, /^hsl\([\d.]+ \d+% \d+%\)$/, c);
    assert.equal(c, couleurDeSerie(i), "la meme entree donne toujours la meme couleur");
    vues.add(c);
    if (i > 0) {
      const ecart = Math.abs(teinte(c) - teinte(couleurDeSerie(i - 1)));
      const tour = Math.min(ecart, 360 - ecart);
      assert.ok(tour > 40, `rangs ${i - 1} et ${i} trop proches : ${tour.toFixed(0)}°`);
    }
  }
  assert.equal(vues.size, 40, "quarante rangs, quarante couleurs differentes");
  assert.equal(couleurDeSerie(-1), "var(--serie-autres)", "le regroupement garde son gris");
});

test("au-dela du plafond, le reste va dans « autres » et n'est jamais perdu", () => {
  const table = {};
  for (let i = 0; i < BANDES_MAX + 4; i++) table[`pod-${String(i).padStart(2, "0")}`] = [[1000, 10]];
  const bandes = empiler(historique(table), "cpu", 0);
  assert.equal(bandes.length, BANDES_MAX + 1, "les nommes, plus une bande pour le reste");
  const derniere = bandes[bandes.length - 1];
  assert.equal(derniere.teinte, -1);
  assert.equal(derniere.points[0].haut - derniere.points[0].bas, 40, "les 4 restants, cumules");
  assert.equal(sommets(bandes)[0], (BANDES_MAX + 4) * 10, "et le total reste juste");
});

test("ce sont les plus gros de la FENETRE qui sont nommes", () => {
  // Une pointe passagere ne doit pas evincer un pod qui consomme en continu.
  const table = { continu: [[1000, 5], [2000, 5], [3000, 5]] };
  for (let i = 0; i < BANDES_MAX; i++) table[`petit-${i}`] = [[1000, 1], [2000, 1], [3000, 1]];
  const bandes = empiler(historique(table), "cpu", 0);
  assert.ok(bandes.some((b) => b.nom === "continu"), "le regulier est nomme");
});

test("les couleurs suivent le nom, pas la consommation", () => {
  // Sinon deux mesures voisines echangeraient les teintes et le graphique clignoterait.
  const h1 = historique({ zzz: [[1000, 100]], aaa: [[1000, 1]] });
  const h2 = historique({ zzz: [[1000, 1]], aaa: [[1000, 100]] });
  const t1 = Object.fromEntries(empiler(h1, "cpu", 0).map((b) => [b.nom, b.teinte]));
  const t2 = Object.fromEntries(empiler(h2, "cpu", 0).map((b) => [b.nom, b.teinte]));
  assert.deepEqual(t1, t2);
  assert.equal(t1.aaa, 0, "le premier nom prend la premiere teinte");
});

test("le total ne se compte pas comme un pod", () => {
  const h = historique({ a: [[1000, 10]] });
  h.set(TOTAL, [{ t: 1000, cpu: 10, ram: 100 }]);
  const bandes = empiler(h, "cpu", 0);
  assert.deepEqual(bandes.map((b) => b.nom), ["a"], "sinon tout serait compte deux fois");
});

test("la fenetre ecarte ce qui est trop vieux", () => {
  const h = historique({ a: [[1000, 10], [9000, 4]] });
  assert.deepEqual(sommets(empiler(h, "cpu", 5000)), [4]);
  assert.deepEqual(empiler(h, "cpu", 99999), [], "rien dans la fenetre : rien a dessiner");
});

test("l'infobulle designe la bande sous le curseur", () => {
  const h = historique({ bas: [[1000, 10]], haut: [[1000, 10]] });
  const bandes = empiler(h, "cpu", 0);
  const cadre = { largeur: 100, hauteur: 100 };
  // max = 20 : la moitie basse du cadre appartient a « bas », la haute a « haut ».
  const enBas = bandeSousLeCurseur(bandes, 50, 90, cadre, 1000, 2000, 20);
  assert.equal(enBas.bande.nom, "bas");
  assert.equal(enBas.valeur, 10);
  const enHaut = bandeSousLeCurseur(bandes, 50, 10, cadre, 1000, 2000, 20);
  assert.equal(enHaut.bande.nom, "haut");
  // Au-dessus de la pile, plus personne : on ne designe pas une bande au hasard. Avec une
  // echelle a 40, le haut du cadre vaut 40 alors que la pile s'arrete a 20.
  assert.equal(bandeSousLeCurseur(bandes, 50, 0, cadre, 1000, 2000, 40), null);
});

test("sans bande, l'infobulle ne designe rien", () => {
  assert.equal(bandeSousLeCurseur([], 10, 10, { largeur: 100, hauteur: 100 }, 0, 1000, 5), null);
});
