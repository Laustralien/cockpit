/**
 * Essais de la fusion « ce qui est declare » et « ce qui tourne » (src/lib/k8s/vue.ts).
 *
 * Ce qu'ils gardent : un objet declare qui n'a aucun pod et disparait de l'ecran — signale sur
 * un vrai namespace, ou dix-sept travaux planifies sur soixante-dix-neuf n'avaient jamais ete
 * declenches — et des compteurs qui annoncent des objets en comptant des pods.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  unifier, deSorte, filtrerLesElements, ensemble, estUnService, grouper,
} from "../../src/lib/k8s/vue.ts";

function pod(partiel) {
  return {
    nom: "web-5cb5677dcc-8ms7z", groupe: "web", sorte: "Deployment", version: "v2",
    etat: "Running", ennuyeux: false, prets: 1, conteneurs: 1, redemarrages: 0,
    depuis: "2026-09-15T12:00:00Z", machine: "m1", noms_conteneurs: ["web"],
    cpu: 40, ram: 100, ...partiel,
  };
}
function declare(partiel) {
  return {
    nom: "web", sorte: "Deployment", version: "v2", voulus: 1, prets: 1,
    suspendu: false, planification: "", dernier: null, depuis: null, ...partiel,
  };
}

test("un travail planifie jamais declenche apparait quand meme", () => {
  // Le cas signale : aucun pod, donc invisible dans une liste de pods.
  const elements = unifier(
    [declare({ nom: "game-daily-snapshot", sorte: "CronJob", planification: "10 3 * * *" })],
    [],
  );
  assert.equal(elements.length, 1);
  assert.equal(elements[0].nom, "game-daily-snapshot");
  assert.equal(elements[0].pods.length, 0);
  assert.equal(elements[0].dernier, null, "« jamais » se lit dans l'absence de dernier");
  assert.equal(elements[0].declare, true);
});

test("un objet declare recupere ses pods", () => {
  const groupes = grouper([pod({ nom: "web-1" }), pod({ nom: "web-2" })]);
  const elements = unifier([declare({ voulus: 2, prets: 2 })], groupes);
  assert.equal(elements.length, 1, "un seul element, pas un doublon");
  assert.equal(elements[0].pods.length, 2);
  assert.equal(elements[0].cpu, 80);
});

test("des pods sans objet declare restent visibles", () => {
  // Leur createur a disparu : ils existent pourtant, et les cacher ferait chercher ailleurs.
  const groupes = grouper([pod({ nom: "orphelin-1", groupe: "orphelin", sorte: "" })]);
  const elements = unifier([], groupes);
  assert.equal(elements.length, 1);
  assert.equal(elements[0].declare, false);
  assert.equal(elements[0].pods.length, 1);
});

test("un service dont la declaration n'est pas satisfaite reclame une action", () => {
  const elements = unifier([declare({ voulus: 3, prets: 1 })], []);
  assert.equal(elements[0].ennuyeux, true, "1 sur 3 : il manque des pods");
  const sains = unifier([declare({ voulus: 3, prets: 3 })], []);
  assert.equal(sains[0].ennuyeux, false);
});

test("un service satisfait mais dont un pod echoue reclame aussi une action", () => {
  const groupes = grouper([pod({ nom: "web-1", etat: "CrashLoopBackOff", ennuyeux: true })]);
  const elements = unifier([declare({ voulus: 1, prets: 1 })], groupes);
  assert.equal(elements[0].ennuyeux, true);
});

test("chaque vue ne montre que sa sorte", () => {
  const elements = unifier(
    [
      declare({ nom: "web" }),
      declare({ nom: "nettoyage", sorte: "CronJob" }),
      declare({ nom: "collecteur", sorte: "DaemonSet" }),
    ],
    grouper([pod({ nom: "seul-1", groupe: "seul", sorte: "" })]),
  );
  assert.deepEqual(deSorte(elements, "services").map((e) => e.nom).sort(), ["collecteur", "web"]);
  assert.deepEqual(deSorte(elements, "taches").map((e) => e.nom), ["nettoyage"]);
  assert.deepEqual(deSorte(elements, "autres").map((e) => e.nom), ["seul"]);
  assert.equal(estUnService("CronJob"), false);
});

test("la vue d'ensemble compte des OBJETS, et les pods a part", () => {
  // L'incoherence signalee : « taches planifiees 264 » comptait les pods de travaux, alors
  // que le cluster declare 79 travaux.
  const pods = [
    pod({ nom: "j1", groupe: "nettoyage", sorte: "CronJob", etat: "Succeeded" }),
    pod({ nom: "j2", groupe: "nettoyage", sorte: "CronJob", etat: "Succeeded" }),
    pod({ nom: "web-1" }),
  ];
  const elements = unifier(
    [declare({ nom: "web" }), declare({ nom: "nettoyage", sorte: "CronJob", dernier: "hier" }),
     declare({ nom: "neuve", sorte: "CronJob" }),
     declare({ nom: "dormeur", sorte: "CronJob", suspendu: true })],
    grouper(pods),
  );
  const e = ensemble(elements, pods);
  assert.equal(e.taches, 3, "trois travaux declares");
  assert.equal(e.tachesSuspendues, 1);
  assert.equal(e.tachesJamaisLancees, 1, "celui qui n'a pas de dernier declenchement");
  // Et le suspendu n'y est PAS : il est eteint expres, ce n'est pas une question ouverte.
  assert.equal(
    ensemble(
      unifier([declare({ nom: "dormeur", sorte: "CronJob", suspendu: true })], []),
      [],
    ).tachesJamaisLancees,
    0,
  );
  assert.equal(e.services, 1);
  assert.equal(e.pods, 3, "et les pods se comptent a part");
  assert.equal(e.podsTermines, 2);
});

test("la recherche trouve un objet par son nom, meme sans pod", () => {
  const elements = unifier(
    [declare({ nom: "game-daily-snapshot-ccmfr", sorte: "CronJob" }), declare({ nom: "web" })],
    [],
  );
  assert.equal(filtrerLesElements(elements, "snap").length, 1);
  assert.equal(filtrerLesElements(elements, "snap").at(0).nom, "game-daily-snapshot-ccmfr");
});

test("la recherche trouve aussi par le nom d'un pod", () => {
  const groupes = grouper([pod({ nom: "web-5cb5677dcc-8ms7z" })]);
  const elements = unifier([declare()], groupes);
  assert.equal(filtrerLesElements(elements, "8ms7z").length, 1);
  assert.equal(filtrerLesElements(elements, "introuvable").length, 0);
});

test("ce qui reclame une action passe devant", () => {
  const elements = unifier(
    [declare({ nom: "zzz", voulus: 2, prets: 0 }), declare({ nom: "aaa" })],
    [],
  );
  assert.deepEqual(elements.map((e) => e.nom), ["zzz", "aaa"]);
});
