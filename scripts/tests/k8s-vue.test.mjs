/**
 * Essais de l'ecran des pods (src/lib/k8s/vue.ts).
 *
 * Ce qu'ils gardent : un pod qui DISPARAIT de la liste, et un ecran ou ce qui va mal se
 * retrouve noye sous 258 travaux termines. Les deux ont deja ete payes ailleurs dans ce
 * projet — le filtre `alive` qui vidait les onglets de terminal, et la liste a plat d'en face.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  filtrer, grouper, compter, formaterCpu, formaterRam, age, grouperLesNamespaces, appliquer, appliquerLesMesures,
} from "../../src/lib/k8s/vue.ts";

const T = Date.parse("2026-09-17T12:00:00Z");

function pod(partiel) {
  return {
    nom: "web-5cb5677dcc-8ms7z", groupe: "web", sorte: "Deployment", version: "v2-ec7a2209",
    etat: "Running", ennuyeux: false, prets: 1, conteneurs: 1, redemarrages: 0,
    depuis: "2026-09-15T12:00:00Z", machine: "machine-12", noms_conteneurs: ["web"],
    cpu: 40, ram: 429_916_160, ...partiel,
  };
}

test("la recherche accepte plusieurs mots, dans n'importe quel ordre", () => {
  const pods = [
    pod({ nom: "web-1" }),
    pod({ nom: "auth-1", groupe: "auth", etat: "CrashLoopBackOff", ennuyeux: true }),
  ];
  assert.equal(filtrer(pods, "auth crash").length, 1);
  assert.equal(filtrer(pods, "crash auth").length, 1, "l'ordre des mots ne compte pas");
  assert.equal(filtrer(pods, "").length, 2, "sans recherche, tout reste");
});

test("la recherche trouve par groupe, par version et par machine", () => {
  const pods = [pod({ nom: "abc-xyz" })];
  assert.equal(filtrer(pods, "web").length, 1, "le nom du service");
  assert.equal(filtrer(pods, "ec7a2209").length, 1, "la version livree");
  assert.equal(filtrer(pods, "machine-12").length, 1, "la machine qui l'heberge");
  assert.equal(filtrer(pods, "introuvable").length, 0);
});

test("un groupe rassemble les pods d'un meme service et cumule leurs mesures", () => {
  const groupes = grouper([
    pod({ nom: "web-a", cpu: 40, ram: 100 }),
    pod({ nom: "web-b", cpu: 38, ram: 200 }),
  ]);
  assert.equal(groupes.length, 1);
  assert.equal(groupes[0].pods.length, 2);
  assert.equal(groupes[0].prets, 2);
  assert.equal(groupes[0].attendus, 2);
  assert.equal(groupes[0].cpu, 78);
  assert.equal(groupes[0].ram, 300);
});

test("une livraison en cours se voit : deux versions dans le meme groupe", () => {
  const groupes = grouper([
    pod({ nom: "web-a", version: "v2-ec7a2209" }),
    pod({ nom: "web-b", version: "v1-9f31c07a" }),
  ]);
  assert.deepEqual(groupes[0].versions, ["v1-9f31c07a", "v2-ec7a2209"]);
});

test("ce qui va mal est en haut, ce qui est fini en bas, et RIEN n'a disparu", () => {
  const pods = [
    pod({ nom: "job-1", groupe: "nettoyage", sorte: "CronJob", etat: "Succeeded", prets: 0 }),
    pod({ nom: "web-1" }),
    pod({ nom: "auth-1", groupe: "auth", etat: "CrashLoopBackOff", ennuyeux: true }),
  ];
  const groupes = grouper(pods);
  assert.deepEqual(groupes.map((g) => g.nom), ["auth", "web", "nettoyage"]);
  const total = groupes.reduce((n, g) => n + g.pods.length, 0);
  assert.equal(total, pods.length, "un tri ne perd jamais une ligne");
});

test("un travail termine n'est pas un probleme", () => {
  const groupes = grouper([
    pod({ nom: "job-1", groupe: "net", sorte: "CronJob", etat: "Succeeded", prets: 0 }),
  ]);
  assert.equal(groupes[0].termine, true);
  assert.equal(groupes[0].ennuyeux, false, "258 travaux finis ne peignent pas l'ecran en rouge");
});

test("deux sortes de meme nom ne se melangent pas", () => {
  // Un Deployment et un CronJob peuvent porter le meme nom : les confondre afficherait
  // un groupe dont la moitie des pods n'a rien a voir avec l'autre.
  const groupes = grouper([
    pod({ nom: "a", groupe: "sauvegarde", sorte: "Deployment" }),
    pod({ nom: "b", groupe: "sauvegarde", sorte: "CronJob", etat: "Succeeded" }),
  ]);
  assert.equal(groupes.length, 2);
});

test("les comptes du haut d'ecran separent ce qui marche, ce qui alerte et ce qui est fini", () => {
  const c = compter([
    pod({}),
    pod({ etat: "CrashLoopBackOff", ennuyeux: true }),
    pod({ etat: "Succeeded" }),
    pod({ etat: "Succeeded" }),
  ]);
  assert.deepEqual(c, { enMarche: 1, ennuyeux: 1, termines: 2 });
});

test("un pod en marche mais en echec ne compte pas comme en marche", () => {
  // Le piege du cluster : la phase dit Running pendant un CrashLoopBackOff.
  const c = compter([pod({ etat: "Running", ennuyeux: true })]);
  assert.equal(c.enMarche, 0);
  assert.equal(c.ennuyeux, 1);
});

test("les mesures absentes ne s'affichent pas comme des zeros", () => {
  const groupes = grouper([pod({ cpu: null, ram: null })]);
  assert.equal(groupes[0].cpu, null);
  assert.equal(formaterCpu(null), "");
  assert.equal(formaterRam(null), "");
});

test("un groupe garde la mesure de ceux qui en ont", () => {
  const groupes = grouper([pod({ nom: "a", cpu: 40 }), pod({ nom: "b", cpu: null })]);
  assert.equal(groupes[0].cpu, 40, "une mesure connue vaut mieux que rien");
});

test("le CPU et la RAM se lisent d'un coup d'oeil", () => {
  assert.equal(formaterCpu(0), "0m");
  assert.equal(formaterCpu(40), "40m");
  assert.equal(formaterCpu(1200), "1.2");
  assert.equal(formaterRam(0), "0 o");
  assert.equal(formaterRam(429_916_160), "410 Mo");
  assert.equal(formaterRam(1_288_490_189), "1.2 Go");
});

test("l'age se calcule au moment ou on l'affiche", () => {
  assert.equal(age("2026-09-17T11:59:30Z", T), "30 s");
  assert.equal(age("2026-09-17T11:30:00Z", T), "30 min");
  assert.equal(age("2026-09-17T02:00:00Z", T), "10 h");
  assert.equal(age("2026-09-15T12:00:00Z", T), "2 j");
  assert.equal(age(null, T), "", "un pod qui n'a pas demarre n'a pas d'age");
  assert.equal(age("pas une date", T), "");
});

test("le selecteur de namespaces se range par famille au-dela d'une poignee", () => {
  const noms = [
    "core-auth-master", "core-api-master", "ccmcms-linternaute-master",
    "ccmcms-journaldunet-master", "cadremploi-master", "gitlab-agent",
    "core-lists-master", "ccmcms-figaro-master", "un-seul", "autre-seul",
    "core-dca-master", "ccmcms-hugo-master",
  ];
  const familles = grouperLesNamespaces(noms);
  assert.deepEqual(familles.map((f) => f.famille), ["ccmcms", "core", ""]);
  assert.equal(familles.at(-1).noms.length, 4, "les isoles finissent ensemble, pas en titres");
  const total = familles.reduce((n, f) => n + f.noms.length, 0);
  assert.equal(total, noms.length, "aucun namespace ne se perd au classement");
});

test("une poignee de namespaces reste a plat", () => {
  const familles = grouperLesNamespaces(["prod", "qlf", "dev"]);
  assert.equal(familles.length, 1);
  assert.deepEqual(familles[0].noms, ["dev", "prod", "qlf"], "et triee");
});

test("un changement du flux ne fait pas clignoter le CPU et la RAM", () => {
  // Le flux envoie l'etat du pod, jamais ses mesures : les ecraser viderait deux colonnes
  // pendant quinze secondes a chaque changement d'etat.
  const avant = [pod({ nom: "web-1", cpu: 40, ram: 100 })];
  const venuDuFlux = pod({ nom: "web-1", etat: "CrashLoopBackOff", ennuyeux: true, cpu: null, ram: null });
  const apres = appliquer(avant, "MODIFIED", venuDuFlux);
  assert.equal(apres[0].etat, "CrashLoopBackOff", "le nouvel etat prend");
  assert.equal(apres[0].cpu, 40, "et la mesure connue reste");
  assert.equal(apres[0].ram, 100);
});

test("le flux ajoute, remplace et retire, sans jamais toucher aux autres", () => {
  const avant = [pod({ nom: "a" }), pod({ nom: "b" })];
  assert.equal(appliquer(avant, "ADDED", pod({ nom: "c" })).length, 3);
  assert.equal(appliquer(avant, "DELETED", pod({ nom: "a" })).length, 1);
  assert.deepEqual(appliquer(avant, "DELETED", pod({ nom: "a" })).map((p) => p.nom), ["b"]);
  const remplace = appliquer(avant, "MODIFIED", pod({ nom: "b", machine: "machine-99" }));
  assert.equal(remplace.length, 2);
  assert.equal(remplace.find((p) => p.nom === "b").machine, "machine-99");
  assert.equal(remplace.find((p) => p.nom === "a").machine, "machine-12", "l'autre est intact");
});

test("les mesures se posent sur les pods connus et laissent les autres tranquilles", () => {
  const pods = [pod({ nom: "a", cpu: null, ram: null }), pod({ nom: "b", cpu: null, ram: null })];
  const apres = appliquerLesMesures(pods, [{ nom: "a", cpu: 40, ram: 100 }]);
  assert.equal(apres[0].cpu, 40);
  assert.equal(apres[1].cpu, null, "un pod sans mesure n'affiche pas zero");
});
