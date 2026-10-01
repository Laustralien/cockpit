/**
 * L'aller-retour d'une note : HTML de l'editeur -> Markdown enregistre -> HTML relu.
 * Signale le 2026-10-01 : une ligne laissee vide entre deux phrases disparaissait.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { marked } from "marked";
import { creerTurndown } from "../../src/lib/notes/conversion.ts";

const turndown = creerTurndown();
const allerRetour = (html) => marked.parse(turndown.turndown(html));

test("une ligne vide entre deux paragraphes survit a l'enregistrement", () => {
  const relu = allerRetour("<div>premiere</div><div><br></div><div>seconde</div>");
  assert.match(relu, /<p>premiere<\/p>\s*<p><br><\/p>\s*<p>seconde<\/p>/);
});

test("l'aller-retour est stable : rien ne s'ajoute a chaque enregistrement", () => {
  const une = turndown.turndown("<p>a</p><p><br></p><p><br></p><p>b</p>");
  const deux = turndown.turndown(marked.parse(une));
  assert.equal(deux, une);
  assert.equal((une.match(/<p><br><\/p>/g) ?? []).length, 2, "deux lignes vides, deux lignes vides");
});

test("un paragraphe qui porte une image n'est pas pris pour une ligne vide", () => {
  assert.match(turndown.turndown('<p><img src="a.png" alt="x"></p>'), /!\[x\]\(a\.png\)/);
});
