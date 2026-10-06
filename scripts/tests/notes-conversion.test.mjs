/**
 * L'aller-retour d'une note : HTML de l'editeur -> Markdown enregistre -> HTML relu.
 * Signale le 2026-10-01 : une ligne laissee vide entre deux phrases disparaissait.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { marked } from "marked";
import domino from "@mixmark-io/domino";
import { creerTurndown, versMarkdown } from "../../src/lib/notes/conversion.ts";

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

const doc = domino.createDocument("");
const enMarkdown = (html) => versMarkdown(turndown, html, doc);
/** Combien de lignes vides le HTML relu affiche entre « a » et « b ». */
const videsEntre = (md) => {
  const relu = marked.parse(md);
  const entre = relu.slice(relu.indexOf("AAA"), relu.indexOf("BBB"));
  return (entre.match(/<p><br><\/p>/g) ?? []).length;
};

test("Entree qui pose des <br> dans le paragraphe garde aussi ses lignes vides", () => {
  // La forme exacte trouvee dans la base de l'utilisateur le 2026-10-01.
  assert.equal(videsEntre(enMarkdown("<p>AAA<br><br></p><p>BBB</p>")), 1);
  assert.equal(videsEntre(enMarkdown("<p>AAA<br><br>BBB</p>")), 1);
  assert.equal(videsEntre(enMarkdown("<p>AAA<br><br><br>BBB</p>")), 2);
  assert.equal(videsEntre(enMarkdown("<div>AAA</div><div><br></div><div>BBB</div>")), 1);
});

test("un retour a la ligne simple reste un retour a la ligne, sans ligne vide", () => {
  const md = enMarkdown("<p>AAA<br>BBB</p>");
  assert.equal(videsEntre(md), 0);
  assert.match(marked.parse(md), /AAA<br>BBB/);
  assert.equal(videsEntre(enMarkdown("<p>AAA<br></p><p>BBB</p>")), 0, "le dernier <br> d'un paragraphe ne se voit pas");
});

test("une note deja propre ressort identique, enregistrement apres enregistrement", () => {
  const une = enMarkdown("<p>AAA<br><br></p><p>BBB</p>");
  assert.equal(enMarkdown(marked.parse(une)), une);
});
