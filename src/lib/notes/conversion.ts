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

const estUnSaut = (n: Node) => n.nodeName === "BR";
const estDuVide = (n: Node) => n.nodeType === 3 && !(n.nodeValue ?? "").trim();

/**
 * Chaque ligne laissee vide devient un VRAI paragraphe vide, quelle que soit la facon dont le
 * navigateur l'a faite.
 *
 * **ENTREE NE PRODUIT PAS TOUJOURS LA MEME CHOSE.** Selon l'endroit, le navigateur ouvre un
 * nouveau paragraphe, ou pose un `<br>` dans le paragraphe courant. Le premier cas etait gere ;
 * le second ne l'etait pas, et le Markdown ignore des retours a la ligne en fin de paragraphe
 * ou en serie : constate le 2026-10-01 dans la base de l'utilisateur, `resultat,  \n  \n`
 * revenait sans sa ligne vide. Regle, celle de l'affichage : n retours a la ligne d'affilee
 * dans un paragraphe font n - 1 lignes vides ; le dernier d'un paragraphe ne se voit pas.
 */
export function normaliserLesLignesVides(racine: Element, doc: Document): void {
  for (const bloc of Array.from(racine.querySelectorAll("p, div"))) {
    if (bloc.closest("pre")) continue;
    const enfants = Array.from(bloc.childNodes);
    // Morceaux de contenu, separes par un nombre de lignes vides.
    const morceaux: Node[][] = [[]];
    const videsApres: number[] = [0];
    let i = 0;
    while (i < enfants.length) {
      if (!estUnSaut(enfants[i])) {
        morceaux[morceaux.length - 1].push(enfants[i]);
        i++;
        continue;
      }
      let j = i;
      let sauts = 0;
      while (j < enfants.length && (estUnSaut(enfants[j]) || estDuVide(enfants[j]))) {
        if (estUnSaut(enfants[j])) sauts++;
        j++;
      }
      const enFin = j >= enfants.length;
      if (enFin) {
        videsApres[videsApres.length - 1] += sauts - 1;
      } else if (sauts >= 2) {
        videsApres[videsApres.length - 1] += sauts - 1;
        morceaux.push([]);
        videsApres.push(0);
      } else {
        morceaux[morceaux.length - 1].push(...enfants.slice(i, j));
      }
      i = j;
    }
    if (morceaux.length === 1 && videsApres[0] === 0) continue;
    const paragrapheVide = () => {
      const p = doc.createElement("p");
      p.appendChild(doc.createElement("br"));
      return p;
    };
    // Le premier morceau reste dans le bloc ; les suivants vont dans des blocs freres.
    while (bloc.firstChild) bloc.removeChild(bloc.firstChild);
    for (const n of morceaux[0]) bloc.appendChild(n);
    let apres: Element = bloc;
    morceaux.forEach((morceau, k) => {
      if (k > 0) {
        const suite = doc.createElement(bloc.nodeName.toLowerCase());
        for (const n of morceau) suite.appendChild(n);
        apres.after(suite);
        apres = suite;
      }
      for (let v = 0; v < videsApres[k]; v++) {
        const vide = paragrapheVide();
        apres.after(vide);
        apres = vide;
      }
    });
  }
}

/// Le Markdown d'une note, depuis le HTML de son editeur.
export function versMarkdown(turndown: TurndownService, html: string, doc: Document): string {
  const conteneur = doc.createElement("div");
  conteneur.innerHTML = html;
  normaliserLesLignesVides(conteneur, doc);
  return turndown.turndown(conteneur);
}
