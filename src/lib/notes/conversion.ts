/**
 * Du HTML de l'editeur de notes au Markdown enregistre.
 *
 * A part du composant pour s'eprouver sous node : turndown y apporte son propre analyseur.
 */
import TurndownService from "turndown";

/// Texte d'un bloc, les `<br>` comptes comme de vrais sauts de ligne.
export function texteDeBloc(node: Node): string {
  let texte = "";
  for (const enfant of Array.from(node.childNodes)) {
    if (enfant.nodeType === 3) texte += enfant.nodeValue ?? "";
    else if (enfant.nodeName === "BR") texte += "\n";
    else texte += texteDeBloc(enfant);
  }
  return texte;
}

/// Tout `<pre>` redevient un bloc de code Markdown, avec ou sans enfant `<code>`.
///
/// La regle d'origine de turndown lit `node.firstChild.textContent` : elle ignore donc un
/// `<pre>` NU (celui que posait le bouton) et perd les `<br>` que WebKit intercale quand on
/// met plusieurs lignes en bloc de code. Dans les deux cas le bloc repartait en simple
/// paragraphe a la sauvegarde — du code perdu en silence.
export function creerTurndown(): TurndownService {
  const turndown = new TurndownService({ headingStyle: "atx", codeBlockStyle: "fenced" });
  turndown.addRule("blocDeCode", {
    filter: "pre",
    replacement: (_contenu, node) => {
      const texte = texteDeBloc(node).replace(/\n+$/, "");
      const langue = (node.querySelector("code")?.className.match(/language-(\S+)/) ?? ["", ""])[1];
      // La cloture doit etre plus longue que la plus longue suite d'accents graves du contenu.
      const plusLongue = (texte.match(/`+/g) ?? []).reduce((max, suite) => Math.max(max, suite.length), 0);
      const cloture = "`".repeat(Math.max(3, plusLongue + 1));
      return `\n\n${cloture}${langue}\n${texte}\n${cloture}\n\n`;
    },
  });

  /// **UNE LIGNE VIDE RESTE UNE LIGNE VIDE.** Turndown reduisait un paragraphe vide a des
  /// espaces, que le Markdown ignore : deux paragraphes separes par une ligne laissee vide
  /// revenaient colles a la relecture (signale le 2026-10-01). Le paragraphe vide s'ecrit donc
  /// tel quel en HTML, que `marked` rend sans y toucher : l'aller-retour est stable.
  turndown.addRule("ligneVide", {
    filter: (node) =>
      (node.nodeName === "P" || node.nodeName === "DIV") &&
      (node.textContent ?? "").trim() === "" &&
      !node.querySelector("img"),
    replacement: () => "\n\n<p><br></p>\n\n",
  });
  return turndown;
}
