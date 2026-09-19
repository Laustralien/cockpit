/**
 * Ou en est l'agent d'un terminal : il travaille, il attend une reponse, il a rendu la main.
 *
 * **CE QU'ON SAIT SANS RIEN DEMANDER A PERSONNE.** Le service dit deja qu'un agent tourne dans
 * une session (`llm`), et la sortie de chaque terminal attache arrive au frontend. Un agent qui
 * travaille ECRIT en permanence (ne serait-ce que son indicateur d'attente) ; un agent qui
 * attend une reponse laisse l'ecran fige. Le silence est donc le signal, et il ne coute aucun
 * aller-retour, aucune version de protocole, aucun terminal detache perdu.
 *
 * **CE QUE CE MODULE NE FAIT PAS : du travail par octet recu.** La regle du projet est que rien
 * ne s'ajoute entre la sortie et xterm. Noter un instant dans une table ordinaire coute quelques
 * nanosecondes et ne declenche AUCUN rendu ; c'est la lecture qui est echantillonnee, une fois
 * par seconde, et seulement quand un agent tourne quelque part.
 */

/** Ce qu'on affiche a cote d'un terminal. */
export type EtatAgent = "aucun" | "en-cours" | "attend" | "fini";

export interface Suivi {
  /** L'agent tournait-il au passage precedent ? Sert a reconnaitre qu'il vient de finir. */
  agentAvant: boolean;
  /** L'etat retenu. Il SURVIT tant qu'on n'est pas retourne voir le terminal. */
  etat: EtatAgent;
}

/**
 * Au-dela de ce silence, un agent qui tourne est considere comme en attente.
 *
 * Trois secondes : les CLI d'agent animent leur indicateur pendant qu'ils travaillent, donc
 * une seconde suffirait a les distinguer. On prend de la marge pour ne pas annoncer « il
 * t'attend » sur un simple a-coup — une fausse alerte fait traverser l'ecran pour rien, et on
 * cesse alors de croire le repere.
 */
export const SILENCE_MS = 3000;

/**
 * L'etat suivant d'un terminal.
 *
 * `derniereSortie` absent veut dire « on n'a jamais rien vu passer » : le terminal n'a pas
 * encore ete ouvert dans cette session de l'application. On repond alors « en cours » et jamais
 * « il attend » — **envoyer quelqu'un vers un terminal qui travaille est pire que de se taire.**
 *
 * **`regarde` VEUT DIRE « CE TERMINAL EST SOUS LES YEUX », ET IL NE PORTE AUCUN REPERE.**
 * Premiere version : le repere s'effacait au CLIC. Mais « il attend » est un ETAT COURANT, pas
 * un evenement passe : une seconde plus tard le calcul le retrouvait vrai et le remettait, donc
 * le cadre disparaissait puis revenait sous les yeux de l'utilisateur. Un terminal qu'on
 * REGARDE n'a rien a signaler — on y voit deja tout — et le repere revient de lui-meme des
 * qu'on regarde ailleurs, ce qui est exactement ce qu'on veut.
 */
export function prochainEtat(
  suivi: Suivi | undefined,
  llm: boolean,
  derniereSortie: number | undefined,
  maintenant: number,
  silenceMs: number = SILENCE_MS,
  regarde: boolean = false,
): Suivi {
  if (llm) {
    const silencieux = derniereSortie !== undefined && maintenant - derniereSortie >= silenceMs;
    return { agentAvant: true, etat: regarde ? "aucun" : silencieux ? "attend" : "en-cours" };
  }
  // L'agent tournait au passage precedent et ne tourne plus : il vient de rendre la main.
  // Le regarder a cet instant CONSOMME l'information : elle a ete vue.
  if (suivi?.agentAvant) return { agentAvant: false, etat: regarde ? "aucun" : "fini" };
  // « Fini » reste affiche jusqu'a ce qu'on retourne voir le terminal : c'est justement
  // l'information qu'on a demandee, elle ne doit pas s'effacer toute seule.
  const garde = suivi?.etat === "fini" && !regarde;
  return { agentAvant: false, etat: garde ? "fini" : "aucun" };
}

/**
 * Faut-il se brancher sur cette session pour l'OBSERVER ?
 *
 * **UN TERMINAL JAMAIS OUVERT N'ENVOIE RIEN, DONC NE SIGNALE RIEN.** Sa sortie n'arrive au
 * frontend que s'il est branche, et il ne l'est qu'a l'ouverture de son onglet : apres un
 * lancement de Cockpit, un agent qui attend dans un terminal qu'on n'a pas encore regarde
 * restait muet. On se branche donc sur ces sessions-la, une seule fois chacune.
 *
 * Quatre conditions, toutes necessaires :
 *  - un agent y tourne (sinon on paierait l'ecran et l'historique de chaque terminal de chaque
 *    projet pour rien) ;
 *  - le service la dit VIVANTE : se brancher sur une session qu'il ne connait pas ROUVRIRAIT un
 *    shell, ce qu'un demarrage ne doit jamais faire ;
 *  - on n'a encore rien vu passer, sinon c'est deja fait ;
 *  - sa taille est connue : on renvoie la SIENNE, sinon on redimensionne un terminal qu'on
 *    n'affiche meme pas, et une application plein ecran ne s'en remet pas.
 */
export function doitObserver(
  terminal: { llm: boolean; alive: boolean; cols: number; rows: number },
  dejaObserve: boolean,
  sortieConnue: boolean,
): boolean {
  if (dejaObserve || sortieConnue) return false;
  if (!terminal.llm || !terminal.alive) return false;
  return terminal.cols > 0 && terminal.rows > 0;
}

/** Un etat qui merite un repere a l'ecran. « En cours » n'en est pas un : c'est l'ordinaire. */
export function meriteUnRepere(etat: EtatAgent): boolean {
  return etat === "attend" || etat === "fini";
}

// --- Le suivi, cote application ------------------------------------------------------------

/**
 * Le dernier instant ou chaque terminal a ecrit quelque chose.
 *
 * **UNE TABLE ORDINAIRE, PAS UN MAGASIN REACTIF.** Elle est ecrite a chaque sortie de
 * terminal, c'est-a-dire des milliers de fois par seconde pendant une compilation : un magasin
 * y declencherait autant de rendus. La lecture, elle, est echantillonnee (voir `stores/agents`).
 */
const derniereSortie = new Map<number, number>();

/** Appele sur le chemin de la sortie : deux operations, aucune allocation, aucun rendu. */
export function noterUneSortie(id: number): void {
  derniereSortie.set(id, Date.now());
}

export function sortieDe(id: number): number | undefined {
  return derniereSortie.get(id);
}

/** Un terminal ferme n'a plus rien a dire : sa ligne s'en va avec lui. */
export function oublierLeTerminal(id: number): void {
  derniereSortie.delete(id);
}
