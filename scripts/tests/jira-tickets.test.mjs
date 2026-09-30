/**
 * Essais du rangement des tickets Jira (src/lib/jira/tickets.ts).
 *
 * Ce qu'ils gardent : un ticket rattache au mauvais projet Cockpit (et donc demarre dans le
 * mauvais depot), et un projet sans cle liee qui afficherait les tickets de tous les autres.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { grouperParCategorie, liaisonDuTicket, clesDuFiltre } from "../../src/lib/jira/tickets.ts";

const ticket = (cle, categorie_statut) => ({
  cle, resume: "", description: "", statut: "", categorie_statut, type_ticket: "Bug",
  priorite: "", projet: cle.split("-")[0], maj_le: "", url: "",
});
const liaisons = [
  { projet: "site", cles: ["PROJ", "ABC"], gabarit: "{type}/{cle}/{slug}" },
  { projet: "outil", cles: ["OUT"], gabarit: "{type}/{cle}/{slug}" },
];

test("les tickets sont ranges en cours puis a faire, les termines ecartes", () => {
  const g = grouperParCategorie([ticket("PROJ-1", "new"), ticket("PROJ-2", "indeterminate"), ticket("PROJ-3", "done")]);
  assert.deepEqual(g.enCours.map((t) => t.cle), ["PROJ-2"]);
  assert.deepEqual(g.aFaire.map((t) => t.cle), ["PROJ-1"]);
});

test("un ticket est rattache au projet qui porte sa cle", () => {
  assert.equal(liaisonDuTicket(ticket("ABC-9", "new"), liaisons)?.projet, "site");
  assert.equal(liaisonDuTicket(ticket("OUT-1", "new"), liaisons)?.projet, "outil");
  assert.equal(liaisonDuTicket(ticket("ZZZ-1", "new"), liaisons), null);
});

test("sans filtre on demande tout, un projet sans cle ne demande rien", () => {
  assert.equal(clesDuFiltre("", liaisons), null);
  assert.deepEqual(clesDuFiltre("site", liaisons), ["PROJ", "ABC"]);
  assert.deepEqual(clesDuFiltre("inconnu", liaisons), []);
});
