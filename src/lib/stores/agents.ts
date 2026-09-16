import { derived, get, writable } from "svelte/store";
import { terminals } from "./terminals";
import {
  prochainEtat,
  sortieDe,
  meriteUnRepere,
  type EtatAgent,
  type Suivi,
} from "../terminaux/activite";

/**
 * Ou en est l'agent de chaque terminal, pour la barre laterale.
 *
 * **LA LECTURE EST ECHANTILLONNEE, L'ECRITURE NE L'EST PAS.** Le chemin de la sortie ne fait
 * que poser un instant dans une table (voir `terminaux/activite`). C'est ICI qu'on en tire un
 * etat, une fois par seconde, et **seulement quand un agent tourne quelque part** : sans agent,
 * ce minuteur ne calcule rien. La regle du projet vaut aussi pour ce qu'on ajoute : tout ce qui
 * est appele en boucle se mesure, et ce qui peut ne pas tourner ne tourne pas.
 */
const PERIODE_MS = 1000;

const suivis = new Map<number, Suivi>();

/** L'etat par identifiant de terminal. Seuls ceux qui meritent un repere y figurent. */
export const etatsAgents = writable<Map<number, EtatAgent>>(new Map());

/** Combien de terminaux reclament quelque chose. Sert au compteur de la barre laterale. */
export const nombreQuiAttendent = derived(etatsAgents, ($etats) =>
  [...$etats.values()].filter((e) => e === "attend").length,
);

function recalculer(): void {
  const liste = get(terminals);
  const maintenant = Date.now();
  const sortie = new Map<number, EtatAgent>();
  const vus = new Set<number>();

  for (const t of liste) {
    vus.add(t.id);
    const suivant = prochainEtat(suivis.get(t.id), t.llm, sortieDe(t.id), maintenant);
    suivis.set(t.id, suivant);
    if (meriteUnRepere(suivant.etat)) sortie.set(t.id, suivant.etat);
  }
  // Un terminal disparu de la liste ne doit pas garder un suivi pour toujours.
  for (const id of [...suivis.keys()]) if (!vus.has(id)) suivis.delete(id);

  // **ON N'ECRIT QUE SI QUELQUE CHOSE A CHANGE.** Reposer une Map identique chaque seconde
  // ferait recalculer la barre laterale pour rien, soixante fois par minute.
  const avant = get(etatsAgents);
  if (avant.size !== sortie.size || [...sortie].some(([id, e]) => avant.get(id) !== e)) {
    etatsAgents.set(sortie);
  }
}

/**
 * Le repere d'un terminal s'efface quand on va le voir.
 *
 * C'est le seul geste qui le retire : l'information « il a fini » ne doit pas s'evaporer
 * toute seule, sinon on la rate en regardant ailleurs.
 */
export function marquerVu(id: number): void {
  const suivi = suivis.get(id);
  if (suivi) suivis.set(id, { ...suivi, etat: suivi.etat === "fini" ? "aucun" : suivi.etat });
  const avant = get(etatsAgents);
  if (!avant.has(id)) return;
  const apres = new Map(avant);
  apres.delete(id);
  etatsAgents.set(apres);
}

let minuteur: ReturnType<typeof setInterval> | null = null;

/// Demarre le suivi. Le minuteur ne tourne QUE s'il y a un agent quelque part.
terminals.subscribe((liste) => {
  const besoin = liste.some((t) => t.llm) || suivis.size > 0;
  if (besoin && minuteur === null) {
    minuteur = setInterval(recalculer, PERIODE_MS);
    recalculer();
  } else if (!besoin && minuteur !== null) {
    clearInterval(minuteur);
    minuteur = null;
    if (get(etatsAgents).size > 0) etatsAgents.set(new Map());
  } else if (besoin) {
    // La liste vient de changer (un agent est apparu ou parti) : on ne fait pas attendre
    // l'utilisateur jusqu'au prochain tour.
    recalculer();
  }
});
