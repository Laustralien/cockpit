<script lang="ts">
  /**
   * Une ligne de pod, la meme partout.
   *
   * Elle s'affiche dans la vue des pods ET dans le detail d'un objet declare. Un seul fichier,
   * parce que deux copies divergent : la premiere version du detail avait perdu le bouton des
   * logs et le nombre de redemarrages.
   */
  import { trad } from "../../i18n";
  import { age, formaterCpu, formaterRam, type Pod } from "../../k8s/vue";

  let {
    pod, choisi = false, maintenant, kubectl = false, surOuvrirPod, surShell, surSupprimer,
  }: {
    pod: Pod;
    choisi?: boolean;
    maintenant: number;
    kubectl?: boolean;
    surOuvrirPod: (pod: Pod, volet?: "logs" | "evenements" | "yaml") => void;
    surShell?: (pod: Pod) => void;
    /** Absent : le geste n'est pas propose. Present : il passe par une confirmation nommee. */
    surSupprimer?: (pod: Pod) => void;
  } = $props();

  /// La couleur suit l'ETAT, jamais le texte affiche : celui-ci change avec la langue.
  const couleur = $derived(
    pod.ennuyeux ? "mauvais" : pod.etat === "Running" ? "bon" : pod.etat === "Succeeded" ? "fini" : "attente",
  );
</script>

<div class="pod" class:choisi>
  <button class="ligne" onclick={() => surOuvrirPod(pod)}>
    <span class="pastille {couleur}"></span>
    <span class="pod-nom">{pod.nom}</span>
    <span class="pod-etat" class:mauvais={pod.ennuyeux}>{pod.etat}</span>
    <span class="pod-age">{age(pod.depuis, maintenant)}</span>
    {#if pod.redemarrages > 0}
      <span class="redemarrages" title={$trad("k8s.redemarrages")}>⟳ {pod.redemarrages}</span>
    {/if}
    <span class="espace"></span>
    <span class="mesure">{formaterCpu(pod.cpu)}</span>
    <span class="mesure">{formaterRam(pod.ram)}</span>
  </button>
  <span class="actions">
    <button class="btn small ghost" onclick={() => surOuvrirPod(pod, "logs")}>{$trad("k8s.logs")}</button>
    {#if kubectl && surShell}
      <button class="btn small ghost" onclick={() => surShell(pod)}>{$trad("k8s.shell")}</button>
    {/if}
    {#if surSupprimer}
      <button
        class="btn small ghost danger"
        title={$trad("k8s.supprimerAide")}
        onclick={() => surSupprimer(pod)}
      >{$trad("k8s.supprimer")}</button>
    {/if}
  </span>
</div>

<style>
  .pod {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    border-radius: var(--radius-sm);
  }
  .pod:hover { background: var(--bg-tertiary); }
  .pod.choisi { background: var(--bg-tertiary); box-shadow: inset 2px 0 0 var(--accent); }

  .ligne {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.25rem 0.4rem;
    background: none;
    border: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font-size: 0.78rem;
  }
  .pastille { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .pastille.bon { background: var(--success); }
  .pastille.mauvais { background: var(--error); }
  .pastille.fini { background: var(--text-muted); }
  .pastille.attente { background: var(--warning); }

  .pod-nom {
    font-family: var(--font-mono);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Une colonne etroite ne doit pas couper « 2 h » en deux lignes : ces libelles ne se
     replient pas, c'est le NOM du pod qui cede la place. */
  .pod-etat, .pod-age, .redemarrages { flex: none; white-space: nowrap; }
  .pod-etat { color: var(--text-muted); font-size: 0.72rem; }
  .pod-etat.mauvais { color: var(--error); }
  .pod-age, .redemarrages { color: var(--text-muted); font-size: 0.72rem; }
  .espace { flex: 1; }
  .mesure {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--text-muted);
    min-width: 3.4rem;
    text-align: right;
  }
  .actions { display: flex; gap: 0.2rem; opacity: 0; flex: none; padding-right: 0.3rem; }
  /* Le geste qui detruit se distingue des autres AVANT le clic, pas dans la confirmation. */
  .actions :global(.danger) { color: var(--error); }
  .pod:hover .actions, .pod.choisi .actions { opacity: 1; }
</style>
