/**
 * Les regles de la vue des logs : suivre le bas, et chercher dedans.
 *
 * Module PUR : aucune dependance au DOM. Ce qui touche a l'element est dans le composant, mais
 * les DECISIONS sont ici, ou elles s'eprouvent.
 */

/** Une ligne decoupee pour l'affichage : les morceaux qui correspondent sont marques. */
export interface Morceau {
  texte: string;
  trouve: boolean;
}

/**
 * A quelle distance du bas on considere qu'on « est en bas ».
 *
 * Quelques pixels de tolerance : un defilement fluide s'arrete rarement au pixel pres, et une
 * exigence stricte ferait decrocher le suivi tout seul.
 */
export const MARGE_DU_BAS = 24;

/**
 * Faut-il continuer a coller au bas des logs ?
 *
 * **CE QUI DECIDE, C'EST OU L'UTILISATEUR A LAISSE LA VUE.** Ramener en bas quelqu'un qui vient
 * de remonter pour lire est la faute classique des consoles de logs : on perd sa ligne a chaque
 * arrivee, et on ne peut plus rien lire pendant que ca ecrit. Tant qu'on est en bas, on suit ;
 * des qu'on remonte, on ne bouge plus ; et on reprend le suivi quand on revient en bas.
 */
export function doitSuivre(
  defilement: number,
  hauteurTotale: number,
  hauteurVisible: number,
  marge: number = MARGE_DU_BAS,
): boolean {
  return hauteurTotale - (defilement + hauteurVisible) <= marge;
}

/**
 * Decoupe une ligne selon la recherche, pour surligner ce qui correspond.
 *
 * La recherche est insensible a la casse : personne ne tape `ERROR` avec la bonne casse quand
 * il cherche une erreur.
 */
export function decouper(ligne: string, recherche: string): Morceau[] {
  const quoi = recherche.trim();
  if (!quoi) return [{ texte: ligne, trouve: false }];
  const bas = ligne.toLowerCase();
  const cible = quoi.toLowerCase();
  const morceaux: Morceau[] = [];
  let depuis = 0;
  for (;;) {
    const trouve = bas.indexOf(cible, depuis);
    if (trouve === -1) break;
    if (trouve > depuis) morceaux.push({ texte: ligne.slice(depuis, trouve), trouve: false });
    morceaux.push({ texte: ligne.slice(trouve, trouve + cible.length), trouve: true });
    depuis = trouve + cible.length;
  }
  if (depuis < ligne.length) morceaux.push({ texte: ligne.slice(depuis), trouve: false });
  return morceaux;
}

/** Combien de lignes contiennent la recherche. Sert au compteur a cote du champ. */
export function compterLesLignes(lignes: string[], recherche: string): number {
  const quoi = recherche.trim().toLowerCase();
  if (!quoi) return 0;
  return lignes.filter((l) => l.toLowerCase().includes(quoi)).length;
}

/**
 * Ajoute ce qui arrive du flux, en bornant ce qu'on garde en memoire.
 *
 * **UN POD BAVARD REMPLIRAIT LA PAGE JUSQU'A LA FAIRE TOMBER.** Certains ecrivent des milliers
 * de lignes par minute ; les garder toutes ferait grossir l'interface sans fin, pour un
 * historique que personne ne remonte. On garde les dernieres, et on le dit a l'ecran.
 */
export const LIGNES_GARDEES = 5000;

export function ajouter(lignes: string[], arrivee: string, maximum = LIGNES_GARDEES): string[] {
  const neuves = arrivee.split("\n").filter((l) => l.length > 0);
  if (neuves.length === 0) return lignes;
  const total = [...lignes, ...neuves];
  return total.length > maximum ? total.slice(total.length - maximum) : total;
}

/**
 * Separe l'horodatage du message.
 *
 * Le cluster prefixe chaque ligne d'une date ISO quand on la demande. L'afficher en gris a part
 * rend la ligne lisible ; la laisser collee au message donne un mur de chiffres.
 */
export function separerLHeure(ligne: string): { heure: string; texte: string } {
  const espace = ligne.indexOf(" ");
  if (espace === -1) return { heure: "", texte: ligne };
  const debut = ligne.slice(0, espace);
  // Une date ISO, telle que Kubernetes la pose : 2026-09-17T15:00:45.406134289Z
  if (!/^\d{4}-\d{2}-\d{2}T[\d:.]+(Z|[+-]\d{2}:?\d{2})$/.test(debut)) {
    return { heure: "", texte: ligne };
  }
  const heure = debut.slice(11, 19);
  return { heure, texte: ligne.slice(espace + 1) };
}
