/**
 * Le pourcentage affiche pendant le telechargement d'une mise a jour.
 *
 * **LA BARRE MONTAIT A 300 %** (signale le 2026-09-30) : elle additionnait les morceaux
 * qu'annonce electron-updater, et ces morceaux ne s'additionnent pas proprement quand il ne
 * telecharge que la difference entre deux versions. On lit donc le TOTAL recu qu'il donne
 * lui-meme, et le pourcentage reste entre 0 et 100 quoi qu'il annonce.
 */
export function pourcentage(recu: number, total: number | null): number | null {
  if (!total || total <= 0 || !Number.isFinite(recu)) return null;
  return Math.max(0, Math.min(100, Math.round((recu / total) * 100)));
}
