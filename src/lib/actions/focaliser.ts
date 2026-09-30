/**
 * Donne le focus a l'element des qu'il apparait.
 *
 * **`autofocus` NE SUFFIT PAS POUR UN CHAMP QUI APPARAIT APRES UN CLIC** : le bouton qui l'a
 * ouvert garde le focus, et Entree le reactionne au lieu de valider le champ. Vu le 2026-09-30
 * sur la creation d'un plugin : taper un nom puis Entree refermait le formulaire. Le focus est
 * donc pose au tour suivant, une fois le clic termine.
 */
export function focaliser(node: HTMLElement) {
  const minuteur = setTimeout(() => node.focus(), 0);
  return { destroy: () => clearTimeout(minuteur) };
}
