<script lang="ts">
  /**
   * Les ressources du namespace : deux courbes, et les pods du plus gourmand au moins gourmand.
   *
   * **CE QU'ON NE SAIT PAS MESURER N'EST PAS DESSINE.** Le stockage et le reseau vivent dans un
   * Prometheus, que Cockpit n'interroge pas : deux courbes plates a leur place laisseraient
   * croire qu'il ne se passe rien. On le DIT, une fois, en bas de l'ecran.
   *
   * **L'HISTORIQUE COMMENCE A L'OUVERTURE**, parce que le cluster ne rend qu'un instantane.
   * L'ecran l'annonce plutot que de laisser croire a un service qui vient de demarrer.
   */
  import { trad } from "../../i18n";
  import Courbe from "./Courbe.svelte";
  import { formaterCpu, formaterRam, type Pod } from "../../k8s/vue";
  import {
    couverture, dureeCourte, fenetre, lesPlusGourmands, periodesOffertes,
    RAFRAICHISSEMENTS, TOTAL, type Historique, type Mesure,
  } from "../../k8s/mesures";

  interface Props {
    pods: Pod[];
    historique: Historique;
    fenetreSecondes: number;
    rafraichissement: number;
    surFenetre: (secondes: number) => void;
    surRafraichissement: (secondes: number) => void;
    /// Le pod suivi de pres, ou `null` pour tout le namespace.
    focus: string | null;
    surFocus: (nom: string | null) => void;
  }
  let {
    pods, historique, fenetreSecondes, rafraichissement,
    surFenetre, surRafraichissement, focus, surFocus,
  }: Props = $props();

  let trierSur: "cpu" | "ram" = $state("cpu");
  let maintenant = $state(Date.now());
  $effect(() => {
    const minuteur = setInterval(() => (maintenant = Date.now()), 1000);
    return () => clearInterval(minuteur);
  });

  /// Depuis combien de temps cet ecran mesure : c'est tout ce qu'on peut montrer.
  const couvert = $derived(couverture(historique.get(TOTAL) ?? [], maintenant));
  const offertes = $derived(periodesOffertes(couvert));
  /// Une periode plus longue que ce qu'on a se ramene a ce qu'on a : on ne dessine pas du vide.
  const utile = $derived(Math.min(fenetreSecondes, Math.max(60, couvert)));
  const depuis = $derived(maintenant - utile * 1000);
  const serie = $derived(fenetre(historique.get(focus ?? TOTAL), depuis));
  /// Les mesures du dernier tour, pour le classement.
  const mesures = $derived<Mesure[]>(
    pods
      .filter((p) => p.cpu !== null || p.ram !== null)
      .map((p) => ({ nom: p.nom, cpu: p.cpu ?? 0, ram: p.ram ?? 0 })),
  );
  const classement = $derived(lesPlusGourmands(mesures, trierSur, 15));
  const plusGros = $derived(Math.max(1, ...classement.map((m) => m[trierSur])));
  const suivi = $derived(focus ? pods.find((p) => p.nom === focus) : undefined);
</script>

<div class="ressources">
  <div class="reglages">
    {#if focus}
      <button class="fil" onclick={() => surFocus(null)}>
        ← {$trad("k8s.toutLeNamespace")}
      </button>
      <span class="focus">{focus}</span>
    {:else}
      <span class="focus">{$trad("k8s.toutLeNamespace")}</span>
    {/if}

    <span class="espace"></span>

    <label class="choix">
      {$trad("k8s.fenetre")}
      <select class="input petit" value={fenetreSecondes} onchange={(e) => surFenetre(Number(e.currentTarget.value))}>
        {#each offertes as f (f)}
          <option value={f}>{dureeCourte(f)}</option>
        {/each}
      </select>
      <span class="couvert">{$trad("k8s.depuisOuvertureN", { duree: dureeCourte(couvert) })}</span>
    </label>

    <label class="choix">
      {$trad("k8s.rafraichissement")}
      <select class="input petit" value={rafraichissement} onchange={(e) => surRafraichissement(Number(e.currentTarget.value))}>
        {#each RAFRAICHISSEMENTS as r (r)}
          <option value={r}>{dureeCourte(r)}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="courbes">
    <Courbe
      serie={serie}
      valeur="cpu"
      titre={$trad("k8s.cpu")}
      formater={(n) => formaterCpu(Math.round(n))}
      teinte="accent"
      {depuis}
      jusqua={maintenant}
    />
    <Courbe
      serie={serie}
      valeur="ram"
      titre={$trad("k8s.ram")}
      formater={(n) => formaterRam(Math.round(n))}
      teinte="succes"
      {depuis}
      jusqua={maintenant}
    />
  </div>

  {#if suivi}
    <div class="faits">
      <span>{suivi.etat}</span>
      <span>{suivi.groupe}</span>
      {#if suivi.machine}<span>⌗ {suivi.machine}</span>{/if}
      {#if suivi.redemarrages > 0}<span class="alerte">⟳ {suivi.redemarrages}</span>{/if}
    </div>
  {/if}

  <div class="classement">
    <div class="classement-tete">
      <span class="titre">{$trad("k8s.lesPlusGourmands")}</span>
      <span class="bascules">
        <button class="bascule" class:actif={trierSur === "cpu"} onclick={() => (trierSur = "cpu")}>
          {$trad("k8s.cpu")}
        </button>
        <button class="bascule" class:actif={trierSur === "ram"} onclick={() => (trierSur = "ram")}>
          {$trad("k8s.ram")}
        </button>
      </span>
    </div>

    {#if classement.length === 0}
      <p class="vide">{$trad("k8s.sansMesures")}</p>
    {/if}

    {#each classement as m (m.nom)}
      <button class="rang" class:choisi={focus === m.nom} onclick={() => surFocus(focus === m.nom ? null : m.nom)}>
        <span class="rang-nom">{m.nom}</span>
        <span class="rang-jauge">
          <span
            class="rang-part {trierSur}"
            style="width:{Math.round((m[trierSur] / plusGros) * 100)}%"
          ></span>
        </span>
        <span class="rang-valeur">{trierSur === "cpu" ? formaterCpu(m.cpu) : formaterRam(m.ram)}</span>
      </button>
    {/each}
  </div>

  <p class="note">{$trad("k8s.mesuresDepuisOuverture")}</p>
</div>

<style>
  .ressources {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    overflow: auto;
    padding-right: 0.2rem;
  }
  .ressources > * { flex: none; }

  .reglages { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .espace { flex: 1; }
  .focus { color: var(--text-primary); font-weight: 600; font-size: 0.9rem; }
  .fil {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0;
  }
  .fil:hover { text-decoration: underline; }
  .couvert { color: var(--text-muted); font-size: 0.72rem; }
  .choix { display: flex; align-items: center; gap: 0.35rem; color: var(--text-muted); font-size: 0.76rem; }
  .petit { width: auto; padding: 0.22rem 0.4rem; font-size: 0.78rem; }

  .courbes { display: grid; grid-template-columns: 1fr 1fr; gap: 0.8rem; }
  @media (max-width: 900px) {
    .courbes { grid-template-columns: 1fr; }
  }

  .faits {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    color: var(--text-muted);
    font-size: 0.76rem;
  }
  .faits .alerte { color: var(--warning); }

  .classement {
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.5rem;
    background: var(--bg-secondary);
  }
  .classement-tete {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0 0.15rem 0.35rem;
  }
  .classement-tete .titre { color: var(--text-secondary); font-size: 0.78rem; }
  .bascules { display: flex; gap: 0.2rem; }
  .bascule {
    padding: 0.12rem 0.55rem;
    background: none;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 0.72rem;
  }
  .bascule.actif { color: var(--text-primary); border-color: var(--accent); background: var(--accent-soft); }

  .rang {
    display: grid;
    grid-template-columns: minmax(8rem, 22rem) 1fr 4.2rem;
    align-items: center;
    gap: 0.6rem;
    width: 100%;
    padding: 0.28rem 0.3rem;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }
  .rang:hover { background: var(--bg-tertiary); }
  .rang.choisi { border-color: var(--accent); background: var(--accent-soft); }
  .rang-nom {
    font-family: var(--font-mono);
    font-size: 0.74rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rang-jauge {
    height: 7px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    overflow: hidden;
  }
  .rang-part { display: block; height: 100%; border-radius: 999px; }
  .rang-part.cpu { background: var(--accent); }
  .rang-part.ram { background: var(--success); }
  .rang-valeur {
    color: var(--text-secondary);
    font-size: 0.74rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .vide, .note { color: var(--text-muted); font-size: 0.76rem; margin: 0; }
  .vide { text-align: center; padding: 1rem 0; }
</style>
