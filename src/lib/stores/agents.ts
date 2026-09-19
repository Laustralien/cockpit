import { derived, get, writable } from "svelte/store";
import { terminals } from "./terminals";
import { attachTerminal } from "../api/workspace";
import { signalerErreur } from "./errors";
import {
  doitObserver,
  noterUneSortie,
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

/**
 * Les terminaux actuellement SOUS LES YEUX : ceux des volets affiches, quand l'onglet des
 * terminaux est ouvert. Ils ne portent aucun repere (voir `prochainEtat`).
 */
const regardes = new Set<number>();

/** L'etat par identifiant de terminal. Seuls ceux qui meritent un repere y figurent. */
export const etatsAgents = writable<Map<number, EtatAgent>>(new Map());

/** Combien de terminaux reclament quelque chose. Sert au compteur de la barre laterale. */
export const nombreQuiAttendent = derived(etatsAgents, ($etats) =>
  [...$etats.values()].filter((e) => e === "attend").length,
);

/**
 * Les sessions sur lesquelles on s'est deja branche pour les OBSERVER.
 *
 * **UN TERMINAL JAMAIS OUVERT N'ENVOIE RIEN, DONC NE SIGNALE RIEN.** La sortie d'un terminal
 * n'arrive au frontend que s'il est branche, et il ne l'est qu'a l'ouverture de son onglet :
 * apres un lancement de Cockpit, un agent qui attend dans un terminal qu'on n'a pas encore
 * regarde ne pouvait donc rien dire. On se branche sur ces sessions-la, une fois chacune.
 *
 * Trois precautions, toutes obligatoires : on ne se branche QUE sur une session que le service
 * dit vivante (se brancher sur une session inconnue ROUVRIRAIT un shell, ce que le demarrage ne
 * doit jamais faire), on envoie SA taille et pas une autre (sinon on redimensionne un terminal
 * qu'on n'affiche meme pas), et seulement quand un agent y tourne — sinon on paierait l'ecran et
 * l'historique de chaque terminal de chaque projet pour rien.
 */
const observes = new Set<number>();

function observer(t: { id: number; llm: boolean; alive: boolean; cols: number; rows: number }): void {
  if (!doitObserver(t, observes.has(t.id), sortieDe(t.id) !== undefined)) return;
  observes.add(t.id);
  attachTerminal(t.id, t.cols, t.rows)
    // **ON COMMENCE A COMPTER LE SILENCE ICI.** Sans cet instant de depart, le terminal reste
    // « on n'a jamais rien vu passer », donc « en cours » pour toujours : le seul octet qui
    // arrive ensuite est le redessin du branchement, et un redessin ne dit rien de l'agent.
    // On ne pretend pas connaitre son passe, on mesure son silence a partir du moment ou on
    // l'ecoute.
    .then(() => noterUneSortie(t.id))
    .catch((e) => {
      observes.delete(t.id);
      signalerErreur("agents.observer", String(e));
    });
}

function recalculer(): void {
  const liste = get(terminals);
  const maintenant = Date.now();
  const sortie = new Map<number, EtatAgent>();
  const vus = new Set<number>();

  for (const t of liste) {
    vus.add(t.id);
    // Un agent tourne ici et on n'a jamais rien vu passer : on se branche pour pouvoir
    // repondre autre chose que « en cours ».
    observer(t);
    const suivant = prochainEtat(
      suivis.get(t.id), t.llm, sortieDe(t.id), maintenant, undefined, regardes.has(t.id),
    );
    suivis.set(t.id, suivant);
    if (meriteUnRepere(suivant.etat)) sortie.set(t.id, suivant.etat);
  }
  // Un terminal disparu de la liste ne doit pas garder un suivi pour toujours.
  for (const id of [...suivis.keys()]) if (!vus.has(id)) suivis.delete(id);
  for (const id of [...observes]) if (!vus.has(id)) observes.delete(id);

  // **ON N'ECRIT QUE SI QUELQUE CHOSE A CHANGE.** Reposer une Map identique chaque seconde
  // ferait recalculer la barre laterale pour rien, soixante fois par minute.
  const avant = get(etatsAgents);
  if (avant.size !== sortie.size || [...sortie].some(([id, e]) => avant.get(id) !== e)) {
    etatsAgents.set(sortie);
  }
}

/**
 * Dit quels terminaux sont sous les yeux. L'onglet des terminaux l'annonce, et annonce une
 * liste VIDE en partant.
 *
 * **CE N'EST PAS « MARQUER COMME VU », ET LA NUANCE EST TOUT LE CORRECTIF.** Effacer le repere
 * au clic faisait disparaitre le cadre puis revenir une seconde plus tard, parce que l'agent
 * attendait toujours : « il attend » est un etat COURANT, il ne se marque pas comme lu. Ici
 * c'est le fait de REGARDER qui fait taire le repere, et il revient tout seul quand on part —
 * sauf « il a fini », qui est un evenement passe et se consomme pour de bon.
 *
 * Tous les volets affiches comptent, pas seulement celui qui a le focus : ils sont a l'ecran.
 */
export function regarderLesTerminaux(ids: Iterable<number>): void {
  const avant = [...regardes].sort().join();
  regardes.clear();
  for (const id of ids) regardes.add(id);
  if ([...regardes].sort().join() === avant) return;
  // Sans ce tour immediat, le repere du terminal qu'on vient d'ouvrir resterait affiche
  // jusqu'a une seconde de plus.
  if (minuteur !== null) recalculer();
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
