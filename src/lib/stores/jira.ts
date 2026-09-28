import { writable } from "svelte/store";
import { jiraLiaisons, type LiaisonJira } from "../api/jira";

/** Les projets Cockpit lies a Jira. Sert a afficher l'onglet Jira d'un projet ou non. */
export const liaisonsJira = writable<LiaisonJira[]>([]);

export async function chargerLiaisonsJira(): Promise<void> {
  try {
    liaisonsJira.set(await jiraLiaisons());
  } catch {
    // Jira non configure ou base indisponible : aucun onglet, rien a signaler ici.
    liaisonsJira.set([]);
  }
}
