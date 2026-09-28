import type { LiaisonJira, TicketJira } from "../api/jira";

/**
 * Le rangement des tickets, sans dependance : teste par `scripts/tests/jira-tickets.test.mjs`.
 */

/** Jira ne renvoie pas les termines ; on les ecarte quand meme si l'un passe. */
export function grouperParCategorie(tickets: TicketJira[]): { enCours: TicketJira[]; aFaire: TicketJira[] } {
  return {
    enCours: tickets.filter((t) => t.categorie_statut === "indeterminate"),
    aFaire: tickets.filter((t) => t.categorie_statut !== "indeterminate" && t.categorie_statut !== "done"),
  };
}

/** Le premier projet Cockpit dont les cles contiennent celle du projet Jira du ticket. */
export function liaisonDuTicket(ticket: TicketJira, liaisons: LiaisonJira[]): LiaisonJira | null {
  return liaisons.find((l) => l.cles.includes(ticket.projet)) ?? null;
}

/**
 * `""` : tous mes tickets (`null`). Un projet : ses cles — **LISTE VIDE S'IL N'EN A PAS**, que le
 * backend traduit par « aucun ticket », jamais par « tous ».
 */
export function clesDuFiltre(filtre: string, liaisons: LiaisonJira[]): string[] | null {
  if (!filtre) return null;
  return liaisons.find((l) => l.projet === filtre)?.cles ?? [];
}
