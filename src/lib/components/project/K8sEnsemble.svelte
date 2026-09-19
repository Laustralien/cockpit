<script lang="ts">
  /**
   * Ce que le namespace contient, en un ecran.
   *
   * **CHAQUE NOMBRE DIT CE QU'IL COMPTE.** Des objets declares d'un cote, des pods de l'autre,
   * et jamais les uns presentes comme les autres : l'ecran a deja annonce « taches planifiees
   * 264 » a quelqu'un qui en declare 79, et le premier reflexe a ete de croire que Cockpit ne
   * trouvait pas tout.
   */
  import { trad } from "../../i18n";
  import { age, type Element, type Ensemble } from "../../k8s/vue";

  let {
    ensemble, aVoir, maintenant, surVue,
  }: {
    ensemble: Ensemble;
    aVoir: Element[];
    maintenant: number;
    surVue: (vue: "services" | "taches" | "pods") => void;
  } = $props();

  /// Au-dela, la liste des ennuis devient une liste de plus a parcourir.
  const ENNUIS_MAX = 8;
</script>

<div class="ensemble">
  <div class="cartes">
    <button class="carte" onclick={() => surVue("services")}>
      <span class="chiffre">{ensemble.services}</span>
      <span class="libelle">{$trad("k8s.ensServices")}</span>
      <span class="sous">{$trad("k8s.ensDeclares")}</span>
    </button>

    <button class="carte" onclick={() => surVue("taches")}>
      <span class="chiffre">{ensemble.taches}</span>
      <span class="libelle">{$trad("k8s.ensTaches")}</span>
      <span class="sous">
        {#if ensemble.tachesJamaisLancees > 0}
          {$trad("k8s.ensJamaisN", { n: ensemble.tachesJamaisLancees })}
        {:else if ensemble.tachesSuspendues > 0}
          {$trad("k8s.ensSuspenduesN", { n: ensemble.tachesSuspendues })}
        {:else}
          {$trad("k8s.ensDeclares")}
        {/if}
      </span>
    </button>

    <button class="carte" onclick={() => surVue("pods")}>
      <span class="chiffre">{ensemble.pods}</span>
      <span class="libelle">{$trad("k8s.ensPods")}</span>
      <span class="sous">
        {$trad("k8s.ensEnMarcheN", { n: ensemble.podsEnMarche })} ·
        {$trad("k8s.ensTermines", { n: ensemble.podsTermines })}
      </span>
    </button>

    <button class="carte" class:alerte={ensemble.aVoir > 0} onclick={() => surVue("pods")}>
      <span class="chiffre">{ensemble.aVoir}</span>
      <span class="libelle">{$trad("k8s.ensAVoir")}</span>
      <!-- Le chiffre compte des OBJETS, la ligne du dessous des pods : elle le dit. -->
      <span class="sous">{$trad("k8s.ensPodsEnEchecN", { n: ensemble.podsEnnuyeux })}</span>
    </button>
  </div>

  {#if aVoir.length > 0}
    <section class="ennuis">
      <h4>{$trad("k8s.ensCeQuiCloche")}</h4>
      {#each aVoir.slice(0, ENNUIS_MAX) as e (e.sorte + e.nom)}
        <div class="ennui">
          <span class="point"></span>
          <span class="nom">{e.nom}</span>
          <span class="sorte">{e.sorte}</span>
          <span class="espace"></span>
          {#if e.sorte === "CronJob"}
            <span class="etat">{e.dernier ? age(e.dernier, maintenant) : $trad("k8s.jamaisLance")}</span>
          {:else}
            <span class="etat">{e.prets}/{e.voulus}</span>
          {/if}
        </div>
      {/each}
      {#if aVoir.length > ENNUIS_MAX}
        <p class="reste">{$trad("k8s.ensEtNAutres", { n: aVoir.length - ENNUIS_MAX })}</p>
      {/if}
    </section>
  {:else}
    <p class="calme">{$trad("k8s.ensToutVaBien")}</p>
  {/if}
</div>

<style>
  .ensemble { display: flex; flex-direction: column; gap: 0.9rem; }
  .ensemble > * { flex: none; }

  .cartes { display: grid; grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); gap: 0.6rem; }
  .carte {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.8rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .carte:hover { border-color: var(--accent); }
  .carte.alerte { border-color: var(--error); }
  .chiffre { font-size: 1.7rem; font-weight: 600; line-height: 1.1; }
  .libelle { font-size: 0.82rem; }
  .sous { font-size: 0.7rem; color: var(--text-muted); }

  .ennuis {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    padding: 0.6rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
  }
  h4 { margin: 0 0 0.3rem; font-size: 0.8rem; color: var(--text-secondary); }
  .ennui { display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; padding: 0.15rem 0; }
  .point { width: 6px; height: 6px; border-radius: 50%; background: var(--error); flex: none; }
  .nom { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sorte, .etat, .reste { font-size: 0.7rem; color: var(--text-muted); }
  .espace { flex: 1; }
  .reste { margin: 0.3rem 0 0; }
  .calme { margin: 0; font-size: 0.82rem; color: var(--text-muted); }
</style>
